//! 원격 MCP 서버의 OAuth 로그인 (Authorization Code + PKCE, 내 PC 주소로 결과 받기).
//!
//! - 목록·사용자가 엔드포인트와 클라이언트 ID를 주면 그대로 쓴다 (Google처럼 직접 만든 클라이언트).
//! - 주지 않으면 MCP 권한 규약대로 찾는다: 401 응답의 `resource_metadata` 또는
//!   `/.well-known/oauth-protected-resource` → 권한 서버 메타데이터 → (클라이언트가 없으면) 동적 등록.
//! - 로그인 결과는 `http://localhost:45789/oauth2callback`으로 받는다 (웹 애플리케이션 클라이언트에 이 주소를 등록).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

pub const CALLBACK_PORT: u16 = 45789;
pub const CALLBACK_PATH: &str = "/oauth2callback";
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

pub fn redirect_uri() -> String {
    format!("http://localhost:{}{}", CALLBACK_PORT, CALLBACK_PATH)
}

/// 서버 설정에 들어가는 로그인 정보 (비밀 값은 자격 증명 관리자에)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OAuthSettings {
    /// 비우면 동적 등록을 시도한다
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 비우면 서버에서 찾는다
    #[serde(default)]
    pub authorization_endpoint: String,
    #[serde(default)]
    pub token_endpoint: String,
    /// 로그인 주소에 덧붙일 값 (예: Google의 access_type=offline)
    #[serde(default)]
    pub extra_params: BTreeMap<String, String>,
    /// 같은 묶음 이름 + 같은 클라이언트 ID인 서버끼리 로그인을 함께 쓴다 (예: "google")
    #[serde(default)]
    pub group: String,
}

/// 받은 토큰 (자격 증명 관리자에 JSON으로 저장)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TokenSet {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// 만료 시각 (유닉스 초)
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// 허락받은 권한
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 갱신할 때 필요한 정보
    #[serde(default)]
    pub token_endpoint: String,
    #[serde(default)]
    pub client_id: String,
}

impl TokenSet {
    pub fn is_fresh(&self, now: i64) -> bool {
        !self.access_token.is_empty() && self.expires_at.map_or(true, |t| t - 60 > now)
    }

    /// 필요한 권한을 모두 받았는지 (서버가 받은 권한을 알려 주지 않았으면 그렇다고 본다)
    pub fn covers(&self, scopes: &[String]) -> bool {
        self.scopes.is_empty() || scopes.iter().all(|s| self.scopes.contains(s))
    }

    /// 자격 증명 관리자 한 칸에 들어가도록, 길면 접근 토큰은 빼고 저장한다 (갱신 토큰으로 다시 받는다).
    pub fn for_storage(&self) -> String {
        let full = serde_json::to_string(self).unwrap_or_default();
        if full.len() <= 1200 || self.refresh_token.is_none() {
            return full;
        }
        serde_json::to_string(&TokenSet { access_token: String::new(), expires_at: None, ..self.clone() }).unwrap_or_default()
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Endpoints {
    pub authorization: String,
    pub token: String,
    pub registration: Option<String>,
    /// MCP 규약의 resource 값 (발견한 경우에만 보낸다)
    pub resource: Option<String>,
    /// 서버가 알려 준 권한 (설정에 권한이 없을 때 쓴다)
    pub scopes_supported: Vec<String>,
}

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())
}

fn check_https(url: &str, what: &str) -> Result<(), String> {
    crate::mcp::validate_url(url).map_err(|_| format!("{} 주소는 https://여야 합니다: {}", what, url))
}

async fn get_json(client: &reqwest::Client, url: &str) -> Option<Value> {
    let r = client.get(url).header("Accept", "application/json").send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.json::<Value>().await.ok()
}

/// `Bearer resource_metadata="…"` 에서 주소를 꺼낸다.
fn resource_metadata_from_header(h: &str) -> Option<String> {
    let i = h.find("resource_metadata=")?;
    let rest = &h[i + "resource_metadata=".len()..];
    let v = if let Some(r) = rest.strip_prefix('"') { r.split('"').next()? } else { rest.split([',', ' ']).next()? };
    Some(v.to_string())
}

/// 권한 서버 메타데이터 주소 후보 (RFC 8414 경로 삽입 → OIDC)
fn metadata_urls(issuer: &str) -> Vec<String> {
    let Ok(u) = reqwest::Url::parse(issuer) else { return Vec::new() };
    let origin = u.origin().ascii_serialization();
    let path = u.path().trim_end_matches('/');
    let mut out = vec![format!("{}/.well-known/oauth-authorization-server{}", origin, path)];
    if path.is_empty() {
        out.push(format!("{}/.well-known/openid-configuration", origin));
    } else {
        out.push(format!("{}/.well-known/openid-configuration{}", origin, path));
        out.push(format!("{}{}/.well-known/openid-configuration", origin, path));
    }
    out
}

/// 로그인에 쓸 주소를 정한다.
pub async fn discover(server_url: &str, s: &OAuthSettings) -> Result<Endpoints, String> {
    if !s.authorization_endpoint.is_empty() && !s.token_endpoint.is_empty() {
        check_https(&s.authorization_endpoint, "로그인")?;
        check_https(&s.token_endpoint, "토큰")?;
        return Ok(Endpoints {
            authorization: s.authorization_endpoint.clone(),
            token: s.token_endpoint.clone(),
            ..Default::default()
        });
    }
    let client = http()?;
    let server = reqwest::Url::parse(server_url).map_err(|_| "서버 주소가 올바르지 않습니다.".to_string())?;
    let origin = server.origin().ascii_serialization();

    // 1) 보호된 자원 메타데이터 (RFC 9728)
    let mut prm: Option<Value> = None;
    let probe = client
        .post(server_url)
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": crate::mcp::PROTOCOL_VERSION, "capabilities": {}, "clientInfo": { "name": "Desktop App for LG K-EXAONE", "version": env!("CARGO_PKG_VERSION") } } }))
        .send()
        .await
        .ok();
    if let Some(url) = probe
        .as_ref()
        .and_then(|r| r.headers().get("www-authenticate"))
        .and_then(|h| h.to_str().ok())
        .and_then(resource_metadata_from_header)
    {
        prm = get_json(&client, &url).await;
    }
    if prm.is_none() {
        let path = server.path().trim_end_matches('/');
        for url in [
            format!("{}/.well-known/oauth-protected-resource{}", origin, path),
            format!("{}/.well-known/oauth-protected-resource", origin),
        ] {
            if let Some(v) = get_json(&client, &url).await {
                prm = Some(v);
                break;
            }
        }
    }
    let issuer = prm
        .as_ref()
        .and_then(|p| p.get("authorization_servers"))
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| origin.clone());
    let resource = prm.as_ref().and_then(|p| p.get("resource")).and_then(|r| r.as_str()).map(String::from);
    let scopes_supported: Vec<String> = prm
        .as_ref()
        .and_then(|p| p.get("scopes_supported"))
        .and_then(|s| s.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // 2) 권한 서버 메타데이터 (RFC 8414 / OIDC)
    let mut meta: Option<Value> = None;
    for url in metadata_urls(&issuer) {
        if let Some(v) = get_json(&client, &url).await {
            meta = Some(v);
            break;
        }
    }
    let meta = meta.ok_or_else(|| {
        "이 서버는 로그인 방법을 알려 주지 않습니다. 서버 안내에 있는 OAuth 클라이언트 ID와 로그인·토큰 주소를 직접 입력하세요.".to_string()
    })?;
    let field = |k: &str| meta.get(k).and_then(|v| v.as_str()).map(String::from);
    let authorization = field("authorization_endpoint").ok_or("로그인 주소(authorization_endpoint)를 찾지 못했습니다.")?;
    let token = field("token_endpoint").ok_or("토큰 주소(token_endpoint)를 찾지 못했습니다.")?;
    check_https(&authorization, "로그인")?;
    check_https(&token, "토큰")?;
    Ok(Endpoints { authorization, token, registration: field("registration_endpoint"), resource, scopes_supported })
}

/// 동적 클라이언트 등록 (RFC 7591)
pub async fn register_client(registration: &str) -> Result<(String, Option<String>), String> {
    check_https(registration, "클라이언트 등록")?;
    let r = http()?
        .post(registration)
        .json(&serde_json::json!({
            "client_name": "Desktop App for LG K-EXAONE",
            "redirect_uris": [redirect_uri()],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none"
        }))
        .send()
        .await
        .map_err(|e| format!("클라이언트를 등록하지 못했습니다: {}", e))?;
    if !r.status().is_success() {
        return Err(format!(
            "이 서버는 자동 등록을 받지 않습니다 (HTTP {}). 서버 안내에 따라 OAuth 클라이언트 ID를 직접 입력하세요.",
            r.status().as_u16()
        ));
    }
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    let id = v.get("client_id").and_then(|v| v.as_str()).ok_or("등록 응답에 client_id가 없습니다.")?;
    Ok((id.to_string(), v.get("client_secret").and_then(|v| v.as_str()).map(String::from)))
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn pkce() -> Pkce {
    use base64::Engine;
    use sha2::Digest;
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
    Pkce { verifier, challenge }
}

pub fn authorization_url(
    ep: &Endpoints,
    client_id: &str,
    scopes: &[String],
    state: &str,
    challenge: &str,
    extra: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut u = reqwest::Url::parse(&ep.authorization).map_err(|_| "로그인 주소가 올바르지 않습니다.".to_string())?;
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &redirect_uri())
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        if !scopes.is_empty() {
            q.append_pair("scope", &scopes.join(" "));
        }
        if let Some(r) = &ep.resource {
            q.append_pair("resource", r);
        }
        for (k, v) in extra {
            q.append_pair(k, v);
        }
    }
    Ok(u.to_string())
}

fn token_from_response(v: &Value, prev: Option<&TokenSet>, requested: &[String], ep_token: &str, client_id: &str) -> Result<TokenSet, String> {
    if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
        let d = v.get("error_description").and_then(|d| d.as_str()).unwrap_or("");
        return Err(format!("로그인 서버 오류: {} {}", e, d).trim().to_string());
    }
    let access = v.get("access_token").and_then(|a| a.as_str()).ok_or("토큰 응답에 access_token이 없습니다.")?;
    let scopes = match v.get("scope").and_then(|s| s.as_str()) {
        Some(s) => s
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect(),
        None => prev.map(|p| p.scopes.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| requested.to_vec()),
    };
    Ok(TokenSet {
        access_token: access.to_string(),
        // 갱신 응답에 새 갱신 토큰이 없으면 이전 것을 계속 쓴다.
        refresh_token: v
            .get("refresh_token")
            .and_then(|r| r.as_str())
            .map(String::from)
            .or_else(|| prev.and_then(|p| p.refresh_token.clone())),
        expires_at: v.get("expires_in").and_then(|e| e.as_i64()).map(|s| now_secs() + s),
        scopes,
        token_endpoint: ep_token.to_string(),
        client_id: client_id.to_string(),
    })
}

async fn post_token(token_endpoint: &str, form: &[(&str, &str)]) -> Result<Value, String> {
    let r = http()?
        .post(token_endpoint)
        .header("Accept", "application/json")
        .form(form)
        .send()
        .await
        .map_err(|e| format!("토큰을 받지 못했습니다: {}", e))?;
    let status = r.status();
    let text = r.text().await.unwrap_or_default();
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => Ok(v),
        Err(_) => Err(format!("토큰 응답을 해석하지 못했습니다 (HTTP {}).", status.as_u16())),
    }
}

pub async fn exchange_code(
    ep: &Endpoints,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    verifier: &str,
    requested: &[String],
) -> Result<TokenSet, String> {
    let redirect = redirect_uri();
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect.as_str()),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];
    if let Some(s) = client_secret.filter(|s| !s.is_empty()) {
        form.push(("client_secret", s));
    }
    if let Some(r) = &ep.resource {
        form.push(("resource", r.as_str()));
    }
    let v = post_token(&ep.token, &form).await?;
    token_from_response(&v, None, requested, &ep.token, client_id)
}

pub async fn refresh(t: &TokenSet, client_secret: Option<&str>) -> Result<TokenSet, String> {
    let rt = t.refresh_token.as_deref().ok_or("갱신 토큰이 없어 다시 로그인해야 합니다.")?;
    let mut form = vec![("grant_type", "refresh_token"), ("refresh_token", rt), ("client_id", t.client_id.as_str())];
    if let Some(s) = client_secret.filter(|s| !s.is_empty()) {
        form.push(("client_secret", s));
    }
    let v = post_token(&t.token_endpoint, &form).await?;
    token_from_response(&v, Some(t), &t.scopes, &t.token_endpoint, &t.client_id)
}

fn page(title: &str, body: &str) -> String {
    let html = format!(
        "<!doctype html><html lang=\"ko\"><meta charset=\"utf-8\"><title>{t}</title><body style=\"font-family:sans-serif;max-width:480px;margin:80px auto;text-align:center;color:#222\"><h2>{t}</h2><p>{b}</p></body></html>",
        t = title,
        b = body
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 로그인 결과(code)를 내 PC 주소에서 기다린다.
pub async fn wait_for_code(state: &str, mut cancel: oneshot::Receiver<()>) -> Result<String, String> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT)).await.map_err(|e| {
        format!(
            "로그인 결과를 받을 {}번 포트를 열 수 없습니다. 다른 로그인이 진행 중이거나 다른 프로그램이 쓰고 있습니다. ({})",
            CALLBACK_PORT, e
        )
    })?;
    // 브라우저가 localhost를 IPv6로 찾을 때를 위해 [::1]도 연다 (안 되면 무시).
    let listener6 = tokio::net::TcpListener::bind(("::1", CALLBACK_PORT)).await.ok();
    let deadline = tokio::time::Instant::now() + LOGIN_TIMEOUT;
    loop {
        let accept6 = async {
            match &listener6 {
                Some(l) => l.accept().await,
                None => std::future::pending().await,
            }
        };
        let (mut sock, _) = tokio::select! {
            r = listener.accept() => r.map_err(|e| e.to_string())?,
            r = accept6 => r.map_err(|e| e.to_string())?,
            _ = &mut cancel => return Err("로그인을 취소했습니다.".to_string()),
            _ = tokio::time::sleep_until(deadline) => return Err("로그인이 5분 안에 끝나지 않아 멈췄습니다. 다시 시도하세요.".to_string()),
        };
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf)).await.ok().and_then(|r| r.ok()).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let target = req.split_whitespace().nth(1).unwrap_or("").to_string();
        let Ok(u) = reqwest::Url::parse(&format!("http://localhost{}", target)) else { continue };
        if u.path() != CALLBACK_PATH {
            let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            continue;
        }
        let q: BTreeMap<String, String> = u.query_pairs().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        if q.get("state").map(String::as_str) != Some(state) {
            let _ = sock.write_all(page("로그인 확인 실패", "요청 정보가 맞지 않습니다. 앱에서 다시 로그인하세요.").as_bytes()).await;
            continue;
        }
        if let Some(e) = q.get("error") {
            let _ = sock.write_all(page("로그인하지 않았습니다", "이 창을 닫고 앱으로 돌아가세요.").as_bytes()).await;
            let desc = q.get("error_description").map(|d| format!(" ({})", d)).unwrap_or_default();
            return Err(if e == "access_denied" {
                "로그인을 취소했거나 권한을 허락하지 않았습니다.".to_string()
            } else {
                format!("로그인 오류: {}{}", html_escape(e), desc)
            });
        }
        let Some(code) = q.get("code") else { continue };
        let _ = sock
            .write_all(page("로그인 완료", "이 창을 닫고 앱으로 돌아가세요.").as_bytes())
            .await;
        let _ = sock.shutdown().await;
        return Ok(code.clone());
    }
}

/// 기본 브라우저로 주소를 연다.
pub fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(url)
            .spawn()
            .map_err(|e| format!("브라우저를 열 수 없습니다: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        let _ = url;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_resource_metadata_header() {
        assert_eq!(
            resource_metadata_from_header(r#"Bearer error="invalid_token", resource_metadata="https://a.com/.well-known/oauth-protected-resource""#).as_deref(),
            Some("https://a.com/.well-known/oauth-protected-resource")
        );
        assert_eq!(resource_metadata_from_header("Bearer resource_metadata=https://b.com/x, error=y").as_deref(), Some("https://b.com/x"));
        assert!(resource_metadata_from_header("Bearer").is_none());
    }

    #[test]
    fn metadata_url_candidates() {
        assert_eq!(
            metadata_urls("https://auth.example.com"),
            vec!["https://auth.example.com/.well-known/oauth-authorization-server", "https://auth.example.com/.well-known/openid-configuration"]
        );
        assert_eq!(metadata_urls("https://ex.com/tenant1")[0], "https://ex.com/.well-known/oauth-authorization-server/tenant1");
    }

    #[test]
    fn builds_authorization_url_with_pkce() {
        let ep = Endpoints {
            authorization: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            ..Default::default()
        };
        let p = pkce();
        assert!(p.verifier.len() >= 43 && !p.challenge.contains('='));
        let extra = BTreeMap::from([("access_type".to_string(), "offline".to_string())]);
        let url = authorization_url(&ep, "cid", &["a b".into(), "c".into()], "st", &p.challenge, &extra).unwrap();
        let u = reqwest::Url::parse(&url).unwrap();
        let q: BTreeMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["redirect_uri"], "http://localhost:45789/oauth2callback");
        assert_eq!(q["scope"], "a b c");
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["code_challenge_method"], "S256");
        assert!(!q.contains_key("resource"));
    }

    #[test]
    fn token_storage_and_freshness() {
        let t = TokenSet {
            access_token: "x".repeat(2000),
            refresh_token: Some("r".into()),
            expires_at: Some(now_secs() + 3600),
            scopes: vec!["s1".into(), "s2".into()],
            ..Default::default()
        };
        assert!(t.is_fresh(now_secs()));
        assert!(!t.is_fresh(now_secs() + 3590));
        assert!(t.covers(&["s1".into()]) && !t.covers(&["s3".into()]));
        let stored: TokenSet = serde_json::from_str(&t.for_storage()).unwrap();
        assert!(stored.access_token.is_empty() && stored.refresh_token.as_deref() == Some("r"));

        let v = serde_json::json!({ "access_token": "new", "expires_in": 3600 });
        let r = token_from_response(&v, Some(&t), &[], "https://t", "c").unwrap();
        assert_eq!(r.refresh_token.as_deref(), Some("r"), "갱신 응답에 새 갱신 토큰이 없으면 이전 것을 쓴다");
        assert_eq!(r.scopes, t.scopes);
        let gh = token_from_response(
            &serde_json::json!({ "access_token": "gho_1", "scope": "repo,read:user" }),
            None,
            &[],
            "https://github.com/login/oauth/access_token",
            "cid",
        )
        .unwrap();
        assert_eq!(gh.scopes, vec!["repo", "read:user"]);
        assert!(gh.covers(&["repo".into(), "read:user".into()]));
        assert!(token_from_response(&serde_json::json!({ "error": "invalid_grant" }), None, &[], "", "").is_err());
    }
}
