//! LM Studio 로컬 서버 연결: 주소 검증, 인증 헤더, 연결 오류 안내, 모델 목록·로드·언로드.

use crate::store::AppSettings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::sync::LazyLock;
use std::path::PathBuf;
use std::time::Duration;

/// 서버 주소에서 만든 요청 주소 모음.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerUrls {
    pub chat: String,
    pub models: String,
    pub load: String,
    pub unload: String,
}

pub const SERVER_OFF_MESSAGE: &str =
    "LM Studio 서버가 꺼져 있습니다. LM Studio에서 서버를 켜거나 [서버 시작]을 눌러 주세요.";

/// 이 PC의 LM Studio 서버(`http://localhost`, `127.0.0.1`, `[::1]`)만 허용한다.
///
/// 요청 주소는 scheme·호스트·포트만으로 만든다. 사용자가 OpenAI SDK 예시처럼
/// `http://localhost:1234/v1`이나 `.../v1/chat/completions?x=1`을 붙여 넣어도 경로·질의·조각은 버린다
/// (그대로 붙이면 `/v1/v1/...`이 되어 모든 요청이 실패한다).
pub fn server_urls(server_url: &str) -> Result<ServerUrls, String> {
    let raw = server_url.trim();
    let url = reqwest::Url::parse(raw)
        .map_err(|_| format!("LM Studio 서버 주소가 올바르지 않습니다: {}", raw))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() != "http" || !local {
        return Err(
            "LM Studio 서버 주소는 http://localhost, http://127.0.0.1, http://[::1] 만 쓸 수 있습니다.".to_string(),
        );
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("서버 주소에 사용자 이름이나 비밀번호를 넣을 수 없습니다.".to_string());
    }
    let host = url.host_str().unwrap_or("localhost");
    let base = match url.port() {
        Some(port) => format!("http://{host}:{port}"),
        None => format!("http://{host}"),
    };
    Ok(ServerUrls {
        chat: format!("{base}/v1/chat/completions"),
        models: format!("{base}/api/v1/models"),
        load: format!("{base}/api/v1/models/load"),
        unload: format!("{base}/api/v1/models/unload"),
    })
}

/// 토큰이 있을 때만 `Authorization: Bearer` 헤더를 붙인다.
pub fn authorize(rb: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    let t = token.trim();
    if t.is_empty() {
        rb
    } else {
        rb.bearer_auth(t)
    }
}

/// 연결 거부·연결 시간 초과는 서버가 꺼진 것으로 안내한다.
pub fn friendly_error(err: &reqwest::Error) -> String {
    if err.is_connect() {
        SERVER_OFF_MESSAGE.to_string()
    } else {
        format!("LM Studio 연결 오류: {}", err)
    }
}

/// 기본 원하는 컨텍스트.
pub const DEFAULT_CONTEXT: u32 = 16384;
/// 모델 로드 요청 시간 제한(35B는 1분 가까이 걸릴 수 있다).
const LOAD_TIMEOUT: Duration = Duration::from_secs(300);
const QUICK_TIMEOUT: Duration = Duration::from_secs(10);
pub const LMS_MISSING_MESSAGE: &str =
    "LM Studio 명령줄 도구를 찾지 못했습니다. LM Studio를 한 번 실행한 뒤 다시 시도해 주세요.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedInstance {
    pub id: String,
    pub context_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmModel {
    pub key: String,
    pub display_name: String,
    pub params: Option<String>,
    pub quantization: Option<String>,
    pub size_bytes: u64,
    pub max_context_length: u32,
    pub loaded: Vec<LoadedInstance>,
    pub vision: bool,
    pub tool_use: bool,
    pub reasoning_options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LoadPlan {
    /// 원하는 컨텍스트 이상으로 이미 로드돼 있다.
    Ready { instance_id: String, context_length: u32 },
    /// 같은 모델이 더 작게 로드돼 있다(쓰되 [다시 로드] 안내).
    SmallerLoaded { instance_id: String, loaded: u32, wanted: u32 },
    /// 로드돼 있지 않다. `others`는 로드된 다른 LLM의 instance id.
    NotLoaded { others: Vec<String> },
    /// 설치된 모델 목록에 없다.
    Missing,
}

/// `GET /api/v1/models` 응답에서 `type == "llm"`만 뽑는다.
pub fn parse_models(v: &Value) -> Vec<LmModel> {
    let Some(arr) = v.get("models").and_then(|m| m.as_array()) else { return Vec::new() };
    arr.iter()
        .filter(|m| m.get("type").and_then(|t| t.as_str()) == Some("llm"))
        .filter_map(|m| {
            let key = m.get("key")?.as_str()?.to_string();
            let loaded = m
                .get("loaded_instances")
                .and_then(|l| l.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|i| {
                            Some(LoadedInstance {
                                id: i.get("id")?.as_str()?.to_string(),
                                // 컨텍스트를 못 읽어도 로드된 것으로 남긴다(두 번째 인스턴스 방지).
                                context_length: i
                                    .pointer("/config/context_length")
                                    .and_then(|c| c.as_u64())
                                    .unwrap_or(0) as u32,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let caps = m.get("capabilities");
            let reasoning_options = caps
                .and_then(|c| c.pointer("/reasoning/allowed_options"))
                .and_then(|o| o.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            Some(LmModel {
                display_name: m.get("display_name").and_then(|d| d.as_str()).unwrap_or(&key).to_string(),
                params: m.get("params_string").and_then(|p| p.as_str()).map(String::from),
                quantization: m.pointer("/quantization/name").and_then(|p| p.as_str()).map(String::from),
                size_bytes: m.get("size_bytes").and_then(|s| s.as_u64()).unwrap_or(0),
                max_context_length: m.get("max_context_length").and_then(|s| s.as_u64()).unwrap_or(0) as u32,
                loaded,
                vision: caps.and_then(|c| c.get("vision")).and_then(|b| b.as_bool()).unwrap_or(false),
                tool_use: caps
                    .and_then(|c| c.get("trained_for_tool_use"))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false),
                reasoning_options,
                key,
            })
        })
        .collect()
}

/// 추론을 끌 때 요청 본문에 더할 항목(결정 0003: `reasoning_effort: "none"`만 9B·35B 모두에서 reasoning_tokens 0을 실측).
pub static REASONING_OFF_FIELDS: LazyLock<Value> = LazyLock::new(|| json!({ "reasoning_effort": "none" }));

/// 모델의 추론 선택지(`options`)와 사용자의 선택(`pref`)에서 요청 본문에 더할 항목을 만든다.
/// 추론을 지원하지 않는 모델이거나 "끄기"가 아니면 아무것도 보내지 않아 모델 기본값에 맡긴다.
pub fn reasoning_fields(options: &[String], pref: Option<&str>) -> Map<String, Value> {
    if options.is_empty() || pref != Some("off") {
        return Map::new();
    }
    REASONING_OFF_FIELDS.as_object().cloned().unwrap_or_default()
}

/// 턴 시작 때 한 번의 모델 목록으로 얻는 정보.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnModelInfo {
    /// 그 모델의 추론 선택지. 모르면 빈 목록(추론 항목을 보내지 않음).
    pub reasoning_options: Vec<String>,
    /// 로드된 인스턴스 중 가장 큰 컨텍스트 길이. 로드 안 됨·보고 안 됨(0)·목록 실패면 None.
    pub loaded_context: Option<u32>,
    /// 모델 표시 이름(시스템 프롬프트와 통계 줄에 쓴다). 모르면 None.
    pub display_name: Option<String>,
}

/// 목록에서 한 모델의 턴 정보를 뽑는다.
pub fn turn_info_from(models: &[LmModel], key: &str) -> TurnModelInfo {
    let Some(m) = models.iter().find(|m| m.key == key) else { return TurnModelInfo::default() };
    TurnModelInfo {
        reasoning_options: m.reasoning_options.clone(),
        loaded_context: m.loaded.iter().map(|i| i.context_length).max().filter(|&c| c > 0),
        display_name: Some(m.display_name.clone()).filter(|n| !n.trim().is_empty()),
    }
}

/// 모델 목록 조회가 턴을 오래 붙잡지 않게 하는 제한.
const TURN_INFO_TIMEOUT: Duration = Duration::from_secs(3);

/// 이번 요청에 쓸 모델의 추론 선택지와 로드된 컨텍스트. 목록을 못 가져오면 기본값(턴을 막지 않는다).
pub async fn turn_model_info(settings: &AppSettings, key: &str) -> TurnModelInfo {
    match tokio::time::timeout(TURN_INFO_TIMEOUT, list_models(settings)).await {
        Ok(Ok(models)) => turn_info_from(&models, key),
        _ => TurnModelInfo::default(),
    }
}


/// 요청 전에 모델이 원하는 컨텍스트로 쓸 수 있는지 판단한다. 같은 모델의 두 번째 인스턴스는 만들지 않는다.
pub fn plan_load(models: &[LmModel], key: &str, wanted: u32) -> LoadPlan {
    let Some(model) = models.iter().find(|m| m.key == key) else { return LoadPlan::Missing };
    if let Some(best) = model.loaded.iter().max_by_key(|i| i.context_length) {
        return if best.context_length >= wanted {
            LoadPlan::Ready { instance_id: best.id.clone(), context_length: best.context_length }
        } else {
            LoadPlan::SmallerLoaded { instance_id: best.id.clone(), loaded: best.context_length, wanted }
        };
    }
    let others = models
        .iter()
        .filter(|m| m.key != key)
        .flat_map(|m| m.loaded.iter().map(|i| i.id.clone()))
        .collect();
    LoadPlan::NotLoaded { others }
}

/// 저장된 컨텍스트가 이 값보다 작으면 없는 것으로 본다.
pub const MIN_CONTEXT: u32 = 1024;

/// 모델별 설정이 있으면 그 값(1024 미만은 무시), 없으면 16384. 모델 최대 컨텍스트를 넘지 않는다.
pub fn wanted_context(settings: &AppSettings, model: &LmModel) -> u32 {
    let want = settings
        .model_prefs
        .get(&model.key)
        .and_then(|p| p.context_length)
        .filter(|&c| c >= MIN_CONTEXT)
        .unwrap_or(DEFAULT_CONTEXT);
    if model.max_context_length > 0 { want.min(model.max_context_length) } else { want }
}

/// LM Studio 오류 본문(`{"error":{"message":..}}`)의 문구를 그대로 쓴다.
pub fn api_error_message(status: u16, body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| format!("LM Studio 오류 ({status}): {}", body.trim().chars().take(300).collect::<String>()))
}

/// `POST /api/v1/models/load` 응답에서 인스턴스를 읽는다. 응답에 컨텍스트가 없으면 요청한 값을 쓴다.
pub fn parse_load_response(v: &Value, requested: u32) -> Result<LoadedInstance, String> {
    let id = v
        .get("instance_id")
        .and_then(|i| i.as_str())
        .ok_or_else(|| "LM Studio 로드 응답에 instance_id가 없습니다.".to_string())?;
    let context_length = v
        .pointer("/load_config/context_length")
        .and_then(|c| c.as_u64())
        .map(|c| c as u32)
        .unwrap_or(requested);
    Ok(LoadedInstance { id: id.to_string(), context_length })
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    crate::providers::local_http_client(timeout)
}

pub async fn list_models(settings: &AppSettings) -> Result<Vec<LmModel>, String> {
    let urls = server_urls(&settings.server_url)?;
    let resp = authorize(client(QUICK_TIMEOUT)?.get(&urls.models), &settings.api_key)
        .send()
        .await
        .map_err(|e| friendly_error(&e))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| friendly_error(&e))?;
    if !status.is_success() {
        return Err(api_error_message(status.as_u16(), &body));
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("LM Studio 모델 목록을 읽지 못했습니다: {e}"))?;
    Ok(parse_models(&v))
}

pub const ALREADY_LOADED_MESSAGE: &str = "이미 로드된 모델입니다. [다시 로드]를 사용하세요.";

/// 같은 모델의 두 번째 인스턴스를 만들지 않기 위한 검사. 이미 로드돼 있으면 오류.
pub fn ensure_not_loaded(models: &[LmModel], key: &str) -> Result<(), String> {
    match plan_load(models, key, DEFAULT_CONTEXT) {
        LoadPlan::Ready { .. } | LoadPlan::SmallerLoaded { .. } => Err(ALREADY_LOADED_MESSAGE.to_string()),
        _ => Ok(()),
    }
}

/// 컨텍스트 길이만 지정해 로드한다(GPU 배분은 LM Studio 기본/저장값에 맡긴다).
pub async fn load_model(settings: &AppSettings, key: &str, context_length: u32) -> Result<LoadedInstance, String> {
    let urls = server_urls(&settings.server_url)?;
    ensure_not_loaded(&list_models(settings).await?, key)?;
    let body = serde_json::json!({ "model": key, "context_length": context_length, "echo_load_config": true });
    let resp = authorize(client(LOAD_TIMEOUT)?.post(&urls.load).json(&body), &settings.api_key)
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "모델 로드가 5분 안에 끝나지 않았습니다.".to_string() } else { friendly_error(&e) })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| friendly_error(&e))?;
    if !status.is_success() {
        return Err(api_error_message(status.as_u16(), &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("LM Studio 로드 응답을 읽지 못했습니다: {e}"))?;
    parse_load_response(&v, context_length)
}

pub async fn unload_instance(settings: &AppSettings, instance_id: &str) -> Result<(), String> {
    let urls = server_urls(&settings.server_url)?;
    let resp = authorize(
        client(LOAD_TIMEOUT)?.post(&urls.unload).json(&serde_json::json!({ "instance_id": instance_id })),
        &settings.api_key,
    )
    .send()
    .await
    .map_err(|e| friendly_error(&e))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let text = resp.text().await.unwrap_or_default();
    Err(api_error_message(status.as_u16(), &text))
}

/// 서버가 응답하면 true. 주소가 잘못됐거나 연결이 안 되면 false.
/// HTTP 상태 코드는 보지 않는다("그 주소에 무언가 답한다" = 연결 가능). 그 서버가 정말 LM Studio인지는
/// 모델 목록(`list_models`)이 확인한다 — 남겨 둔 사소한 점(최종 검토 T4).
pub async fn server_running(settings: &AppSettings) -> bool {
    let Ok(urls) = server_urls(&settings.server_url) else { return false };
    let Ok(c) = client(Duration::from_secs(3)) else { return false };
    authorize(c.get(&urls.models), &settings.api_key).send().await.is_ok()
}

/// `%USERPROFILE%\.lmstudio\bin\lms.exe`가 있으면 그 경로.
pub fn lms_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let p = PathBuf::from(home).join(".lmstudio").join("bin").join("lms.exe");
    p.is_file().then_some(p)
}

/// `lms server start`가 끝나기를 기다리는 최대 시간.
const START_SERVER_TIMEOUT: Duration = Duration::from_secs(60);

/// `lms server start`를 창 없이 실행한다. 늘 시간 안에 돌아온다.
pub fn start_server() -> Result<(), String> {
    let lms = lms_path().ok_or_else(|| LMS_MISSING_MESSAGE.to_string())?;
    let mut cmd = std::process::Command::new(lms);
    cmd.args(["server", "start"]);
    let (ok, msg) = run_quiet(cmd, START_SERVER_TIMEOUT)?;
    if ok {
        Ok(())
    } else {
        Err(format!("서버를 시작하지 못했습니다: {}", msg.trim().chars().take(300).collect::<String>()))
    }
}

/// 명령을 창 없이 실행하고 (성공 여부, stdout+stderr)를 돌려준다.
///
/// `Command::output()`은 출력 파이프가 닫힐 때까지 기다리므로, 명령이 띄운 상주 프로세스(LM Studio
/// 데몬)가 파이프를 물려받으면 끝나지 않는다. 그래서 입력은 비우고 출력은 임시 파일로 받으며,
/// 명령 자체가 끝나기만 기다린다(`timeout`을 넘기면 그 명령만 끝내고 오류).
fn run_quiet(mut cmd: std::process::Command, timeout: Duration) -> Result<(bool, String), String> {
    use std::process::Stdio;
    let log_path = std::env::temp_dir().join(format!("anylm-cmd-{}.log", uuid::Uuid::new_v4()));
    let log = std::fs::File::create(&log_path).map_err(|e| format!("임시 파일을 만들지 못했습니다: {e}"))?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;
    cmd.stdin(Stdio::null()).stdout(Stdio::from(log)).stderr(Stdio::from(log_err));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let read_log = |p: &std::path::Path| {
        let text = std::fs::read(p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let _ = std::fs::remove_file(p);
        text
    };
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&log_path);
            return Err(format!("lms 실행 실패: {e}"));
        }
    };
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((status.success(), read_log(&log_path))),
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = read_log(&log_path);
                return Err(format!(
                    "서버 시작 명령이 {}초 안에 끝나지 않았습니다. LM Studio를 직접 열어 서버를 켜 주세요.",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                let _ = read_log(&log_path);
                return Err(format!("lms 실행 실패: {e}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_urls_accept_only_local_http() {
        let u = server_urls("http://localhost:1234/").unwrap();
        assert_eq!(u.chat, "http://localhost:1234/v1/chat/completions");
        assert_eq!(u.models, "http://localhost:1234/api/v1/models");
        assert_eq!(u.load, "http://localhost:1234/api/v1/models/load");
        assert_eq!(u.unload, "http://localhost:1234/api/v1/models/unload");
        assert!(server_urls("http://127.0.0.1:1234").is_ok());
        assert!(server_urls("http://[::1]:1234").is_ok());
        assert!(server_urls("http://192.168.0.5:1234").is_err());
        assert!(server_urls("https://localhost.evil.com").is_err());
        assert!(server_urls("http://user:pw@localhost:1234").is_err());
        assert!(server_urls("not a url").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn run_quiet_returns_even_if_a_child_keeps_running() {
        // `lms server start`처럼, 명령은 바로 끝나지만 띄운 프로세스가 5초 더 살아 있다.
        // Command::output()이었다면 그 프로세스가 출력 파이프를 물려받아 5초 넘게 기다린다.
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", "/B", "ping", "-n", "6", "127.0.0.1", "&", "echo", "started"]);
        let t = std::time::Instant::now();
        let (ok, out) = run_quiet(cmd, Duration::from_secs(20)).unwrap();
        assert!(ok);
        assert!(out.contains("started"), "{out}");
        assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn run_quiet_times_out_and_reports_failure() {
        let mut cmd = std::process::Command::new("ping");
        cmd.args(["-n", "30", "127.0.0.1"]);
        let t = std::time::Instant::now();
        let err = run_quiet(cmd, Duration::from_millis(800)).unwrap_err();
        assert!(err.contains("끝나지 않았습니다"), "{err}");
        assert!(t.elapsed() < Duration::from_secs(5));

        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "echo", "bad", "&", "exit", "3"]);
        let (ok, out) = run_quiet(cmd, Duration::from_secs(10)).unwrap();
        assert!(!ok && out.contains("bad"), "{out}");
    }

    /// 실제 `lms`로 확인(LM Studio가 설치된 PC에서만): 서버를 건드리지 않는 `lms server status`.
    #[test]
    #[ignore]
    fn lms_status_returns_through_run_quiet() {
        let Some(lms) = lms_path() else { return };
        let mut cmd = std::process::Command::new(lms);
        cmd.args(["server", "status"]);
        let (ok, out) = run_quiet(cmd, Duration::from_secs(30)).unwrap();
        eprintln!("ok={ok} out={out}");
    }

    #[test]
    fn server_urls_drop_path_query_and_fragment() {
        let want = |base: &str, u: &ServerUrls| {
            assert_eq!(u.chat, format!("{base}/v1/chat/completions"));
            assert_eq!(u.models, format!("{base}/api/v1/models"));
            assert_eq!(u.load, format!("{base}/api/v1/models/load"));
            assert_eq!(u.unload, format!("{base}/api/v1/models/unload"));
        };
        want("http://localhost:1234", &server_urls("http://localhost:1234/v1").unwrap());
        want("http://localhost:1234", &server_urls("http://localhost:1234/v1/").unwrap());
        want("http://127.0.0.1:1234", &server_urls("http://127.0.0.1:1234/").unwrap());
        want("http://localhost:1234", &server_urls("http://localhost:1234/v1/chat/completions?x=1").unwrap());
        want("http://localhost:1234", &server_urls("  http://localhost:1234#frag  ").unwrap());
        want("http://[::1]:1234", &server_urls("http://[::1]:1234/v1").unwrap());
        want("http://localhost", &server_urls("http://localhost/").unwrap());
        // 경로가 있어도 localhost 규칙은 그대로
        assert!(server_urls("http://example.com:1234/v1").is_err());
    }

    #[test]
    fn empty_token_sends_no_auth_header() {
        let c = reqwest::Client::new();
        let r = authorize(c.get("http://localhost:1234"), "").build().unwrap();
        assert!(r.headers().get("authorization").is_none());
        let r = authorize(c.get("http://localhost:1234"), "  ").build().unwrap();
        assert!(r.headers().get("authorization").is_none());
        let r = authorize(c.get("http://localhost:1234"), "tok").build().unwrap();
        assert_eq!(r.headers()["authorization"], "Bearer tok");
    }

    #[tokio::test]
    async fn connection_refused_is_friendly() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let err = reqwest::get(format!("http://127.0.0.1:{port}")).await.unwrap_err();
        assert!(friendly_error(&err).contains("LM Studio 서버가 꺼져 있습니다"));
    }

    fn fixture() -> Vec<LmModel> {
        parse_models(&serde_json::from_str(include_str!("testdata/lmstudio_models.json")).unwrap())
    }

    #[test]
    fn turn_info_takes_largest_loaded_context_and_ignores_zero() {
        let mut m = fixture();
        let key = m[0].key.clone();
        m[0].loaded = vec![
            LoadedInstance { id: "a".into(), context_length: 4096 },
            LoadedInstance { id: "b".into(), context_length: 32768 },
        ];
        assert_eq!(turn_info_from(&m, &key).loaded_context, Some(32768));
        m[0].loaded = vec![LoadedInstance { id: "a".into(), context_length: 0 }];
        assert_eq!(turn_info_from(&m, &key).loaded_context, None);
        m[0].loaded.clear();
        assert_eq!(turn_info_from(&m, &key).loaded_context, None);
        assert_eq!(turn_info_from(&m, "없는 모델"), TurnModelInfo::default());
        assert_eq!(turn_info_from(&m, &key).display_name.as_deref(), Some(m[0].display_name.as_str()));
    }

    #[test]
    fn parses_only_llms_with_capabilities() {
        let m = fixture();
        assert_eq!(m.len(), 2);
        let q = m.iter().find(|x| x.key == "qwen3.5-9b").unwrap();
        assert_eq!(q.display_name, "Qwen3.5 9B");
        assert_eq!(q.max_context_length, 262144);
        assert_eq!(q.loaded[0].context_length, 32768);
        assert!(q.vision && q.tool_use);
        assert_eq!(q.reasoning_options, vec!["off", "on"]);
        assert_eq!(q.quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(q.params.as_deref(), Some("9B"));
    }

    #[test]
    fn plan_load_cases() {
        let m = fixture();
        assert!(matches!(plan_load(&m, "qwen3.5-9b", 32768), LoadPlan::Ready { context_length: 32768, .. }));
        assert!(matches!(plan_load(&m, "qwen3.5-9b", 16384), LoadPlan::Ready { context_length: 32768, .. }));
        assert!(matches!(
            plan_load(&m, "qwen3.5-9b", 65536),
            LoadPlan::SmallerLoaded { loaded: 32768, wanted: 65536, .. }
        ));
        match plan_load(&m, "qwen3.6-35b-a3b", 32768) {
            LoadPlan::NotLoaded { others } => assert_eq!(others, vec!["qwen3.5-9b"]),
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn plan_load_missing_model() {
        assert!(matches!(plan_load(&fixture(), "deleted-model", 32768), LoadPlan::Missing));
    }

    #[test]
    fn load_plan_serializes_with_kind_tag() {
        let v = serde_json::to_value(LoadPlan::SmallerLoaded { instance_id: "a".into(), loaded: 8192, wanted: 32768 }).unwrap();
        assert_eq!(v, serde_json::json!({"kind":"smallerLoaded","instanceId":"a","loaded":8192,"wanted":32768}));
    }

    #[test]
    fn wanted_context_respects_prefs_and_max() {
        let m = fixture();
        let q = m.iter().find(|x| x.key == "qwen3.5-9b").unwrap();
        let mut s = AppSettings::default();
        assert_eq!(wanted_context(&s, q), 16384);
        let mut small_model = q.clone();
        small_model.max_context_length = 8192;
        assert_eq!(wanted_context(&s, &small_model), 8192);
        s.model_prefs.insert("qwen3.5-9b".into(), crate::store::ModelPref { context_length: Some(8192), reasoning: None });
        assert_eq!(wanted_context(&s, q), 8192);
        s.model_prefs.insert("qwen3.5-9b".into(), crate::store::ModelPref { context_length: Some(999_999), reasoning: None });
        assert_eq!(wanted_context(&s, q), 262144);
        for bad in [0u32, 1, 1023] {
            s.model_prefs.insert("qwen3.5-9b".into(), crate::store::ModelPref { context_length: Some(bad), reasoning: None });
            assert_eq!(wanted_context(&s, q), 16384, "saved {bad} must fall back to the default");
        }
    }

    #[test]
    fn parse_load_response_reads_instance() {
        let v = serde_json::json!({"type":"llm","instance_id":"qwen3.5-9b:2","status":"loaded","load_config":{"context_length":16384}});
        let i = parse_load_response(&v, 4096).unwrap();
        assert_eq!((i.id.as_str(), i.context_length), ("qwen3.5-9b:2", 16384));
        let v = serde_json::json!({"instance_id":"x","status":"loaded"});
        assert_eq!(parse_load_response(&v, 4096).unwrap().context_length, 4096);
        assert!(parse_load_response(&serde_json::json!({}), 4096).is_err());
    }

    #[test]
    fn api_error_message_prefers_lm_studio_text() {
        let body = r#"{"error":{"type":"model_not_found","message":"Model nope not found"}}"#;
        assert_eq!(api_error_message(404, body), "Model nope not found");
        assert!(api_error_message(500, "oops").contains("500"));
    }

    #[test]
    fn ensure_not_loaded_refuses_loaded_models() {
        let m = fixture();
        assert_eq!(ensure_not_loaded(&m, "qwen3.5-9b").unwrap_err(), ALREADY_LOADED_MESSAGE);
        assert!(ensure_not_loaded(&m, "qwen3.6-35b-a3b").is_ok());
    }

    #[test]
    fn instance_without_context_is_kept_as_loaded() {
        let v = serde_json::json!({"models":[{"type":"llm","key":"k","display_name":"K",
            "loaded_instances":[{"id":"k","config":{}}],"max_context_length":4096}]});
        let m = parse_models(&v);
        assert_eq!(m[0].loaded, vec![LoadedInstance { id: "k".into(), context_length: 0 }]);
        assert!(matches!(plan_load(&m, "k", 32768), LoadPlan::SmallerLoaded { loaded: 0, .. }));
        assert!(ensure_not_loaded(&m, "k").is_err());
    }

    #[test]
    fn lms_path_ends_with_lms_exe() {
        if let Some(p) = lms_path() {
            assert!(p.ends_with("lms.exe"));
        }
    }

    /// 실제 LM Studio가 켜져 있을 때만: `cargo test --lib live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_list_models() {
        let s = AppSettings::default();
        assert!(server_running(&s).await);
        let m = list_models(&s).await.unwrap();
        for x in &m {
            println!("{} loaded={:?}", x.key, x.loaded);
        }
        assert!(!m.is_empty());
    }

    #[test]
    fn reasoning_fields_follow_model_and_pref() {
        let on_off = vec!["off".to_string(), "on".to_string()];
        assert!(reasoning_fields(&[], Some("off")).is_empty());
        assert!(reasoning_fields(&on_off, None).is_empty());
        assert!(reasoning_fields(&on_off, Some("on")).is_empty());
        assert_eq!(serde_json::Value::Object(reasoning_fields(&on_off, Some("off"))), REASONING_OFF_FIELDS.clone());
    }
}
