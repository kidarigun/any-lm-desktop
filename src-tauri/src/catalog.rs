//! Skill·MCP 찾아보기 목록(카탈로그) 불러오기와 목록에서 설치하기.
//!
//! 기본 원격 카탈로그(`DEFAULT_CATALOG_URL`) 또는 사용자가 지정한 HTTPS JSON을 불러온다.
//! 마지막으로 받은 목록을 앱 데이터 폴더에 저장해 두어, 인터넷이 안 될 때도 보여 준다.
//! 목록의 내용 검사와 화면 구성(내장 공식 웹 검색 MCP 병합 포함)은 웹뷰에서 하고, 여기서는 받아오기·해시 검사·설치만 맡는다.

use crate::skills;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_CATALOG_URL: &str = "https://raw.githubusercontent.com/kidarigun/exaone-catalog/main/catalog.json";
const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_SKILL_ZIP_BYTES: usize = 20 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

/// 비어 있는 주소는 기본 원격 카탈로그 주소로 해석한다.
pub fn resolve_url(url: &str) -> &str {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        DEFAULT_CATALOG_URL
    } else {
        trimmed
    }
}

/// 기본 원격 카탈로그의 레거시 앱 이름만 Any LM Desktop 표기로 정리한다 (항목은 절대 건드리지 않는다).
fn normalize_default_catalog_name(url: &str, default_url: &str, catalog: &mut Value) {
    if url != default_url {
        return;
    }
    if let Some(obj) = catalog.as_object_mut() {
        let has_legacy_brand = obj
            .get("name")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.contains("EXAONE") || s.contains("LG"));
        if has_legacy_brand {
            obj.insert("name".to_string(), Value::String("Any LM Desktop 기본 목록".to_string()));
        }
    }
}

/// https 주소만 받는다. (내 PC에서 시험할 때만 localhost http 허용)
pub fn check_url(url: &str) -> Result<(), String> {
    crate::mcp::validate_url(url).map_err(|_| "목록과 파일은 https:// 주소에서만 받을 수 있습니다.".to_string())
}

async fn fetch_bytes(url: &str, max: usize) -> Result<Vec<u8>, String> {
    check_url(url)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(FETCH_TIMEOUT)
        .user_agent(concat!("Any-LM-Desktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("받아오지 못했습니다. 인터넷 연결을 확인하세요. ({})", e))?;
    if !resp.status().is_success() {
        return Err(format!("받아오지 못했습니다 (HTTP {}).", resp.status().as_u16()));
    }
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("받는 중 오류: {}", e))? {
        buf.extend_from_slice(&chunk);
        if buf.len() > max {
            return Err(format!("파일이 너무 큽니다 ({}MB 넘음).", max / 1024 / 1024));
        }
    }
    Ok(buf)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResult {
    pub url: String,
    pub catalog: Value,
    /// 인터넷에서 받지 못해 저장해 둔 목록을 보여 주는지
    pub from_cache: bool,
    /// 받지 못한 이유 (저장본을 보여 줄 때)
    pub error: Option<String>,
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join("catalog_cache.json")
}

fn parse_catalog(bytes: &[u8]) -> Result<Value, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "목록 파일이 올바른 JSON이 아닙니다.".to_string())?;
    match v.get("version").and_then(|v| v.as_u64()) {
        Some(1) => Ok(v),
        Some(n) => Err(format!("이 앱이 모르는 목록 형식(version {})입니다. 앱을 업데이트하세요.", n)),
        None => Err("목록 파일에 version이 없습니다.".to_string()),
    }
}

/// 목록을 받아온다. 주소가 비어 있으면 기본 원격 카탈로그(`DEFAULT_CATALOG_URL`)를 사용한다.
/// 실패하면 같은 주소에서 마지막으로 받은 목록을 돌려준다.
pub async fn load(url: &str, cache_dir: &Path) -> Result<CatalogResult, String> {
    load_with_default(url, DEFAULT_CATALOG_URL, cache_dir).await
}

async fn load_with_default(url: &str, default_url: &str, cache_dir: &Path) -> Result<CatalogResult, String> {
    let trimmed = url.trim();
    let url = if trimmed.is_empty() { default_url } else { trimmed };
    let fetched = match fetch_bytes(url, MAX_CATALOG_BYTES).await {
        Ok(b) => parse_catalog(&b),
        Err(e) => Err(e),
    };
    match fetched {
        Ok(mut catalog) => {
            let _ = std::fs::create_dir_all(cache_dir);
            let _ = std::fs::write(
                cache_path(cache_dir),
                serde_json::to_vec(&serde_json::json!({ "url": url, "catalog": catalog })).unwrap_or_default(),
            );
            normalize_default_catalog_name(url, default_url, &mut catalog);
            Ok(CatalogResult { url: url.to_string(), catalog, from_cache: false, error: None })
        }
        Err(e) => {
            let cached = std::fs::read(cache_path(cache_dir))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .filter(|v| v.get("url").and_then(|u| u.as_str()) == Some(url))
                .and_then(|v| v.get("catalog").cloned());
            match cached {
                Some(mut catalog) => {
                    normalize_default_catalog_name(url, default_url, &mut catalog);
                    Ok(CatalogResult { url: url.to_string(), catalog, from_cache: true, error: Some(e) })
                }
                None => Err(e),
            }
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{:02x}", b)).collect()
}

/// 목록의 Skill zip을 받아 해시를 확인하고 설치한다.
pub async fn install_skill(url: &str, sha256: &str, overwrite: bool) -> Result<skills::SkillInfo, String> {
    let want = sha256.trim().to_ascii_lowercase();
    if want.len() != 64 || !want.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("목록에 이 Skill의 sha256이 없어 설치할 수 없습니다.".to_string());
    }
    let bytes = fetch_bytes(url, MAX_SKILL_ZIP_BYTES).await?;
    if sha256_hex(&bytes) != want {
        return Err("받은 파일이 목록에 적힌 것과 다릅니다(sha256 불일치). 목록이 갱신 중일 수 있으니 잠시 뒤 다시 시도하세요.".to_string());
    }
    tokio::task::spawn_blocking(move || {
        let tmp = std::env::temp_dir().join(format!("anylm-skill-{}.zip", uuid::Uuid::new_v4().simple()));
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        let res = skills::install_from_zip(&tmp, overwrite);
        let _ = std::fs::remove_file(&tmp);
        res
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtimes {
    pub node: bool,
    pub uv: bool,
    pub python: bool,
    pub docker: bool,
}

/// MCP 서버·Skill 실행에 필요한 프로그램이 이 PC에 있는지
pub async fn runtimes() -> Runtimes {
    Runtimes {
        node: crate::mcp::program_available("npx") && crate::mcp::program_available("node"),
        uv: crate::mcp::program_available("uvx"),
        python: crate::python_runner::find_python(None).await.is_some(),
        docker: crate::mcp::program_available("docker"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_needs_known_version() {
        assert!(parse_catalog(br#"{"version":1,"mcp":[]}"#).is_ok());
        assert!(parse_catalog(br#"{"version":2}"#).unwrap_err().contains("업데이트"));
        assert!(parse_catalog(br#"{"mcp":[]}"#).is_err());
        assert!(parse_catalog(b"<html>").is_err());
    }

    #[test]
    fn only_https_urls() {
        assert!(check_url(DEFAULT_CATALOG_URL).is_ok());
        assert!(check_url("https://example.com/catalog.json").is_ok());
        assert!(check_url("http://example.com/catalog.json").is_err());
        assert!(check_url("file:///C:/catalog.json").is_err());
    }

    #[test]
    fn sha256_matches_known_value() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn default_catalog_url_is_configured_and_resolved() {
        assert!(!DEFAULT_CATALOG_URL.is_empty());
        assert_eq!(resolve_url(""), DEFAULT_CATALOG_URL);
        assert_eq!(resolve_url("   "), DEFAULT_CATALOG_URL);
        assert_eq!(resolve_url(" https://example.com/custom.json "), "https://example.com/custom.json");
    }

    #[test]
    fn custom_catalog_keeps_its_own_name() {
        let mut custom = serde_json::json!({ "version": 1, "name": "내 EXAONE 사용자 목록", "skills": [], "mcp": [] });
        normalize_default_catalog_name("https://example.com/catalog.json", DEFAULT_CATALOG_URL, &mut custom);
        assert_eq!(custom["name"], "내 EXAONE 사용자 목록");
    }

    #[tokio::test]
    async fn falls_back_to_cached_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://127.0.0.1:1/catalog.json";
        // 저장본이 없으면 오류
        assert!(load(url, dir.path()).await.is_err());
        std::fs::write(
            cache_path(dir.path()),
            serde_json::to_vec(&serde_json::json!({ "url": url, "catalog": { "version": 1, "mcp": [] } })).unwrap(),
        )
        .unwrap();
        let r = load(url, dir.path()).await.unwrap();
        assert!(r.from_cache && r.error.is_some());
        // 다른 주소의 저장본은 쓰지 않는다
        assert!(load("https://127.0.0.1:1/other.json", dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn empty_url_uses_default_catalog_cache_not_empty_stub() {
        let dir = tempfile::tempdir().unwrap();
        let default_url = "https://127.0.0.1:1/catalog.json";
        // 저장본이 없으면 빈 스텁을 돌려주지 않고 오류를 반환한다
        assert!(load_with_default("", default_url, dir.path()).await.is_err());

        std::fs::write(
            cache_path(dir.path()),
            serde_json::to_vec(&serde_json::json!({
                "url": default_url,
                "catalog": {
                    "version": 1,
                    "name": "Desktop App for LG K-EXAONE 기본 목록 (테스트)",
                    "skills": [{ "id": "weekly-report", "name": "주간 보고서" }],
                    "mcp": [{ "id": "google-workspace", "name": "Google Workspace" }]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let r = load_with_default("", default_url, dir.path()).await.unwrap();
        assert!(r.from_cache);
        assert!(r.error.is_some());
        assert_eq!(r.url, default_url);
        assert_eq!(r.catalog["name"], "Any LM Desktop 기본 목록");
        assert_eq!(r.catalog["skills"].as_array().unwrap().len(), 1);
        assert_eq!(r.catalog["mcp"].as_array().unwrap().len(), 1);
    }

    /// 실제 기본 목록 확인: cargo test --lib live_default_catalog_has_skills_and_mcp -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_default_catalog_has_skills_and_mcp() {
        let dir = tempfile::tempdir().unwrap();
        let r = load("", dir.path()).await.unwrap();
        assert_eq!(r.url, DEFAULT_CATALOG_URL);
        assert!(!r.from_cache);
        assert_eq!(r.catalog["name"], "Any LM Desktop 기본 목록");
        let skills_len = r.catalog["skills"].as_array().map(|a| a.len()).unwrap_or(0);
        let mcp_len = r.catalog["mcp"].as_array().map(|a| a.len()).unwrap_or(0);
        eprintln!("live default catalog: skills={} mcp={}", skills_len, mcp_len);
        assert!(skills_len >= 50, "expected >= 50 skills, got {}", skills_len);
        assert!(mcp_len >= 15, "expected >= 15 mcp servers, got {}", mcp_len);
    }

    /// 실제 기본 또는 사용자 목록으로: cargo test --lib live_catalog -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_catalog_skill_install_and_remote_mcp() {
        let dir = tempfile::tempdir().unwrap();
        skills::set_user_dir(dir.path().join("skills"));
        let configured = std::env::var("ANYLM_CATALOG_URL").unwrap_or_else(|_| DEFAULT_CATALOG_URL.to_string());
        let base = reqwest::Url::parse(&configured).unwrap();
        let r = load(&configured, dir.path()).await.unwrap();
        assert!(!r.from_cache);
        let cat = r.catalog;
        let mut installed_url = None;
        for skill in cat["skills"].as_array().unwrap() {
            let url = base.join(skill["download"].as_str().unwrap()).unwrap();
            let info = install_skill(url.as_str(), skill["sha256"].as_str().unwrap(), false).await.unwrap();
            installed_url = Some(url);
            eprintln!("skill installed: {} ({} files) desc={}…", info.name, info.files.len(), info.description.chars().take(40).collect::<String>());
            assert!(info.error.is_none(), "{}: {:?}", info.name, info.error);
            assert_eq!(info.name, skill["skillName"].as_str().unwrap());
        }
        eprintln!("available to model: {}", skills::available(None).len());
        // 해시가 다르면 설치하지 않는다
        if let Some(url) = installed_url {
            assert!(install_skill(url.as_str(), &"0".repeat(64), true).await.unwrap_err().contains("sha256"));
        }

        for m in cat["mcp"].as_array().unwrap() {
            if m["install"]["transport"] != "http" || m["inputs"].as_array().is_some_and(|i| i.iter().any(|x| x["required"] != false)) {
                continue;
            }
            let cfg = crate::mcp::McpServerConfig {
                id: "t".into(),
                name: m["name"].as_str().unwrap().into(),
                transport: "http".into(),
                url: m["install"]["url"].as_str().unwrap().into(),
                ..Default::default()
            };
            match crate::mcp::Client::connect(&cfg, &crate::mcp::MemorySecrets::default(), None).await {
                Ok(c) => eprintln!("{}: OK {} tools={:?}", cfg.name, c.server_info, c.tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>()),
                Err(e) => eprintln!("{}: FAIL {}", cfg.name, e),
            }
        }
    }

    #[tokio::test]
    async fn rejects_bad_hash_before_downloading() {
        assert!(install_skill("https://127.0.0.1:1/x.zip", "abc", false).await.unwrap_err().contains("sha256"));
    }
}
