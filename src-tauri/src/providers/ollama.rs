//! Ollama 로컬 서버 어댑터: `/api/tags`, `/api/ps`, `/api/show`, `/api/generate`, `/api/version`, `/api/chat`.
//!
//! 채팅은 OpenAI 호환 `/v1/chat/completions` 대신 고유 `/api/chat`으로 보낸다. OpenAI 호환 경로에는
//! 컨텍스트 크기(`num_ctx`)를 넣을 방법이 없어, 모델이 기본 컨텍스트(보통 4096)로 다시 로드되고
//! 프롬프트 앞부분(시스템 프롬프트·도구)이 오류 없이 잘린다. 에이전트 루프는 OpenAI 모양 본문과
//! SSE 청크를 그대로 쓰고, 이 모듈의 순수 함수가 요청·스트림·응답을 서로 바꾼다.

use crate::lmstudio::{self, LmModel, LoadPlan, LoadedInstance, TurnModelInfo, DEFAULT_CONTEXT, MIN_CONTEXT};
use crate::providers::{api_error_message, friendly_error, local_origin, LlmProvider};
use crate::store::AppSettings;
use futures_util::stream::{self, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaUrls {
    pub chat: String,
    pub tags: String,
    pub ps: String,
    pub show: String,
    pub generate: String,
    pub version: String,
}

pub fn urls(server_url: &str) -> Result<OllamaUrls, String> {
    let base = local_origin(server_url, LlmProvider::Ollama)?;
    Ok(OllamaUrls {
        chat: format!("{base}/api/chat"),
        tags: format!("{base}/api/tags"),
        ps: format!("{base}/api/ps"),
        show: format!("{base}/api/show"),
        generate: format!("{base}/api/generate"),
        version: format!("{base}/api/version"),
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OllamaShowInfo {
    pub max_context_length: u32,
    pub vision: bool,
    pub tool_use: bool,
    pub reasoning_options: Vec<String>,
    pub is_embedding_only: bool,
}

const QUICK_TIMEOUT: Duration = Duration::from_secs(10);
const SHOW_TIMEOUT: Duration = Duration::from_secs(3);
const LOAD_TIMEOUT: Duration = Duration::from_secs(300);
const UNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
const START_POLL_TIMEOUT: Duration = Duration::from_secs(15);
/// 미리 로드(`load_model`)할 때 보내는 `keep_alive`. 채팅·요약 요청에는 넣지 않아 사용자의
/// `OLLAMA_KEEP_ALIVE`(서버 기본값)를 따른다. 같은 `num_ctx`만 보내면 모델을 다시 올리지 않는다.
pub const KEEP_ALIVE: &str = "5m";
/// 응답의 `prompt_eval_count`가 `num_ctx`에서 이만큼 안쪽까지 차면 넘침에 가까운 것으로 본다.
pub const NEAR_CONTEXT_MARGIN: u64 = 256;

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    super::local_http_client(timeout)
}

fn fallback_show(name: &str, families: &[String]) -> OllamaShowInfo {
    let lower = name.to_lowercase();
    let is_embedding_only = lower.contains("embed") || lower.starts_with("bge-") || lower.contains("/bge-");
    let vision = families
        .iter()
        .any(|f| matches!(f.to_lowercase().as_str(), "clip" | "mllama" | "vision"));
    OllamaShowInfo {
        max_context_length: 0,
        vision,
        tool_use: !is_embedding_only,
        reasoning_options: Vec::new(),
        is_embedding_only,
    }
}

/// `POST /api/show` 응답에서 최대 컨텍스트와 기능 배지를 읽는다.
pub fn parse_show(v: &Value, name: &str, families: &[String]) -> OllamaShowInfo {
    let mut info = if let Some(caps) = v.get("capabilities").and_then(|c| c.as_array()) {
        let has = |target: &str| caps.iter().any(|c| c.as_str() == Some(target));
        let is_embedding_only = has("embedding") && !has("completion");
        let reasoning_options = if has("thinking") {
            vec!["off".to_string(), "on".to_string()]
        } else {
            Vec::new()
        };
        OllamaShowInfo {
            max_context_length: 0,
            vision: has("vision"),
            tool_use: has("tools"),
            reasoning_options,
            is_embedding_only,
        }
    } else {
        fallback_show(name, families)
    };

    if let Some(model_info) = v.get("model_info").and_then(|m| m.as_object()) {
        for (k, val) in model_info {
            if k.ends_with(".context_length") {
                if let Some(ctx) = val.as_u64().filter(|&n| n > 0) {
                    info.max_context_length = ctx.min(u32::MAX as u64) as u32;
                    break;
                }
            }
        }
    }
    info
}

/// `GET /api/ps` 응답에서 현재 메모리에 로드된 모델 목록과 컨텍스트를 읽는다.
pub fn parse_ps(v: &Value) -> HashMap<String, LoadedInstance> {
    let mut out = HashMap::new();
    let Some(models) = v.get("models").and_then(|m| m.as_array()) else {
        return out;
    };
    for m in models {
        let Some(name) = m
            .get("name")
            .or_else(|| m.get("model"))
            .and_then(|n| n.as_str())
            .filter(|s| !s.trim().is_empty())
        else {
            continue;
        };
        let context_length = m
            .get("context_length")
            .and_then(|c| c.as_u64())
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;
        out.insert(
            name.to_string(),
            LoadedInstance {
                id: name.to_string(),
                context_length,
            },
        );
    }
    out
}

/// `/api/tags`, `/api/ps`, `/api/show` 결과를 합쳐 `LmModel` 목록을 만든다(임베딩 전용 모델은 제외).
pub fn build_models(
    tags: &Value,
    ps: &HashMap<String, LoadedInstance>,
    shows: &HashMap<String, OllamaShowInfo>,
) -> Vec<LmModel> {
    let Some(arr) = tags.get("models").and_then(|m| m.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            let name = m
                .get("name")
                .or_else(|| m.get("model"))
                .and_then(|s| s.as_str())
                .filter(|s| !s.trim().is_empty())?
                .to_string();
            let families: Vec<String> = m
                .pointer("/details/families")
                .and_then(|f| f.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let show = shows
                .get(&name)
                .cloned()
                .unwrap_or_else(|| fallback_show(&name, &families));
            if show.is_embedding_only {
                return None;
            }
            let loaded = ps.get(&name).cloned().into_iter().collect();
            Some(LmModel {
                key: name.clone(),
                display_name: name,
                params: m
                    .pointer("/details/parameter_size")
                    .and_then(|p| p.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(String::from),
                quantization: m
                    .pointer("/details/quantization_level")
                    .and_then(|q| q.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(String::from),
                size_bytes: m.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                max_context_length: show.max_context_length,
                loaded,
                vision: show.vision,
                tool_use: show.tool_use,
                reasoning_options: show.reasoning_options,
            })
        })
        .collect()
}

async fn fetch_ps(settings: &AppSettings, ps_url: &str) -> HashMap<String, LoadedInstance> {
    let Ok(c) = client(SHOW_TIMEOUT) else {
        return HashMap::new();
    };
    let Ok(resp) = lmstudio::authorize(c.get(ps_url), &settings.api_key).send().await else {
        return HashMap::new();
    };
    if !resp.status().is_success() {
        return HashMap::new();
    }
    let Ok(body) = resp.text().await else {
        return HashMap::new();
    };
    serde_json::from_str::<Value>(&body)
        .map(|v| parse_ps(&v))
        .unwrap_or_default()
}

async fn fetch_show(
    client: &reqwest::Client,
    show_url: &str,
    api_key: &str,
    name: &str,
    families: &[String],
) -> Option<(String, OllamaShowInfo)> {
    let resp = lmstudio::authorize(client.post(show_url).json(&json!({ "model": name })), api_key)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    let v: Value = serde_json::from_str(&body).ok()?;
    Some((name.to_string(), parse_show(&v, name, families)))
}

pub async fn list_models(settings: &AppSettings) -> Result<Vec<LmModel>, String> {
    let u = urls(&settings.server_url)?;
    let resp = lmstudio::authorize(client(QUICK_TIMEOUT)?.get(&u.tags), &settings.api_key)
        .send()
        .await
        .map_err(|e| friendly_error(LlmProvider::Ollama, &e))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| friendly_error(LlmProvider::Ollama, &e))?;
    if !status.is_success() {
        return Err(api_error_message(LlmProvider::Ollama, status.as_u16(), &body));
    }
    let tags: Value =
        serde_json::from_str(&body).map_err(|e| format!("Ollama 모델 목록을 읽지 못했습니다: {e}"))?;

    let entries: Vec<(String, Vec<String>)> = tags
        .get("models")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let name = m
                .get("name")
                .or_else(|| m.get("model"))
                .and_then(|s| s.as_str())
                .filter(|s| !s.trim().is_empty())?
                .to_string();
            let families = m
                .pointer("/details/families")
                .and_then(|f| f.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            Some((name, families))
        })
        .collect();

    let ps_fut = fetch_ps(settings, &u.ps);
    let show_client = client(SHOW_TIMEOUT)?;
    let show_url = u.show.clone();
    let api_key = settings.api_key.clone();
    let shows_fut = async move {
        let results: Vec<Option<(String, OllamaShowInfo)>> = stream::iter(entries)
            .map(|(name, families)| {
                let c = &show_client;
                let url = &show_url;
                let key = &api_key;
                async move { fetch_show(c, url, key, &name, &families).await }
            })
            .buffer_unordered(4)
            .collect()
            .await;
        results.into_iter().flatten().collect::<HashMap<_, _>>()
    };

    let (ps, shows) = tokio::join!(ps_fut, shows_fut);
    Ok(build_models(&tags, &ps, &shows))
}

pub async fn plan_load(settings: &AppSettings, key: &str) -> Result<LoadPlan, String> {
    let models = list_models(settings).await?;
    let wanted = models
        .iter()
        .find(|m| m.key == key)
        .map(|m| lmstudio::wanted_context(settings, m))
        .unwrap_or(DEFAULT_CONTEXT);
    Ok(lmstudio::plan_load(&models, key, wanted))
}

pub async fn load_model(
    settings: &AppSettings,
    key: &str,
    context_length: u32,
) -> Result<LoadedInstance, String> {
    let u = urls(&settings.server_url)?;
    let body = json!({
        "model": key,
        "prompt": "",
        "stream": false,
        "keep_alive": KEEP_ALIVE,
        "options": { "num_ctx": context_length }
    });
    let resp = lmstudio::authorize(client(LOAD_TIMEOUT)?.post(&u.generate).json(&body), &settings.api_key)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "모델 로드가 5분 안에 끝나지 않았습니다.".to_string()
            } else {
                friendly_error(LlmProvider::Ollama, &e)
            }
        })?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| friendly_error(LlmProvider::Ollama, &e))?;
    if !status.is_success() {
        return Err(api_error_message(LlmProvider::Ollama, status.as_u16(), &text));
    }
    let ps = fetch_ps(settings, &u.ps).await;
    let actual_ctx = ps
        .get(key)
        .map(|i| i.context_length)
        .filter(|&c| c > 0)
        .unwrap_or(context_length);
    Ok(LoadedInstance {
        id: key.to_string(),
        context_length: actual_ctx,
    })
}

pub async fn unload_instance(settings: &AppSettings, instance_id: &str) -> Result<(), String> {
    let u = urls(&settings.server_url)?;
    let body = json!({
        "model": instance_id,
        "prompt": "",
        "stream": false,
        "keep_alive": 0
    });
    let resp = lmstudio::authorize(
        client(UNLOAD_TIMEOUT)?.post(&u.generate).json(&body),
        &settings.api_key,
    )
    .send()
    .await
    .map_err(|e| friendly_error(LlmProvider::Ollama, &e))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let text = resp.text().await.unwrap_or_default();
    Err(api_error_message(LlmProvider::Ollama, status.as_u16(), &text))
}

pub async fn server_running(settings: &AppSettings) -> bool {
    let Ok(u) = urls(&settings.server_url) else {
        return false;
    };
    let Ok(c) = client(Duration::from_secs(3)) else {
        return false;
    };
    lmstudio::authorize(c.get(&u.version), &settings.api_key)
        .send()
        .await
        .is_ok()
}

/// Ollama 실행 파일을 찾는다 (`%LOCALAPPDATA%\Programs\Ollama\ollama.exe` 또는 `PATH`).
pub fn ollama_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let p = PathBuf::from(local).join("Programs").join("Ollama").join("ollama.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    let exe_name = if cfg!(target_os = "windows") { "ollama.exe" } else { "ollama" };
    find_on_path(&std::env::var_os("PATH")?, exe_name)
}

/// `PATH`에서 실행 파일을 찾는다. 상대 경로 항목(`.`, `bin` 등)은 앱의 현재 폴더 기준이 되어
/// 엉뚱한 파일을 실행할 수 있으므로 건너뛴다.
fn find_on_path(path_var: &std::ffi::OsStr, exe_name: &str) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(exe_name))
        .find(|candidate| candidate.is_file())
}

/// `ollama serve`에 줄 `OLLAMA_HOST`. 앱이 연결할 주소와 포트에서 서버가 열리게 한다
/// (포트가 없으면 http 기본 포트 80, `localhost`는 Ollama 기본처럼 127.0.0.1).
pub fn ollama_host_env(server_url: &str) -> Result<String, String> {
    let url = crate::providers::validate_local_url(server_url, LlmProvider::Ollama)?;
    let port = url.port_or_known_default().unwrap_or(80);
    let host = match url.host_str().unwrap_or("localhost") {
        "localhost" => "127.0.0.1",
        other => other,
    };
    Ok(format!("{host}:{port}"))
}

pub async fn start_server(settings: &AppSettings) -> Result<(), String> {
    if server_running(settings).await {
        return Ok(());
    }
    let bin = ollama_path().ok_or_else(|| {
        "Ollama 실행 파일을 찾지 못했습니다. Ollama를 설치한 뒤 다시 시도해 주세요.".to_string()
    })?;
    let host = ollama_host_env(&settings.server_url)?;
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("serve")
        .env("OLLAMA_HOST", host)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    cmd.spawn()
        .map_err(|e| format!("Ollama 서버를 시작하지 못했습니다: {e}"))?;

    let started = std::time::Instant::now();
    while started.elapsed() < START_POLL_TIMEOUT {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if server_running(settings).await {
            return Ok(());
        }
    }
    Err("Ollama 서버가 15초 안에 시작되지 않았습니다. Ollama를 직접 실행해 주세요.".to_string())
}

pub async fn turn_model_info(settings: &AppSettings, key: &str) -> TurnModelInfo {
    let pref_ctx = settings
        .model_prefs
        .get(key)
        .and_then(|p| p.context_length)
        .filter(|&c| c >= MIN_CONTEXT);
    let Ok(u) = urls(&settings.server_url) else {
        return TurnModelInfo {
            loaded_context: pref_ctx,
            display_name: (!key.trim().is_empty()).then(|| key.to_string()),
            ..Default::default()
        };
    };
    let Ok(c) = client(SHOW_TIMEOUT) else {
        return TurnModelInfo {
            loaded_context: pref_ctx,
            display_name: (!key.trim().is_empty()).then(|| key.to_string()),
            ..Default::default()
        };
    };

    let (ps, show) = tokio::join!(
        fetch_ps(settings, &u.ps),
        fetch_show(&c, &u.show, &settings.api_key, key, &[])
    );
    let ps_ctx = ps.get(key).map(|i| i.context_length).filter(|&c| c > 0);
    let show = show.map(|(_, s)| s);
    // 채팅 요청마다 이 값을 `num_ctx`로 보내므로, 로드되어 있지 않으면(쉬다 내려감) 다음 요청이
    // 이 값으로 다시 로드한다. 그래서 로드 전에도 앱이 원하는 컨텍스트(설정값, 없으면 기본값)를 쓴다.
    let max = show.as_ref().map_or(0, |s| s.max_context_length);
    let wanted = pref_ctx.unwrap_or(DEFAULT_CONTEXT);
    let wanted = if max > 0 { wanted.min(max) } else { wanted };
    let loaded_context = ps_ctx.or(Some(wanted));
    let reasoning_options = show.map(|s| s.reasoning_options).unwrap_or_default();
    TurnModelInfo {
        reasoning_options,
        loaded_context,
        display_name: (!key.trim().is_empty()).then(|| key.to_string()),
    }
}

// ---------------------------------------------------------------------------
// `/api/chat` 변환 (OpenAI 모양 ↔ Ollama 고유 형식)
// ---------------------------------------------------------------------------

/// OpenAI 모양 요청 항목 중 Ollama `options`로 옮길 것. `max_tokens`는 `num_predict`가 된다
/// (Ollama의 OpenAI 호환 계층이 하는 것과 같은 대응).
const OPTION_FIELDS: &[(&str, &str)] = &[
    ("temperature", "temperature"),
    ("top_p", "top_p"),
    ("presence_penalty", "presence_penalty"),
    ("frequency_penalty", "frequency_penalty"),
    ("seed", "seed"),
    ("stop", "stop"),
    ("max_tokens", "num_predict"),
];

/// 메시지 `content`(문자열·null·OpenAI 조각 배열)를 글과 base64 이미지로 나눈다.
fn split_content(content: Option<&Value>) -> (String, Vec<String>) {
    match content {
        Some(Value::String(s)) => (s.clone(), Vec::new()),
        Some(Value::Array(parts)) => {
            let mut text = Vec::new();
            let mut images = Vec::new();
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                            text.push(t.to_string());
                        }
                    }
                    Some("image_url") => {
                        let url = part
                            .pointer("/image_url/url")
                            .or_else(|| part.get("image_url"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        // Ollama는 data URL이 아니라 base64 본문만 받는다.
                        if let Some((_, b64)) = url.split_once(";base64,") {
                            images.push(b64.to_string());
                        }
                    }
                    _ => {}
                }
            }
            (text.join("\n"), images)
        }
        _ => (String::new(), Vec::new()),
    }
}

/// OpenAI 모양 도구 인자(JSON 문자열)를 Ollama가 받는 객체로 바꾼다. 못 읽으면 빈 객체.
fn arguments_object(args: Option<&Value>) -> Value {
    match args {
        Some(Value::String(s)) => serde_json::from_str::<Value>(s)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({})),
        Some(v @ Value::Object(_)) => v.clone(),
        _ => json!({}),
    }
}

fn native_message(m: &Value, call_names: &mut HashMap<String, String>) -> Value {
    let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
    let (text, images) = split_content(m.get("content"));
    let mut out = json!({ "role": role, "content": text });
    if !images.is_empty() {
        out["images"] = json!(images);
    }
    match role {
        "assistant" => {
            if let Some(r) = m
                .get("reasoning_content")
                .or_else(|| m.get("reasoning"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                out["thinking"] = json!(r);
            }
            if let Some(calls) = m.get("tool_calls").and_then(Value::as_array).filter(|c| !c.is_empty()) {
                let native: Vec<Value> = calls
                    .iter()
                    .map(|c| {
                        let name = c.pointer("/function/name").and_then(Value::as_str).unwrap_or("");
                        let mut call = json!({
                            "function": { "name": name, "arguments": arguments_object(c.pointer("/function/arguments")) }
                        });
                        if let Some(id) = c.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                            call_names.insert(id.to_string(), name.to_string());
                            call["id"] = json!(id);
                        }
                        call
                    })
                    .collect();
                out["tool_calls"] = json!(native);
            }
        }
        "tool" => {
            let id = m.get("tool_call_id").and_then(Value::as_str).filter(|s| !s.is_empty());
            // 모델이 어느 도구의 결과인지 알 수 있게 도구 이름을 붙인다(`tool_name`).
            let name = id
                .and_then(|id| call_names.get(id).cloned())
                .or_else(|| m.get("name").and_then(Value::as_str).map(String::from))
                .filter(|n| !n.is_empty());
            if let Some(name) = name {
                out["tool_name"] = json!(name);
            }
            if let Some(id) = id {
                out["tool_call_id"] = json!(id);
            }
        }
        _ => {}
    }
    out
}

/// `think` 값을 정한다. 생각하기를 지원하지 않는 모델(`thinking` 능력 없음)에는 보내지 않는다.
/// 지원하는 모델은 `reasoning_effort`(또는 `reasoning.effort`)가 꺼짐(none·off)이면 false, low·medium·high면
/// 그 값, 그 밖(설정 없음·켬)이면 true. Ollama는 `think`가 없으면 생각을 `message.thinking`으로 나누지 않고
/// 본문에 섞어 보낼 수 있어, 켜 둔 모델은 분명히 true로 보낸다.
fn think_value(openai: &Value, thinking: bool) -> Option<Value> {
    if !thinking {
        return None;
    }
    let effort = openai
        .get("reasoning_effort")
        .or_else(|| openai.pointer("/reasoning/effort"))
        .and_then(Value::as_str)
        .map(|e| e.trim().to_ascii_lowercase())
        .unwrap_or_default();
    Some(match effort.as_str() {
        "none" | "off" => json!(false),
        "low" | "medium" | "high" => json!(effort),
        _ => json!(true),
    })
}

/// 에이전트 루프의 OpenAI 모양 요청 본문을 `/api/chat` 본문으로 바꾼다.
/// `num_ctx`(0이면 넣지 않음)를 요청마다 넣어, 모델이 쉬다 내려간 뒤에도 Ollama 기본 컨텍스트가 아니라
/// 앱 컨텍스트로 (다시) 로드되게 한다. `keep_alive`는 넣지 않는다(사용자의 `OLLAMA_KEEP_ALIVE`를 따른다).
/// `thinking`은 모델이 생각하기를 지원하는지(`think_value` 참고).
pub fn native_chat_body(openai: &Value, num_ctx: u32, thinking: bool) -> Value {
    let mut call_names = HashMap::new();
    let messages: Vec<Value> = openai
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|m| native_message(m, &mut call_names))
        .collect();
    let mut options = serde_json::Map::new();
    if num_ctx > 0 {
        options.insert("num_ctx".into(), json!(num_ctx));
    }
    for (from, to) in OPTION_FIELDS {
        if let Some(v) = openai.get(*from).filter(|v| !v.is_null()) {
            options.insert((*to).into(), v.clone());
        }
    }
    let mut body = json!({
        "model": openai.get("model").cloned().unwrap_or(Value::Null),
        "messages": messages,
        "stream": openai.get("stream").and_then(Value::as_bool).unwrap_or(false),
        "options": options,
    });
    if let Some(tools) = openai.get("tools").and_then(Value::as_array).filter(|t| !t.is_empty()) {
        body["tools"] = json!(tools);
    }
    if let Some(think) = think_value(openai, thinking) {
        body["think"] = think;
    }
    body
}

fn native_error(err: &Value) -> Value {
    let msg = err
        .as_str()
        .map(str::to_string)
        .or_else(|| err.get("message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| err.to_string());
    json!({ "error": { "message": msg } })
}

/// 마지막 객체의 `prompt_eval_count`·`eval_count`를 OpenAI `usage`로 바꾼다.
fn native_usage(v: &Value) -> Option<Value> {
    let prompt = v.get("prompt_eval_count").and_then(Value::as_u64);
    let completion = v.get("eval_count").and_then(Value::as_u64);
    if prompt.is_none() && completion.is_none() {
        return None;
    }
    let (p, c) = (prompt.unwrap_or(0), completion.unwrap_or(0));
    Some(json!({ "prompt_tokens": p, "completion_tokens": c, "total_tokens": p + c }))
}

fn finish_reason(v: &Value, had_tool_calls: bool) -> &'static str {
    if had_tool_calls {
        return "tool_calls";
    }
    match v.get("done_reason").and_then(Value::as_str) {
        Some("length") => "length",
        _ => "stop",
    }
}

fn sse_line(v: &Value) -> String {
    format!("data: {v}")
}

/// `/api/chat` NDJSON 스트림을 에이전트 루프가 읽는 OpenAI 모양 SSE 줄(`data: {...}`, `data: [DONE]`)로 바꾼다.
/// `message.thinking` → `reasoning_content`, `message.content` → `content`, `message.tool_calls`(인자 객체,
/// 호출마다 완성된 채로 온다) → 번호를 매긴 `tool_calls`(인자 문자열), 마지막 객체(`done: true`) →
/// `finish_reason`·`usage`와 `[DONE]`.
#[derive(Debug)]
pub struct NativeStream {
    calls: usize,
    id_prefix: String,
    done: bool,
    /// 이 요청에 보낸 `num_ctx`(0이면 모름). 프롬프트가 거의 다 찼는지 보는 데만 쓴다.
    num_ctx: u32,
}

impl Default for NativeStream {
    fn default() -> Self {
        Self::new(0)
    }
}

/// 프롬프트가 `num_ctx`를 거의 다 채웠는지. Ollama는 넘친 앞부분을 조용히 잘라 내므로(오류 없음)
/// 응답의 `prompt_eval_count`로만 알 수 있다.
pub fn near_context_limit(prompt_eval_count: u64, num_ctx: u32) -> bool {
    num_ctx > 0 && prompt_eval_count + NEAR_CONTEXT_MARGIN >= u64::from(num_ctx)
}

impl NativeStream {
    pub fn new(num_ctx: u32) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        // 서버가 id를 주지 않을 때 쓴다. 라운드마다 달라야 대화 기록의 도구 결과가 섞이지 않는다.
        Self { calls: 0, id_prefix: format!("call_{nanos:x}"), done: false, num_ctx }
    }

    /// Ollama 도구 호출(인자 객체)을 번호와 id가 붙은 OpenAI 모양(인자 문자열)으로 바꾼다.
    fn convert_calls(&mut self, calls: &[Value]) -> Vec<Value> {
        calls
            .iter()
            .map(|c| {
                let index = self.calls;
                self.calls += 1;
                let id = c
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{}_{index}", self.id_prefix));
                let args = match c.pointer("/function/arguments") {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) if !other.is_null() => other.to_string(),
                    _ => "{}".to_string(),
                };
                json!({
                    "index": index,
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": c.pointer("/function/name").and_then(Value::as_str).unwrap_or(""),
                        "arguments": args
                    }
                })
            })
            .collect()
    }

    pub fn translate_line(&mut self, line: &str) -> Vec<String> {
        let line = line.trim();
        if line.is_empty() || self.done {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        if let Some(err) = v.get("error") {
            return vec![sse_line(&native_error(err))];
        }
        let mut out = Vec::new();
        let mut delta = serde_json::Map::new();
        if let Some(t) = v.pointer("/message/thinking").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            delta.insert("reasoning_content".into(), json!(t));
        }
        if let Some(c) = v.pointer("/message/content").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            delta.insert("content".into(), json!(c));
        }
        if let Some(calls) = v.pointer("/message/tool_calls").and_then(Value::as_array).filter(|c| !c.is_empty()) {
            delta.insert("tool_calls".into(), json!(self.convert_calls(calls)));
        }
        if !delta.is_empty() {
            out.push(sse_line(&json!({ "choices": [{ "index": 0, "delta": delta }] })));
        }
        if v.get("done").and_then(Value::as_bool) == Some(true) {
            self.done = true;
            if let Some(prompt) = v.get("prompt_eval_count").and_then(Value::as_u64) {
                if near_context_limit(prompt, self.num_ctx) {
                    // 다음 라운드의 넘침 처리로 이어 주는 것은 아직 없다(STATUS 남은 위험 참고).
                    eprintln!(
                        "[ollama] 프롬프트가 컨텍스트에 거의 찼습니다: prompt_eval_count {prompt} / num_ctx {} — 앞부분이 잘렸을 수 있습니다",
                        self.num_ctx
                    );
                }
            }
            let mut last = json!({
                "choices": [{ "index": 0, "delta": {}, "finish_reason": finish_reason(&v, self.calls > 0) }]
            });
            if let Some(u) = native_usage(&v) {
                last["usage"] = u;
            }
            out.push(sse_line(&last));
            out.push("data: [DONE]".to_string());
        }
        out
    }
}

/// 스트림을 쓰지 않는 `/api/chat` 응답을 OpenAI 모양(`choices[0].message`)으로 바꾼다.
pub fn native_response_to_openai(v: &Value) -> Value {
    if let Some(err) = v.get("error") {
        return native_error(err);
    }
    let mut message = json!({
        "role": "assistant",
        "content": v.pointer("/message/content").and_then(Value::as_str).unwrap_or("")
    });
    if let Some(t) = v.pointer("/message/thinking").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        message["reasoning_content"] = json!(t);
    }
    let calls = v.pointer("/message/tool_calls").and_then(Value::as_array).filter(|c| !c.is_empty());
    let had_calls = calls.is_some();
    if let Some(calls) = calls {
        let mut converted = NativeStream::default().convert_calls(calls);
        for c in &mut converted {
            if let Some(o) = c.as_object_mut() {
                o.remove("index");
            }
        }
        message["tool_calls"] = json!(converted);
    }
    let mut out = json!({
        "choices": [{ "index": 0, "message": message, "finish_reason": finish_reason(v, had_calls) }]
    });
    if let Some(u) = native_usage(v) {
        out["usage"] = u;
    }
    out
}

/// 대화 턴 밖의 요청(대화 압축 요약 등)에 넣을 컨텍스트: 지금 로드된 값, 없으면 앱이 원하는 값.
pub async fn request_context(settings: &AppSettings, key: &str) -> u32 {
    crate::context_budget::resolve_context(turn_model_info(settings, key).await.loaded_context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ModelPref;
    use http_body_util::{BodyExt, Full};
    use hyper::body::{Bytes, Incoming};
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;
    use std::sync::{Arc, Mutex};

    #[test]
    fn urls_use_origin_only() {
        let u = urls("http://localhost:11434/v1/chat/completions?foo=1").unwrap();
        assert_eq!(u.chat, "http://localhost:11434/api/chat");
        assert_eq!(u.tags, "http://localhost:11434/api/tags");
        assert_eq!(u.ps, "http://localhost:11434/api/ps");
        assert_eq!(u.show, "http://localhost:11434/api/show");
        assert_eq!(u.generate, "http://localhost:11434/api/generate");
        assert_eq!(u.version, "http://localhost:11434/api/version");
    }

    #[test]
    fn parse_show_reads_capabilities_context_and_legacy_fallback() {
        let show = json!({
            "capabilities": ["completion", "vision", "tools", "thinking"],
            "model_info": {
                "general.architecture": "qwen3",
                "qwen3.context_length": 40960
            }
        });
        let info = parse_show(&show, "qwen3:8b", &[]);
        assert_eq!(info.max_context_length, 40960);
        assert!(info.vision && info.tool_use);
        assert_eq!(info.reasoning_options, vec!["off", "on"]);
        assert!(!info.is_embedding_only);

        let embed_show = json!({ "capabilities": ["embedding"] });
        assert!(parse_show(&embed_show, "nomic-embed-text:latest", &[]).is_embedding_only);

        let legacy_vision = parse_show(&json!({}), "llava:latest", &["llama".into(), "clip".into()]);
        assert!(legacy_vision.vision && legacy_vision.tool_use && !legacy_vision.is_embedding_only);

        let legacy_embed = parse_show(&json!({}), "nomic-embed-text:latest", &[]);
        assert!(legacy_embed.is_embedding_only);
    }

    #[test]
    fn build_models_merges_tags_ps_and_show_and_skips_embeddings() {
        let tags = json!({
            "models": [
                {
                    "name": "qwen3:8b",
                    "size": 5_200_000_000u64,
                    "details": { "parameter_size": "8.2B", "quantization_level": "Q4_K_M", "families": ["qwen3"] }
                },
                {
                    "name": "nomic-embed-text:latest",
                    "size": 274_000_000u64,
                    "details": { "families": ["nomic-bert"] }
                }
            ]
        });
        let ps = parse_ps(&json!({
            "models": [{ "name": "qwen3:8b", "context_length": 32768 }]
        }));
        let mut shows = HashMap::new();
        shows.insert(
            "qwen3:8b".to_string(),
            OllamaShowInfo {
                max_context_length: 40960,
                vision: false,
                tool_use: true,
                reasoning_options: vec!["off".into(), "on".into()],
                is_embedding_only: false,
            },
        );
        shows.insert(
            "nomic-embed-text:latest".to_string(),
            OllamaShowInfo {
                is_embedding_only: true,
                ..Default::default()
            },
        );

        let models = build_models(&tags, &ps, &shows);
        assert_eq!(models.len(), 1);
        let q = &models[0];
        assert_eq!(q.key, "qwen3:8b");
        assert_eq!(q.params.as_deref(), Some("8.2B"));
        assert_eq!(q.quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(q.max_context_length, 40960);
        assert_eq!(q.loaded, vec![LoadedInstance { id: "qwen3:8b".into(), context_length: 32768 }]);
        assert!(q.tool_use && !q.vision);
        assert_eq!(q.reasoning_options, vec!["off", "on"]);
    }

    // 아래 표본은 Ollama API 문서(docs/api.md, POST /api/chat)의 예시 모양을 따른다.

    #[test]
    fn native_chat_body_carries_num_ctx_tools_and_history_without_keep_alive() {
        let openai = json!({
            "model": "qwen3:8b",
            "stream": true,
            "stream_options": { "include_usage": true },
            "max_tokens": 4096,
            "presence_penalty": 1.5,
            "reasoning_effort": "none",
            "tool_choice": "auto",
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "날씨",
                    "parameters": { "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] }
                }
            }],
            "messages": [
                { "role": "system", "content": "시스템 지침" },
                { "role": "user", "content": "what is the weather in tokyo?" },
                {
                    "role": "assistant",
                    "content": null,
                    "reasoning_content": "도구를 부르자",
                    "tool_calls": [{ "id": "call_1", "type": "function",
                        "function": { "name": "get_weather", "arguments": "{\"city\":\"Tokyo\"}" } }]
                },
                { "role": "tool", "tool_call_id": "call_1", "content": "11 degrees celsius" },
                { "role": "user", "content": [
                    { "type": "text", "text": "이 그림은?" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,iVBORw0KGgo=" } }
                ] }
            ]
        });
        let body = native_chat_body(&openai, 16384, true);
        assert_eq!(body["model"], "qwen3:8b");
        assert_eq!(body["stream"], true);
        // 사용자의 OLLAMA_KEEP_ALIVE를 덮어쓰지 않는다.
        assert!(body.get("keep_alive").is_none());
        assert_eq!(body["options"]["num_ctx"], 16384);
        assert_eq!(body["options"]["num_predict"], 4096);
        assert_eq!(body["options"]["presence_penalty"], 1.5);
        assert_eq!(body["think"], false);
        assert_eq!(body["tools"], openai["tools"]);
        for gone in ["stream_options", "tool_choice", "max_tokens", "reasoning_effort", "presence_penalty"] {
            assert!(body.get(gone).is_none(), "{gone}");
        }
        let m = body["messages"].as_array().unwrap();
        assert_eq!(m[0], json!({ "role": "system", "content": "시스템 지침" }));
        assert_eq!(m[2]["content"], "");
        assert_eq!(m[2]["thinking"], "도구를 부르자");
        assert_eq!(m[2]["tool_calls"][0]["function"], json!({ "name": "get_weather", "arguments": { "city": "Tokyo" } }));
        assert_eq!(m[3]["role"], "tool");
        assert_eq!(m[3]["tool_name"], "get_weather");
        assert_eq!(m[3]["content"], "11 degrees celsius");
        assert_eq!(m[4]["content"], "이 그림은?");
        assert_eq!(m[4]["images"], json!(["iVBORw0KGgo="]));
    }

    #[test]
    fn native_chat_body_skips_think_for_non_thinking_models_and_zero_context() {
        let body = native_chat_body(&json!({ "model": "m", "messages": [], "stream": false }), 0, false);
        assert!(body.get("think").is_none());
        assert!(body["options"].get("num_ctx").is_none());
        assert!(body.get("tools").is_none());
        assert_eq!(body["stream"], false);
        let bad_args = native_chat_body(
            &json!({ "messages": [{ "role": "assistant", "content": "",
                "tool_calls": [{ "id": "c", "function": { "name": "x", "arguments": "not json" } }] }] }),
            8192,
            false,
        );
        assert_eq!(bad_args["messages"][0]["tool_calls"][0]["function"]["arguments"], json!({}));
        // 생각하기가 없는 모델에는 꺼짐 설정도 보내지 않는다.
        assert!(native_chat_body(&json!({ "reasoning_effort": "none" }), 0, false).get("think").is_none());
    }

    #[test]
    fn native_chat_body_turns_thinking_on_for_thinking_models_unless_off() {
        let think = |v: Value| native_chat_body(&v, 0, true).get("think").cloned();
        assert_eq!(think(json!({ "model": "m", "messages": [] })), Some(json!(true)));
        assert_eq!(think(json!({ "reasoning_effort": "none" })), Some(json!(false)));
        assert_eq!(think(json!({ "reasoning": { "effort": "off" } })), Some(json!(false)));
        assert_eq!(think(json!({ "reasoning_effort": "high" })), Some(json!("high")));
    }

    #[test]
    fn near_context_limit_uses_the_margin_and_ignores_unknown_context() {
        assert!(!near_context_limit(16_000, 16_384));
        assert!(near_context_limit(16_128, 16_384));
        assert!(near_context_limit(20_000, 16_384));
        assert!(!near_context_limit(1_000_000, 0));
    }

    fn feed(lines: &[Value]) -> (Vec<String>, crate::agent::RoundAccumulator, bool) {
        use crate::agent::{parse_sse_line, SseEvent};
        let mut stream = NativeStream::default();
        let mut acc = crate::agent::RoundAccumulator::default();
        let mut sse = Vec::new();
        let mut done = false;
        for l in lines {
            for out in stream.translate_line(&l.to_string()) {
                match parse_sse_line(&out) {
                    SseEvent::Data(v) => {
                        acc.apply_chunk_for(&v, "Ollama").unwrap();
                    }
                    SseEvent::Done => done = true,
                    SseEvent::Skip => {}
                }
                sse.push(out);
            }
        }
        (sse, acc, done)
    }

    #[test]
    fn native_stream_maps_thinking_content_and_final_usage() {
        let (_, acc, done) = feed(&[
            json!({ "model": "qwen3", "created_at": "2025-05-30T10:00:00Z",
                "message": { "role": "assistant", "content": "", "thinking": "Let me think" }, "done": false }),
            json!({ "model": "qwen3", "message": { "role": "assistant", "content": "The sky" }, "done": false }),
            json!({ "model": "qwen3", "message": { "role": "assistant", "content": " is blue." }, "done": false }),
            json!({ "model": "qwen3", "created_at": "2025-05-30T10:00:01Z",
                "message": { "role": "assistant", "content": "" }, "done_reason": "stop", "done": true,
                "total_duration": 4883583458u64, "load_duration": 1334875, "prompt_eval_count": 26,
                "prompt_eval_duration": 342546000, "eval_count": 282, "eval_duration": 4535599000u64 }),
        ]);
        assert!(done);
        assert_eq!(acc.reasoning, "Let me think");
        assert_eq!(acc.content, "The sky is blue.");
        assert_eq!(acc.finish_reason.as_deref(), Some("stop"));
        let usage = acc.usage.unwrap();
        assert_eq!((usage["prompt_tokens"].as_u64(), usage["completion_tokens"].as_u64()), (Some(26), Some(282)));
    }

    #[test]
    fn native_stream_numbers_tool_calls_and_serializes_arguments() {
        let (_, acc, done) = feed(&[
            json!({ "model": "llama3.2", "created_at": "2024-07-22T20:33:28.123648Z",
                "message": { "role": "assistant", "content": "",
                    "tool_calls": [{ "function": { "name": "get_weather", "arguments": { "city": "Tokyo" } } }] },
                "done": false }),
            json!({ "model": "llama3.2", "message": { "role": "assistant", "content": "",
                    "tool_calls": [
                        { "id": "call_srv", "function": { "index": 0, "name": "get_time", "arguments": { "zone": "JST" } } },
                        { "function": { "name": "list_directory", "arguments": {} } }
                    ] }, "done": false }),
            json!({ "model": "llama3.2", "message": { "role": "assistant", "content": "" },
                "done_reason": "stop", "done": true, "total_duration": 182242375, "prompt_eval_count": 169, "eval_count": 15 }),
        ]);
        assert!(done);
        assert_eq!(acc.finish_reason.as_deref(), Some("tool_calls"));
        let names: Vec<&str> = acc.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["get_weather", "get_time", "list_directory"]);
        assert_eq!(serde_json::from_str::<Value>(&acc.tools[0].arguments).unwrap(), json!({ "city": "Tokyo" }));
        assert_eq!(acc.tools[2].arguments, "{}");
        assert_eq!(acc.tools[1].id, "call_srv");
        assert!(!acc.tools[0].id.is_empty() && acc.tools[0].id != acc.tools[2].id);
    }

    #[test]
    fn native_stream_reports_length_errors_and_ignores_after_done() {
        let (sse, acc, _) = feed(&[
            json!({ "message": { "role": "assistant", "content": "잘린" }, "done": false }),
            json!({ "message": { "role": "assistant", "content": "" }, "done_reason": "length", "done": true }),
            json!({ "message": { "role": "assistant", "content": "뒤늦은 글" }, "done": false }),
        ]);
        assert_eq!(acc.finish_reason.as_deref(), Some("length"));
        assert_eq!(acc.content, "잘린");
        assert_eq!(sse.last().map(String::as_str), Some("data: [DONE]"));

        let mut stream = NativeStream::default();
        assert!(stream.translate_line("").is_empty());
        assert!(stream.translate_line("not json").is_empty());
        let err = stream.translate_line(r#"{"error":"model requires more system memory"}"#);
        let crate::agent::SseEvent::Data(v) = crate::agent::parse_sse_line(&err[0]) else { panic!("{err:?}") };
        let msg = crate::agent::RoundAccumulator::default().apply_chunk_for(&v, "Ollama").unwrap_err();
        assert!(msg.contains("model requires more system memory"), "{msg}");
    }

    #[test]
    fn native_response_becomes_openai_message() {
        let v = native_response_to_openai(&json!({
            "model": "llama3.2", "created_at": "2023-12-12T14:13:43.416799Z",
            "message": { "role": "assistant", "content": "Hello! How are you today?", "thinking": "인사" },
            "done_reason": "stop", "done": true, "prompt_eval_count": 26, "eval_count": 298
        }));
        assert_eq!(v["choices"][0]["message"]["content"], "Hello! How are you today?");
        assert_eq!(v["choices"][0]["message"]["reasoning_content"], "인사");
        assert_eq!(v["choices"][0]["finish_reason"], "stop");
        assert_eq!(v["usage"]["completion_tokens"], 298);

        let tools = native_response_to_openai(&json!({
            "message": { "role": "assistant", "content": "",
                "tool_calls": [{ "function": { "name": "get_weather", "arguments": { "city": "Tokyo" } } }] },
            "done": true
        }));
        assert_eq!(tools["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(tools["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"], r#"{"city":"Tokyo"}"#);
        assert!(tools["choices"][0]["message"]["tool_calls"][0].get("index").is_none());
        assert_eq!(native_response_to_openai(&json!({ "error": "boom" }))["error"]["message"], "boom");
    }

    #[test]
    fn ollama_host_env_matches_the_configured_address() {
        assert_eq!(ollama_host_env("http://localhost:11434").unwrap(), "127.0.0.1:11434");
        assert_eq!(ollama_host_env("http://127.0.0.1:12000/v1").unwrap(), "127.0.0.1:12000");
        assert_eq!(ollama_host_env("http://[::1]:11500").unwrap(), "[::1]:11500");
        assert_eq!(ollama_host_env("http://localhost").unwrap(), "127.0.0.1:80");
        assert!(ollama_host_env("http://192.168.0.2:11434").is_err());
    }

    #[test]
    fn path_lookup_skips_relative_entries() {
        let rel_root = tempfile::tempdir_in(".").unwrap();
        let rel_name = rel_root.path().file_name().unwrap().to_owned();
        let abs = tempfile::tempdir().unwrap();
        for d in [rel_root.path(), abs.path()] {
            std::fs::write(d.join("ollama-test.exe"), b"").unwrap();
        }
        let path = std::env::join_paths([std::path::PathBuf::from(&rel_name), abs.path().to_path_buf()]).unwrap();
        assert_eq!(find_on_path(&path, "ollama-test.exe"), Some(abs.path().join("ollama-test.exe")));
        let only_rel = std::env::join_paths([std::path::PathBuf::from(&rel_name)]).unwrap();
        assert_eq!(find_on_path(&only_rel, "ollama-test.exe"), None);
    }

    async fn mock_ollama_server(generate_requests: Arc<Mutex<Vec<Value>>>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { break };
                let reqs = Arc::clone(&generate_requests);
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let reqs = Arc::clone(&reqs);
                        async move {
                            let path = req.uri().path().to_string();
                            let body_bytes = req.into_body().collect().await.unwrap().to_bytes();
                            let body_json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
                            let (status, reply) = match path.as_str() {
                                "/api/version" => (200, json!({ "version": "0.6.8" })),
                                "/api/tags" => (
                                    200,
                                    json!({
                                        "models": [
                                            {
                                                "name": "qwen3:8b",
                                                "size": 5_000_000_000u64,
                                                "details": { "parameter_size": "8B", "quantization_level": "Q4_K_M" }
                                            },
                                            {
                                                "name": "bge-m3:latest",
                                                "size": 1_200_000_000u64,
                                                "details": {}
                                            }
                                        ]
                                    }),
                                ),
                                "/api/ps" => (
                                    200,
                                    json!({
                                        "models": [{ "name": "qwen3:8b", "context_length": 16384 }]
                                    }),
                                ),
                                "/api/show" => {
                                    let model = body_json["model"].as_str().unwrap_or("");
                                    if model == "bge-m3:latest" {
                                        (200, json!({ "capabilities": ["embedding"] }))
                                    } else {
                                        (
                                            200,
                                            json!({
                                                "capabilities": ["completion", "tools", "thinking"],
                                                "model_info": { "qwen3.context_length": 40960 }
                                            }),
                                        )
                                    }
                                }
                                "/api/generate" => {
                                    reqs.lock().unwrap().push(body_json);
                                    (200, json!({ "done": true }))
                                }
                                _ => (404, json!({ "error": "not found" })),
                            };
                            Ok::<_, std::convert::Infallible>(
                                Response::builder()
                                    .status(status)
                                    .header("Content-Type", "application/json")
                                    .body(Full::new(Bytes::from(reply.to_string())))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = http1::Builder::new().serve_connection(io, svc).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn mock_ollama_end_to_end_list_plan_load_unload_and_turn_info() {
        let reqs = Arc::new(Mutex::new(Vec::new()));
        let url = mock_ollama_server(Arc::clone(&reqs)).await;
        let mut settings = AppSettings {
            provider: LlmProvider::Ollama,
            server_url: url,
            ..Default::default()
        };
        settings
            .model_prefs
            .insert("qwen3:8b".into(), ModelPref { context_length: Some(32768), reasoning: None });

        assert!(server_running(&settings).await);

        let models = list_models(&settings).await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].key, "qwen3:8b");
        assert_eq!(models[0].loaded[0].context_length, 16384);

        let plan = plan_load(&settings, "qwen3:8b").await.unwrap();
        assert!(matches!(
            plan,
            LoadPlan::SmallerLoaded { loaded: 16384, wanted: 32768, .. }
        ));

        let loaded = load_model(&settings, "qwen3:8b", 32768).await.unwrap();
        assert_eq!(loaded.id, "qwen3:8b");

        unload_instance(&settings, "qwen3:8b").await.unwrap();

        let captured = reqs.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0]["options"]["num_ctx"], 32768);
        assert_eq!(captured[1]["keep_alive"], 0);
        drop(captured);

        let info = turn_model_info(&settings, "qwen3:8b").await;
        assert_eq!(info.loaded_context, Some(16384));
        // 로드되어 있지 않으면 다음 요청의 num_ctx가 될 원하는 값(모델 최대로 자름)
        settings
            .model_prefs
            .insert("other:1b".into(), ModelPref { context_length: Some(65536), reasoning: None });
        assert_eq!(turn_model_info(&settings, "other:1b").await.loaded_context, Some(40960));
        assert_eq!(request_context(&settings, "other:1b").await, 40960);
        assert_eq!(info.reasoning_options, vec!["off", "on"]);
        assert_eq!(info.display_name.as_deref(), Some("qwen3:8b"));
    }
}
