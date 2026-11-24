//! OpenAI 호환 로컬 서버 어댑터 (llama.cpp · vLLM · LocalAI 등).

use crate::lmstudio::{self, LmModel, LoadPlan, LoadedInstance, TurnModelInfo, MIN_CONTEXT};
use crate::providers::{api_error_message, friendly_error, validate_local_url, LlmProvider};
use crate::store::AppSettings;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatUrls {
    pub chat: String,
    pub models: String,
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

    Ok(OpenAiCompatUrls {
        chat: format!("{origin}{prefix}/chat/completions"),
        models: format!("{origin}{prefix}/models"),
    })
}

const QUICK_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())
}

/// `GET /v1/models` 응답에서 모델 목록을 읽는다 (`data` 또는 일부 서버의 `models` 배열 지원).
pub fn parse_models(v: &Value, settings: &AppSettings) -> Vec<LmModel> {
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
            let max_context_length = m
                .get("max_model_len")
                .or_else(|| m.get("context_length"))
                .or_else(|| m.pointer("/meta/n_ctx_train"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32;
            let pref_ctx = settings
                .model_prefs
                .get(&key)
                .and_then(|p| p.context_length)
                .filter(|&c| c >= MIN_CONTEXT);
            let effective_ctx = pref_ctx.unwrap_or(max_context_length);

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
    Ok(parse_models(&v, settings))
}

/// OpenAI 호환 서버는 이미 모델을 서빙 중인 것으로 보고 목록에 있으면 즉시 `Ready`를 돌려준다.
/// 일부 서버가 `/v1/models`에서 빈 목록을 돌려주는 경우, 사용자가 직접 입력한 모델 ID가 있으면 그대로 `Ready`로 진행한다.
pub async fn plan_load(settings: &AppSettings, key: &str) -> Result<LoadPlan, String> {
    let models = list_models(settings).await?;
    if models.is_empty() && !key.trim().is_empty() {
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
        loaded_context: pref_ctx.or_else(|| (m.max_context_length >= MIN_CONTEXT).then_some(m.max_context_length)),
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

        let models = parse_models(&payload, &settings);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].max_context_length, 65536);
        assert_eq!(models[0].loaded[0].context_length, 65536);
        assert_eq!(models[1].max_context_length, 32768);
        assert_eq!(models[2].max_context_length, 0);
        assert_eq!(models[2].loaded[0].context_length, 16384);
    }

    async fn mock_openai_server(models_reply: Value) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { break };
                let reply_payload = models_reply.clone();
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let svc = service_fn(move |req: Request<Incoming>| {
                        let reply_payload = reply_payload.clone();
                        async move {
                            let (status, reply) = if req.uri().path() == "/api/v1/models" {
                                (200, reply_payload)
                            } else {
                                (404, json!({ "error": { "message": "not found" } }))
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
    async fn mock_openai_compat_empty_models_allows_manual_model_id() {
        let url = mock_openai_server(json!({ "data": [] })).await;
        let mut settings = AppSettings {
            provider: LlmProvider::OpenaiCompat,
            server_url: url,
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
    }
}
