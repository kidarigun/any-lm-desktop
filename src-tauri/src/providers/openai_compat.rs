//! OpenAI 호환 로컬 서버 어댑터 (llama.cpp · vLLM · LocalAI 등).

use crate::lmstudio::{self, LmModel, LoadPlan, LoadedInstance, TurnModelInfo, DEFAULT_CONTEXT, MIN_CONTEXT};
use crate::providers::{api_error_message, friendly_error, validate_local_url, LlmProvider};
use crate::store::AppSettings;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatUrls {
    pub chat: String,
    pub models: String,
    /// llama.cpp 서버의 `GET /props`(실행 중 컨텍스트 `n_ctx`). `/v1` 앞 경로에 둔다.
    pub props: String,
}

/// 경로 접두어를 보존하되 끝의 `/v1`, `/chat/completions`, `/models`를 정규화한다.
pub fn urls(server_url: &str) -> Result<OpenAiCompatUrls, String> {
    let url = validate_local_url(server_url, LlmProvider::OpenaiCompat)?;
    let host = url.host_str().unwrap_or("localhost");
    let origin = match url.port() {
        Some(port) => format!("http://{host}:{port}"),
        None => format!("http://{host}"),
    };

    let mut path = url.path().trim_end_matches('/').to_string();
    if let Some(stripped) = path.strip_suffix("/chat/completions") {
        path = stripped.trim_end_matches('/').to_string();
    } else if let Some(stripped) = path.strip_suffix("/models") {
        path = stripped.trim_end_matches('/').to_string();
    }

    let prefix = if path.is_empty() || path == "/" {
        "/v1".to_string()
    } else if path == "/v1" || path.ends_with("/v1") {
        path
    } else {
        format!("{path}/v1")
    };

    let root = prefix.strip_suffix("/v1").unwrap_or(&prefix);
    Ok(OpenAiCompatUrls {
        chat: format!("{origin}{prefix}/chat/completions"),
        models: format!("{origin}{prefix}/models"),
        props: format!("{origin}{root}/props"),
    })
}

const QUICK_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    super::local_http_client(timeout)
}

/// llama.cpp 서버 `GET /props`의 실행 중 컨텍스트(`default_generation_settings.n_ctx`, 슬롯 하나의 크기).
pub fn parse_props_n_ctx(v: &Value) -> Option<u32> {
    v.pointer("/default_generation_settings/n_ctx")
        .or_else(|| v.get("n_ctx"))
        .and_then(Value::as_u64)
        .filter(|&n| n > 0)
        .map(|n| n.min(u32::MAX as u64) as u32)
}

/// llama.cpp식 목록인지(`meta`가 있는 항목이 있으면). 이런 서버만 `/props`를 묻는다.
pub fn looks_like_llama_cpp(v: &Value) -> bool {
    v.get("data")
        .or_else(|| v.get("models"))
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|m| m.get("meta").is_some_and(Value::is_object)))
}

fn as_ctx(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_u64)
        .filter(|&n| n > 0)
        .map(|n| n.min(u32::MAX as u64) as u32)
}

/// `GET /v1/models` 응답에서 모델 목록을 읽는다 (`data` 또는 일부 서버의 `models` 배열 지원).
///
/// 실행 중 컨텍스트(`loaded[0].context_length`)는 서버가 알려 준 값을 쓴다: vLLM `max_model_len`,
/// `context_length`, llama.cpp `/props`의 `n_ctx`(`runtime_n_ctx`). llama.cpp의 `meta.n_ctx_train`은
/// 모델을 학습한 길이일 뿐 서버의 `-c`(기본 4096)와 다르므로, `n_ctx`를 모르면
/// `min(n_ctx_train, DEFAULT_CONTEXT)`로 둔다. 사용자가 정한 컨텍스트는 서버가 알려 준 한도를 넘지 않는다.
pub fn parse_models(v: &Value, settings: &AppSettings, runtime_n_ctx: Option<u32>) -> Vec<LmModel> {
    let Some(arr) = v
        .get("data")
        .or_else(|| v.get("models"))
        .and_then(|d| d.as_array())
    else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            let key = m
                .get("id")
                .or_else(|| m.get("model"))
                .or_else(|| m.get("name"))
                .and_then(|s| s.as_str())
                .filter(|s| !s.trim().is_empty())?
                .to_string();
            let display_name = m
                .get("name")
                .and_then(|s| s.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&key)
                .to_string();
            let llama_cpp = m.get("meta").is_some_and(Value::is_object);
            let n_ctx_train = as_ctx(m.pointer("/meta/n_ctx_train"));
            // 서버가 실제로 받는 컨텍스트(알 때만)
            let runtime = as_ctx(m.get("max_model_len"))
                .or_else(|| as_ctx(m.get("context_length")))
                .or(if llama_cpp { runtime_n_ctx } else { None });
            let max_context_length = runtime.or(n_ctx_train).unwrap_or(0);
            let pref_ctx = settings
                .model_prefs
                .get(&key)
                .and_then(|p| p.context_length)
                .filter(|&c| c >= MIN_CONTEXT);
            let effective_ctx = match (pref_ctx, runtime) {
                (Some(p), Some(r)) => p.min(r),
                (Some(p), None) => n_ctx_train.map_or(p, |t| p.min(t)),
                (None, Some(r)) => r,
                (None, None) => n_ctx_train.map_or(0, |t| t.min(DEFAULT_CONTEXT)),
            };

            Some(LmModel {
                key: key.clone(),
                display_name,
                params: None,
                quantization: None,
                size_bytes: 0,
                max_context_length,
                loaded: vec![LoadedInstance {
                    id: key,
                    context_length: effective_ctx,
                }],
                vision: false,
                tool_use: true,
                reasoning_options: Vec::new(),
            })
        })
        .collect()
}

/// llama.cpp `GET /props`에서 실행 중 컨텍스트를 묻는다. 실패하면(다른 서버·꺼짐·느림) `None`.
async fn fetch_props_n_ctx(settings: &AppSettings, props_url: &str) -> Option<u32> {
    let c = client(PROBE_TIMEOUT).ok()?;
    let resp = lmstudio::authorize(c.get(props_url), &settings.api_key).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = serde_json::from_str(&resp.text().await.ok()?).ok()?;
    parse_props_n_ctx(&v)
}

pub async fn list_models(settings: &AppSettings) -> Result<Vec<LmModel>, String> {
    let u = urls(&settings.server_url)?;
    let resp = lmstudio::authorize(client(QUICK_TIMEOUT)?.get(&u.models), &settings.api_key)
        .send()
        .await
        .map_err(|e| friendly_error(LlmProvider::OpenaiCompat, &e))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| friendly_error(LlmProvider::OpenaiCompat, &e))?;
    if !status.is_success() {
        return Err(api_error_message(LlmProvider::OpenaiCompat, status.as_u16(), &body));
    }
    let v: Value = serde_json::from_str(&body)
        .map_err(|e| format!("OpenAI 호환 모델 목록을 읽지 못했습니다: {e}"))?;
    let runtime_n_ctx = if looks_like_llama_cpp(&v) {
        fetch_props_n_ctx(settings, &u.props).await
    } else {
        None
    };
    Ok(parse_models(&v, settings, runtime_n_ctx))
}

/// OpenAI 호환 서버는 이미 모델을 서빙 중인 것으로 보고 목록에 있으면 즉시 `Ready`를 돌려준다.
/// 일부 서버는 `/v1/models`에서 빈 목록을 돌려준다. 그때는 사용자가 이 공급자의 기본 모델로 직접 입력한
/// 모델 ID(`settings.model`)만 그대로 진행한다. 다른 이름(다른 공급자에서 고른 모델 등)은 서버가 아는지
/// 확인할 수 없으므로 `Ready`로 보내지 않고 오류를 돌려준다.
pub async fn plan_load(settings: &AppSettings, key: &str) -> Result<LoadPlan, String> {
    let models = list_models(settings).await?;
    if models.is_empty() {
        let key = key.trim();
        if key.is_empty() || key != settings.model.trim() {
            return Err(empty_list_message(key));
        }
        let pref_ctx = settings
            .model_prefs
            .get(key)
            .and_then(|p| p.context_length)
            .filter(|&c| c >= MIN_CONTEXT)
            .unwrap_or(0);
        return Ok(LoadPlan::Ready {
            instance_id: key.to_string(),
            context_length: pref_ctx,
        });
    }
    let Some(m) = models.into_iter().find(|m| m.key == key) else {
        return Ok(LoadPlan::Missing);
    };
    let context_length = m
        .loaded
        .first()
        .map(|i| i.context_length)
        .filter(|&c| c > 0)
        .unwrap_or(m.max_context_length);
    Ok(LoadPlan::Ready {
        instance_id: m.key,
        context_length,
    })
}

fn empty_list_message(key: &str) -> String {
    let subject = if key.is_empty() { "사용할 모델".to_string() } else { format!("'{key}' 모델") };
    format!(
        "OpenAI 호환 서버가 모델 목록을 비워 보내 {subject}이 서버에 있는지 확인할 수 없습니다. \
         [OpenAI 호환 연결]에서 이 서버의 모델 ID를 기본 모델로 입력해 주세요."
    )
}

pub async fn server_running(settings: &AppSettings) -> bool {
    let Ok(u) = urls(&settings.server_url) else {
        return false;
    };
    let Ok(c) = client(PROBE_TIMEOUT) else {
        return false;
    };
    lmstudio::authorize(c.get(&u.models), &settings.api_key)
        .send()
        .await
        .is_ok()
}

pub async fn turn_model_info(settings: &AppSettings, key: &str) -> TurnModelInfo {
    let pref_ctx = settings
        .model_prefs
        .get(key)
        .and_then(|p| p.context_length)
        .filter(|&c| c >= MIN_CONTEXT);
    let fallback = TurnModelInfo {
        reasoning_options: Vec::new(),
        loaded_context: pref_ctx,
        display_name: (!key.trim().is_empty()).then(|| key.to_string()),
    };
    let Ok(Ok(models)) = tokio::time::timeout(PROBE_TIMEOUT, list_models(settings)).await else {
        return fallback;
    };
    let Some(m) = models.into_iter().find(|m| m.key == key) else {
        return fallback;
    };
    TurnModelInfo {
        reasoning_options: Vec::new(),
        loaded_context: m
            .loaded
            .first()
            .map(|i| i.context_length)
            .filter(|&c| c >= MIN_CONTEXT)
            .or(pref_ctx),
        display_name: Some(m.display_name).filter(|n| !n.trim().is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ModelPref;
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;
    use serde_json::json;

    #[test]
    fn urls_normalize_v1_and_preserve_custom_prefixes() {
        let base = urls("http://localhost:8080").unwrap();
        assert_eq!(base.chat, "http://localhost:8080/v1/chat/completions");
        assert_eq!(base.models, "http://localhost:8080/v1/models");

        let v1 = urls("http://localhost:8080/v1/").unwrap();
        assert_eq!(v1, base);

        let full = urls("http://localhost:8080/v1/chat/completions?x=1").unwrap();
        assert_eq!(full, base);

        let api_v1 = urls("http://127.0.0.1:8080/api/v1").unwrap();
        assert_eq!(api_v1.chat, "http://127.0.0.1:8080/api/v1/chat/completions");
        assert_eq!(api_v1.models, "http://127.0.0.1:8080/api/v1/models");

        let custom = urls("http://localhost:8080/openai").unwrap();
        assert_eq!(custom.chat, "http://localhost:8080/openai/v1/chat/completions");
        assert_eq!(custom.models, "http://localhost:8080/openai/v1/models");
    }

    #[test]
    fn urls_put_props_beside_the_v1_prefix() {
        assert_eq!(urls("http://localhost:8080").unwrap().props, "http://localhost:8080/props");
        assert_eq!(urls("http://localhost:8080/v1/models").unwrap().props, "http://localhost:8080/props");
        assert_eq!(urls("http://127.0.0.1:8080/api/v1").unwrap().props, "http://127.0.0.1:8080/api/props");
    }

    #[test]
    fn parse_models_handles_vllm_llamacpp_and_user_context_override() {
        let payload = json!({
            "object": "list",
            "data": [
                { "id": "vllm-qwen", "max_model_len": 65536 },
                { "id": "llama-cpp-model", "meta": { "n_ctx_train": 32768 } },
                { "id": "plain-model" }
            ]
        });
        let mut settings = AppSettings {
            provider: LlmProvider::OpenaiCompat,
            ..Default::default()
        };
        settings
            .model_prefs
            .insert("plain-model".into(), ModelPref { context_length: Some(16384), reasoning: None });

        let models = parse_models(&payload, &settings, None);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].max_context_length, 65536);
        assert_eq!(models[0].loaded[0].context_length, 65536);
        // 학습 길이(n_ctx_train)만 알면 실행 중 컨텍스트로 보지 않는다: min(학습 길이, 기본값)
        assert_eq!(models[1].max_context_length, 32768);
        assert_eq!(models[1].loaded[0].context_length, DEFAULT_CONTEXT);
        assert_eq!(models[2].max_context_length, 0);
        assert_eq!(models[2].loaded[0].context_length, 16384);
    }

    #[test]
    fn llama_cpp_runtime_n_ctx_from_props_wins_over_training_length() {
        // llama.cpp 서버 README의 `GET /props` 예시 모양
        let props = json!({
            "default_generation_settings": {
                "id": 0, "id_task": -1, "n_ctx": 4096, "speculative": false, "is_processing": false,
                "params": { "n_predict": -1, "temperature": 0.8 }
            },
            "total_slots": 1,
            "model_path": "../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
            "modalities": { "vision": false }
        });
        assert_eq!(parse_props_n_ctx(&props), Some(4096));
        assert_eq!(parse_props_n_ctx(&json!({ "default_generation_settings": { "n_ctx": 0 } })), None);
        assert_eq!(parse_props_n_ctx(&json!({})), None);

        let payload = json!({
            "object": "list",
            "data": [{
                "id": "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf", "object": "model", "owned_by": "llamacpp",
                "meta": { "vocab_type": 2, "n_vocab": 128256, "n_ctx_train": 131072, "n_embd": 4096,
                          "n_params": 8030261312u64, "size": 4912898304u64 }
            }]
        });
        assert!(looks_like_llama_cpp(&payload));
        assert!(!looks_like_llama_cpp(&json!({ "data": [{ "id": "vllm", "max_model_len": 8192 }] })));
        let mut settings = AppSettings { provider: LlmProvider::OpenaiCompat, ..Default::default() };
        let m = &parse_models(&payload, &settings, Some(4096))[0];
        assert_eq!((m.max_context_length, m.loaded[0].context_length), (4096, 4096));
        let m = &parse_models(&payload, &settings, None)[0];
        assert_eq!((m.max_context_length, m.loaded[0].context_length), (131072, DEFAULT_CONTEXT));
        // 사용자가 정한 값은 서버의 n_ctx를 넘지 않는다.
        settings.model_prefs.insert(
            "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf".into(),
            ModelPref { context_length: Some(32768), reasoning: None },
        );
        assert_eq!(parse_models(&payload, &settings, Some(8192))[0].loaded[0].context_length, 8192);
        assert_eq!(parse_models(&payload, &settings, None)[0].loaded[0].context_length, 32768);
    }

    async fn mock_openai_server(models_reply: Value) -> String {
        mock_openai_server_with_props(models_reply, None).await
    }

    async fn mock_openai_server_with_props(models_reply: Value, props: Option<Value>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { break };
                let reply_payload = models_reply.clone();
                let props = props.clone();
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let reply_payload = reply_payload.clone();
                        let props = props.clone();
                        async move {
                            let (status, reply) = match (req.uri().path(), props) {
                                ("/api/v1/models", _) => (200, reply_payload),
                                ("/api/props", Some(p)) => (200, p),
                                _ => (404, json!({ "error": { "message": "not found" } })),
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
        format!("http://{addr}/api/v1")
    }

    #[tokio::test]
    async fn mock_llama_cpp_reports_props_n_ctx_as_turn_context() {
        let models = json!({ "data": [{ "id": "gguf", "owned_by": "llamacpp", "meta": { "n_ctx_train": 131072 } }] });
        let props = json!({ "default_generation_settings": { "n_ctx": 4096 } });
        let url = mock_openai_server_with_props(models.clone(), Some(props)).await;
        let settings = AppSettings { provider: LlmProvider::OpenaiCompat, server_url: url, ..Default::default() };
        assert!(matches!(plan_load(&settings, "gguf").await.unwrap(), LoadPlan::Ready { context_length: 4096, .. }));
        assert_eq!(turn_model_info(&settings, "gguf").await.loaded_context, Some(4096));

        // /props가 없는 서버: 학습 길이 대신 기본값으로 줄인다.
        let url = mock_openai_server_with_props(models, None).await;
        let settings = AppSettings { provider: LlmProvider::OpenaiCompat, server_url: url, ..Default::default() };
        assert_eq!(turn_model_info(&settings, "gguf").await.loaded_context, Some(DEFAULT_CONTEXT));
    }

    #[tokio::test]
    async fn mock_openai_compat_list_plan_and_turn_info() {
        let url = mock_openai_server(json!({
            "data": [{ "id": "local-coder", "max_model_len": 32768 }]
        }))
        .await;
        let mut settings = AppSettings {
            provider: LlmProvider::OpenaiCompat,
            server_url: url,
            ..Default::default()
        };
        assert!(server_running(&settings).await);
        assert!(matches!(
            plan_load(&settings, "local-coder").await.unwrap(),
            LoadPlan::Ready { context_length: 32768, .. }
        ));
        assert!(matches!(
            plan_load(&settings, "missing-model").await.unwrap(),
            LoadPlan::Missing
        ));

        settings
            .model_prefs
            .insert("local-coder".into(), ModelPref { context_length: Some(16384), reasoning: None });
        let info = turn_model_info(&settings, "local-coder").await;
        assert_eq!(info.loaded_context, Some(16384));
        assert_eq!(info.display_name.as_deref(), Some("local-coder"));

        assert!(crate::providers::load_model(&settings, "local-coder", 16384).await.is_err());
        assert!(crate::providers::start_server(&settings).await.is_err());
    }

    #[tokio::test]
    async fn mock_openai_compat_empty_models_allows_only_the_configured_model_id() {
        let url = mock_openai_server(json!({ "data": [] })).await;
        let mut settings = AppSettings {
            provider: LlmProvider::OpenaiCompat,
            server_url: url,
            model: "manual-model".into(),
            ..Default::default()
        };
        assert!(matches!(
            plan_load(&settings, "manual-model").await.unwrap(),
            LoadPlan::Ready { context_length: 0, .. }
        ));
        settings
            .model_prefs
            .insert("manual-model".into(), ModelPref { context_length: Some(16384), reasoning: None });
        assert!(matches!(
            plan_load(&settings, "manual-model").await.unwrap(),
            LoadPlan::Ready { context_length: 16384, .. }
        ));
        // 다른 공급자에서 고른 모델 이름은 서버가 아는지 모르므로 Ready로 보내지 않는다.
        let err = plan_load(&settings, "qwen3.5-9b").await.unwrap_err();
        assert!(err.contains("'qwen3.5-9b' 모델이") && err.contains("[OpenAI 호환 연결]"), "{err}");
        settings.model.clear();
        assert!(plan_load(&settings, "").await.unwrap_err().contains("사용할 모델이"));
    }
}
