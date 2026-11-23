//! LM Studio 로컬 서버 연결: 주소 검증, 인증 헤더, 연결 오류 안내.

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
    let base = raw.trim_end_matches('/');
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
}
