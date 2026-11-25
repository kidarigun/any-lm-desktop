//! 로컬 LLM 공급자 어댑터 (LM Studio · Ollama · OpenAI 호환) 공통 인터페이스 및 라우팅.

pub mod ollama;
pub mod openai_compat;

use crate::lmstudio::{self, LmModel, LoadPlan, LoadedInstance, TurnModelInfo};
use crate::store::AppSettings;
pub use crate::store::LlmProvider;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub supports_load: bool,
    pub supports_unload: bool,
    pub supports_start_server: bool,
    pub supports_reasoning_toggle: bool,
}

pub fn capabilities(provider: LlmProvider) -> ProviderCapabilities {
    match provider {
        LlmProvider::Lmstudio | LlmProvider::Ollama => ProviderCapabilities {
            supports_load: true,
            supports_unload: true,
            supports_start_server: true,
            supports_reasoning_toggle: true,
        },
        LlmProvider::OpenaiCompat => ProviderCapabilities {
            supports_load: false,
            supports_unload: false,
            supports_start_server: false,
            supports_reasoning_toggle: false,
        },
    }
}

/// 로컬 LLM 서버용 HTTP 클라이언트 설정. 대화·파일 내용이 PC 밖으로 나가지 않게
/// HTTP_PROXY·ALL_PROXY 같은 프록시 환경 변수를 무시하고(루프백도 프록시로 보낼 수 있다),
/// 리디렉션을 따르지 않는다(로컬 서버가 다른 주소로 돌려보내도 그대로 응답으로 본다).
pub fn local_http_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
}

/// 전체 요청 시간 제한이 있는 로컬 LLM 클라이언트.
pub fn local_http_client(timeout: std::time::Duration) -> Result<reqwest::Client, String> {
    local_http_builder().timeout(timeout).build().map_err(|e| e.to_string())
}

/// 이 PC의 로컬 서버(`http://localhost`, `127.0.0.1`, `[::1]`)만 허용한다.
pub fn validate_local_url(server_url: &str, provider: LlmProvider) -> Result<reqwest::Url, String> {
    let raw = server_url.trim();
    let label = provider.label();
    let url = reqwest::Url::parse(raw)
        .map_err(|_| format!("{label} 서버 주소가 올바르지 않습니다: {raw}"))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() != "http" || !local {
        return Err(format!(
            "{label} 서버 주소는 http://localhost, http://127.0.0.1, http://[::1] 만 쓸 수 있습니다."
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("서버 주소에 사용자 이름이나 비밀번호를 넣을 수 없습니다.".to_string());
    }
    Ok(url)
}

/// scheme·호스트·포트만으로 기준 주소를 만든다(경로·질의·조각은 버린다).
pub fn local_origin(server_url: &str, provider: LlmProvider) -> Result<String, String> {
    let url = validate_local_url(server_url, provider)?;
    let host = url.host_str().unwrap_or("localhost");
    Ok(match url.port() {
        Some(port) => format!("http://{host}:{port}"),
        None => format!("http://{host}"),
    })
}

pub fn validate_server_url(provider: LlmProvider, server_url: &str) -> Result<(), String> {
    match provider {
        LlmProvider::Lmstudio => lmstudio::server_urls(server_url).map(|_| ()),
        LlmProvider::Ollama => ollama::urls(server_url).map(|_| ()),
        LlmProvider::OpenaiCompat => openai_compat::urls(server_url).map(|_| ()),
    }
}

pub fn server_off_message(provider: LlmProvider) -> String {
    match provider {
        LlmProvider::Lmstudio => lmstudio::SERVER_OFF_MESSAGE.to_string(),
        LlmProvider::Ollama => {
            "Ollama 서버가 꺼져 있습니다. Ollama를 실행하거나 [서버 시작]을 눌러 주세요.".to_string()
        }
        LlmProvider::OpenaiCompat => {
            "OpenAI 호환 서버가 꺼져 있습니다. 로컬 서버(llama.cpp·vLLM 등)가 켜져 있는지 확인해 주세요."
                .to_string()
        }
    }
}

pub fn provider_subject(provider: LlmProvider) -> &'static str {
    match provider {
        LlmProvider::Lmstudio => "LM Studio가",
        LlmProvider::Ollama => "Ollama가",
        LlmProvider::OpenaiCompat => "OpenAI 호환 서버가",
    }
}

pub fn friendly_error(provider: LlmProvider, err: &reqwest::Error) -> String {
    if err.is_connect() {
        server_off_message(provider)
    } else {
        format!("{} 연결 오류: {}", provider.label(), err)
    }
}

/// 오류 본문(`{"error":{"message":..}}` 또는 Ollama의 `{"error":"..."}`)에서 문구를 뽑는다.
pub fn api_error_message(provider: LlmProvider, status: u16, body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
            if !msg.trim().is_empty() {
                return msg.to_string();
            }
        }
        if let Some(msg) = v.get("error").and_then(|m| m.as_str()) {
            if !msg.trim().is_empty() {
                return msg.to_string();
            }
        }
    }
    format!(
        "{} 오류 ({status}): {}",
        provider.label(),
        body.trim().chars().take(300).collect::<String>()
    )
}

pub fn chat_url(settings: &AppSettings) -> Result<String, String> {
    match settings.provider {
        LlmProvider::Lmstudio => lmstudio::server_urls(&settings.server_url).map(|u| u.chat),
        LlmProvider::Ollama => ollama::urls(&settings.server_url).map(|u| u.chat),
        LlmProvider::OpenaiCompat => openai_compat::urls(&settings.server_url).map(|u| u.chat),
    }
}

/// 에이전트 루프가 만든 OpenAI 모양 채팅 본문을 공급자에 보낼 본문으로 바꾼다.
/// Ollama는 고유 `/api/chat` 형식으로 바꾸고 `num_ctx`(=`context_length`)·`keep_alive`를 넣는다.
/// LM Studio·OpenAI 호환은 그대로 보낸다.
pub fn chat_body<'a>(settings: &AppSettings, body: &'a Value, context_length: u32) -> Cow<'a, Value> {
    match settings.provider {
        LlmProvider::Ollama => Cow::Owned(ollama::native_chat_body(body, context_length)),
        LlmProvider::Lmstudio | LlmProvider::OpenaiCompat => Cow::Borrowed(body),
    }
}

/// 채팅 스트림 줄을 에이전트 루프가 읽는 OpenAI 모양 SSE 줄로 맞춘다.
#[derive(Debug)]
pub enum ChatStream {
    /// 이미 SSE(`data: ...`)라 그대로 넘긴다.
    Sse,
    /// Ollama `/api/chat` NDJSON.
    Ollama(ollama::NativeStream),
}

impl ChatStream {
    pub fn new(provider: LlmProvider) -> Self {
        match provider {
            LlmProvider::Ollama => ChatStream::Ollama(ollama::NativeStream::default()),
            LlmProvider::Lmstudio | LlmProvider::OpenaiCompat => ChatStream::Sse,
        }
    }

    pub fn lines(&mut self, raw: Vec<String>) -> Vec<String> {
        match self {
            ChatStream::Sse => raw,
            ChatStream::Ollama(s) => raw.iter().flat_map(|l| s.translate_line(l)).collect(),
        }
    }
}

/// 스트림을 쓰지 않는 채팅 응답을 OpenAI 모양(`choices[0].message`)으로 맞춘다.
pub fn chat_response(provider: LlmProvider, v: Value) -> Value {
    match provider {
        LlmProvider::Ollama => ollama::native_response_to_openai(&v),
        LlmProvider::Lmstudio | LlmProvider::OpenaiCompat => v,
    }
}

/// 대화 턴 밖의 채팅 요청(대화 압축 요약)에 쓸 컨텍스트. 본문에 컨텍스트를 싣는 공급자(Ollama)만
/// 조회하고, 나머지는 0(쓰지 않음)을 돌려준다.
pub async fn request_context(settings: &AppSettings) -> u32 {
    match settings.provider {
        LlmProvider::Ollama => ollama::request_context(settings, &settings.model).await,
        LlmProvider::Lmstudio | LlmProvider::OpenaiCompat => 0,
    }
}

pub async fn list_models(settings: &AppSettings) -> Result<Vec<LmModel>, String> {
    match settings.provider {
        LlmProvider::Lmstudio => lmstudio::list_models(settings).await,
        LlmProvider::Ollama => ollama::list_models(settings).await,
        LlmProvider::OpenaiCompat => openai_compat::list_models(settings).await,
    }
}

pub async fn plan_load(settings: &AppSettings, key: &str) -> Result<LoadPlan, String> {
    match settings.provider {
        LlmProvider::Lmstudio => {
            let models = lmstudio::list_models(settings).await?;
            let wanted = models
                .iter()
                .find(|m| m.key == key)
                .map(|m| lmstudio::wanted_context(settings, m))
                .unwrap_or(lmstudio::DEFAULT_CONTEXT);
            Ok(lmstudio::plan_load(&models, key, wanted))
        }
        LlmProvider::Ollama => ollama::plan_load(settings, key).await,
        LlmProvider::OpenaiCompat => openai_compat::plan_load(settings, key).await,
    }
}

pub async fn load_model(
    settings: &AppSettings,
    key: &str,
    context_length: u32,
) -> Result<LoadedInstance, String> {
    match settings.provider {
        LlmProvider::Lmstudio => lmstudio::load_model(settings, key, context_length).await,
        LlmProvider::Ollama => ollama::load_model(settings, key, context_length).await,
        LlmProvider::OpenaiCompat => {
            Err("OpenAI 호환 서버는 앱에서 모델을 직접 로드할 수 없습니다.".to_string())
        }
    }
}

pub async fn unload_instance(settings: &AppSettings, instance_id: &str) -> Result<(), String> {
    match settings.provider {
        LlmProvider::Lmstudio => lmstudio::unload_instance(settings, instance_id).await,
        LlmProvider::Ollama => ollama::unload_instance(settings, instance_id).await,
        LlmProvider::OpenaiCompat => {
            Err("OpenAI 호환 서버는 앱에서 모델을 직접 언로드할 수 없습니다.".to_string())
        }
    }
}

pub async fn server_running(settings: &AppSettings) -> bool {
    match settings.provider {
        LlmProvider::Lmstudio => lmstudio::server_running(settings).await,
        LlmProvider::Ollama => ollama::server_running(settings).await,
        LlmProvider::OpenaiCompat => openai_compat::server_running(settings).await,
    }
}

pub async fn start_server(settings: &AppSettings) -> Result<(), String> {
    match settings.provider {
        LlmProvider::Lmstudio => tokio::task::spawn_blocking(lmstudio::start_server)
            .await
            .map_err(|e| e.to_string())?,
        LlmProvider::Ollama => ollama::start_server(settings).await,
        LlmProvider::OpenaiCompat => Err(
            "OpenAI 호환 서버는 앱에서 직접 시작할 수 없습니다. 로컬 서버를 직접 실행해 주세요."
                .to_string(),
        ),
    }
}

pub async fn turn_model_info(settings: &AppSettings, key: &str) -> TurnModelInfo {
    match settings.provider {
        LlmProvider::Lmstudio => lmstudio::turn_model_info(settings, key).await,
        LlmProvider::Ollama => ollama::turn_model_info(settings, key).await,
        LlmProvider::OpenaiCompat => openai_compat::turn_model_info(settings, key).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_client_returns_redirects_as_is() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // 돌려보낼 곳: 연결이 오면 표시한다.
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target.local_addr().unwrap();
        let followed = tokio::spawn(async move { target.accept().await.is_ok() });
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let reply = format!(
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{target_addr}/v1/models\r\n\
                 Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        });
        let client = local_http_client(std::time::Duration::from_secs(5)).unwrap();
        let resp = client.get(format!("http://{addr}/v1/models")).send().await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(resp.headers()["location"], format!("http://{target_addr}/v1/models"));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(!followed.is_finished(), "리디렉션을 따라가면 안 된다");
        followed.abort();
    }

    #[test]
    fn local_url_security_applies_to_all_providers() {
        for p in LlmProvider::ALL {
            assert!(validate_server_url(p, p.default_server_url()).is_ok());
            assert!(validate_server_url(p, "http://127.0.0.1:9000").is_ok());
            assert!(validate_server_url(p, "http://[::1]:9000").is_ok());

            assert!(validate_server_url(p, "http://192.168.0.10:11434").is_err());
            assert!(validate_server_url(p, "https://api.openai.com/v1").is_err());
            assert!(validate_server_url(p, "http://user:pw@localhost:8080").is_err());
            assert!(validate_server_url(p, "not a url").is_err());
        }
    }

    #[test]
    fn api_error_message_reads_object_and_ollama_string_errors() {
        assert_eq!(
            api_error_message(
                LlmProvider::Lmstudio,
                404,
                r#"{"error":{"message":"Model not found"}}"#
            ),
            "Model not found"
        );
        assert_eq!(
            api_error_message(
                LlmProvider::Ollama,
                404,
                r#"{"error":"model 'foo' not found"}"#
            ),
            "model 'foo' not found"
        );
        let fallback = api_error_message(LlmProvider::OpenaiCompat, 502, "bad gateway");
        assert!(fallback.contains("OpenAI 호환") && fallback.contains("502"), "{fallback}");
    }

    #[test]
    fn provider_capabilities_match_design() {
        assert!(capabilities(LlmProvider::Lmstudio).supports_load);
        assert!(capabilities(LlmProvider::Ollama).supports_load);
        assert!(capabilities(LlmProvider::Ollama).supports_start_server);
        let oai = capabilities(LlmProvider::OpenaiCompat);
        assert!(!oai.supports_load && !oai.supports_unload && !oai.supports_start_server);
    }
}
