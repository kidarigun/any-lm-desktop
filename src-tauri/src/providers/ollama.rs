//! Ollama 로컬 서버 어댑터: `/api/tags`, `/api/ps`, `/api/show`, `/api/generate`, `/api/version`, `/v1/chat/completions`.

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
        chat: format!("{base}/v1/chat/completions"),
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
        "keep_alive": "5m",
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
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub async fn start_server(settings: &AppSettings) -> Result<(), String> {
    if server_running(settings).await {
        return Ok(());
    }
    let bin = ollama_path().ok_or_else(|| {
        "Ollama 실행 파일을 찾지 못했습니다. Ollama를 설치한 뒤 다시 시도해 주세요.".to_string()
    })?;
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("serve")
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
    let loaded_context = ps_ctx.or(pref_ctx);
    let reasoning_options = show.map(|(_, s)| s.reasoning_options).unwrap_or_default();
    TurnModelInfo {
        reasoning_options,
        loaded_context,
        display_name: (!key.trim().is_empty()).then(|| key.to_string()),
    }
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
        assert_eq!(u.chat, "http://localhost:11434/v1/chat/completions");
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
        assert_eq!(info.reasoning_options, vec!["off", "on"]);
        assert_eq!(info.display_name.as_deref(), Some("qwen3:8b"));
    }
}
