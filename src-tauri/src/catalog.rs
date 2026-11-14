//! Skill·MCP 찾아보기 목록(카탈로그) 불러오기와 목록에서 설치하기.
//!
//! 목록은 https 주소의 JSON 파일 하나다 (기본: GitHub 저장소의 catalog.json).
//! 마지막으로 받은 목록을 앱 데이터 폴더에 저장해 두어, 인터넷이 안 될 때도 보여 준다.
//! 목록의 내용 검사와 화면 구성은 웹뷰에서 하고, 여기서는 받아오기·해시 검사·설치만 맡는다.

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

/// https 주소만 받는다. (내 PC에서 시험할 때만 localhost http 허용)
pub fn check_url(url: &str) -> Result<(), String> {
    crate::mcp::validate_url(url).map_err(|_| "목록과 파일은 https:// 주소에서만 받을 수 있습니다.".to_string())
}

async fn fetch_bytes(url: &str, max: usize) -> Result<Vec<u8>, String> {
    check_url(url)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(FETCH_TIMEOUT)
        .user_agent(concat!("EXAONE-Desktop/", env!("CARGO_PKG_VERSION")))
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

/// 목록을 받아온다. 실패하면 같은 주소에서 마지막으로 받은 목록을 돌려준다.
pub async fn load(url: &str, cache_dir: &Path) -> Result<CatalogResult, String> {
    let fetched = match fetch_bytes(url, MAX_CATALOG_BYTES).await {
        Ok(b) => parse_catalog(&b),
        Err(e) => Err(e),
    };
    match fetched {
        Ok(catalog) => {
            let _ = std::fs::create_dir_all(cache_dir);
            let _ = std::fs::write(
                cache_path(cache_dir),
                serde_json::to_vec(&serde_json::json!({ "url": url, "catalog": catalog })).unwrap_or_default(),
            );
            Ok(CatalogResult { url: url.to_string(), catalog, from_cache: false, error: None })
        }
        Err(e) => {
            let cached = std::fs::read(cache_path(cache_dir))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .filter(|v| v.get("url").and_then(|u| u.as_str()) == Some(url))
                .and_then(|v| v.get("catalog").cloned());
            match cached {
                Some(catalog) => Ok(CatalogResult { url: url.to_string(), catalog, from_cache: true, error: Some(e) }),
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
        let tmp = std::env::temp_dir().join(format!("exaone-skill-{}.zip", uuid::Uuid::new_v4().simple()));
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
        assert!(check_url("http://example.com/catalog.json").is_err());
        assert!(check_url("file:///C:/catalog.json").is_err());
    }

    #[test]
    fn sha256_matches_known_value() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
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

    /// 실제 기본 목록으로: cargo test --lib live_ -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_catalog_skill_install_and_remote_mcp() {
        let dir = tempfile::tempdir().unwrap();
        skills::set_user_dir(dir.path().join("skills"));
        let r = load(DEFAULT_CATALOG_URL, dir.path()).await.unwrap();
        assert!(!r.from_cache);
        let cat = r.catalog;
        let mut url = String::new();
        for skill in cat["skills"].as_array().unwrap() {
            url = format!("https://raw.githubusercontent.com/kidarigun/exaone-catalog/main/{}", skill["download"].as_str().unwrap());
            let info = install_skill(&url, skill["sha256"].as_str().unwrap(), false).await.unwrap();
            eprintln!("skill installed: {} ({} files) desc={}…", info.name, info.files.len(), info.description.chars().take(40).collect::<String>());
            assert!(info.error.is_none(), "{}: {:?}", info.name, info.error);
            assert_eq!(info.name, skill["skillName"].as_str().unwrap());
        }
        eprintln!("available to model: {}", skills::available(None).len());
        // 해시가 다르면 설치하지 않는다
        assert!(install_skill(&url, &"0".repeat(64), true).await.unwrap_err().contains("sha256"));

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
