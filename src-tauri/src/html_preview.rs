use crate::fs_ops;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{HOST, REFERER};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const MAX_PREVIEWS: usize = 24;
const MAX_ASSET_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Clone)]
struct PreviewEntry {
    root: Option<PathBuf>,
    entry_path: String,
    html: Bytes,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewHandle {
    pub token: String,
    pub url: String,
}

struct PreviewResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    location: Option<String>,
}

pub struct PreviewServer {
    origin: String,
    entries: Arc<PreviewRegistry>,
}

impl PreviewServer {
    pub fn start() -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let authority = format!("anylm-preview.localhost:{}", listener.local_addr()?.port());
        let origin = format!("http://{authority}");
        listener.set_nonblocking(true)?;
        let entries = Arc::new(PreviewRegistry::default());
        let registry = entries.clone();
        let server_origin = origin.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(listener) = tokio::net::TcpListener::from_std(listener) else { return };
            while let Ok((stream, _)) = listener.accept().await {
                let entries = registry.clone();
                let origin = server_origin.clone();
                let authority = authority.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |request: Request<Incoming>| {
                        let entries = entries.clone();
                        let origin = origin.clone();
                        let authority = authority.clone();
                        async move {
                            let reply = if request.headers().get(HOST).and_then(|v| v.to_str().ok()) != Some(&authority) {
                                response(403, "text/plain", b"Forbidden".to_vec())
                            } else if request.method() != Method::GET && request.method() != Method::HEAD {
                                response(405, "text/plain", b"Method not allowed".to_vec())
                            } else {
                                let path = request.uri().path_and_query().map_or("/", |p| p.as_str()).to_owned();
                                let referer = request.headers().get(REFERER).and_then(|v| v.to_str().ok()).map(str::to_owned);
                                let response_origin = origin.clone();
                                tokio::task::spawn_blocking(move || entries.serve(&response_origin, &path, referer.as_deref()))
                                    .await.unwrap_or_else(|_| response(500, "text/plain", Vec::new()))
                            };
                            let csp = preview_policy(&origin);
                            let mut builder = Response::builder().status(reply.status)
                                .header("Content-Type", reply.content_type)
                                .header("Content-Security-Policy", csp)
                                .header("X-Content-Type-Options", "nosniff")
                                .header("Referrer-Policy", "same-origin")
                                .header("Cache-Control", "no-store");
                            if let Some(location) = reply.location {
                                builder = builder.header("Location", location);
                            }
                            Ok::<_, Infallible>(builder.body(Full::new(Bytes::from(reply.body))).expect("valid preview response"))
                        }
                    });
                    let connection = http1::Builder::new().keep_alive(false).serve_connection(TokioIo::new(stream), service);
                    let _ = tokio::time::timeout(Duration::from_secs(20), connection).await;
                });
            }
        });
        Ok(Self { origin, entries })
    }

    pub fn create(&self, root: Option<PathBuf>, entry_path: String, html: Vec<u8>) -> Result<PreviewHandle, String> {
        if html.len() as u64 > MAX_ASSET_BYTES {
            return Err("HTML 미리보기 파일이 너무 큽니다.".into());
        }
        if contains_private_component(&entry_path) {
            return Err("이 HTML 파일의 경로는 미리보기에 사용할 수 없습니다.".into());
        }
        self.entries.create(&self.origin, root, entry_path, html)
    }

    pub fn close(&self, token: &str) {
        self.entries.entries.lock().unwrap_or_else(|e| e.into_inner()).remove(token);
    }
}

#[derive(Default)]
struct PreviewRegistry {
    entries: Mutex<HashMap<String, PreviewEntry>>,
}

impl PreviewRegistry {
    fn create(&self, origin: &str, root: Option<PathBuf>, entry_path: String, html: Vec<u8>) -> Result<PreviewHandle, String> {
        let token = Uuid::new_v4().simple().to_string();
        let entry_path = normalize_slashes(&entry_path);
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= MAX_PREVIEWS {
            return Err("열려 있는 HTML 미리보기가 너무 많습니다.".into());
        }
        entries.insert(token.clone(), PreviewEntry { root, entry_path: entry_path.clone(), html: html.into() });
        let path = encode_path(&format!("{token}/{entry_path}"));
        Ok(PreviewHandle { url: format!("{origin}/{path}"), token })
    }

    fn serve(&self, origin: &str, path_and_query: &str, referrer: Option<&str>) -> PreviewResponse {
        let request_path = path_and_query.split('?').next().unwrap_or("/");
        let Ok(decoded) = percent_decode(request_path) else { return response(400, "text/plain; charset=utf-8", b"Bad request".to_vec()) };
        let relative = decoded.trim_start_matches('/');
        let direct = relative.split_once('/');
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let scoped = direct.and_then(|(token, path)| entries.get(token).map(|entry| (entry.clone(), path)));
        let Some((entry, requested)) = scoped else {
            // Redirect root-relative requests so imports in the resulting CSS/JS retain the token.
            let token = referrer
                .and_then(|url| url.strip_prefix(origin))
                .and_then(|url| url.strip_prefix('/'))
                .and_then(|path| path.split_once('/'))
                .map(|(token, _)| token)
                .filter(|token| entries.contains_key(*token));
            return match token {
                Some(token) if !relative.is_empty() => PreviewResponse {
                    location: Some(format!("/{token}{path_and_query}")),
                    ..response(307, "text/plain", Vec::new())
                },
                _ => response(404, "text/plain", Vec::new()),
            };
        };
        drop(entries);
        if requested.is_empty() || contains_private_component(requested) {
            return response(if requested.is_empty() { 404 } else { 403 }, "text/plain; charset=utf-8", b"Forbidden".to_vec());
        }
        if normalize_slashes(requested) == entry.entry_path {
            return response(200, "text/html; charset=utf-8", entry.html.to_vec());
        }
        let Some(root) = entry.root else { return response(404, "text/plain", Vec::new()) };
        let target = match fs_ops::resolve_safe_path(&root, requested) {
            Ok(path) => path,
            Err(_) => return response(403, "text/plain; charset=utf-8", b"Forbidden".to_vec()),
        };
        let Ok(canonical) = fs::canonicalize(&target) else { return response(404, "text/plain", Vec::new()) };
        let Ok(canonical_root) = fs::canonicalize(&root) else { return response(404, "text/plain", Vec::new()) };
        if !canonical.strip_prefix(canonical_root).is_ok_and(|path| !contains_private_component(&path.to_string_lossy())) {
            return response(403, "text/plain", Vec::new());
        }
        let Ok(meta) = fs::metadata(&canonical) else { return response(404, "text/plain; charset=utf-8", b"Not found".to_vec()) };
        if !meta.is_file() || meta.len() > MAX_ASSET_BYTES {
            return response(404, "text/plain; charset=utf-8", b"Not found".to_vec());
        }
        let mut body = Vec::new();
        match fs::File::open(canonical).and_then(|file| file.take(MAX_ASSET_BYTES + 1).read_to_end(&mut body)) {
            Ok(_) if body.len() as u64 <= MAX_ASSET_BYTES => response(200, content_type_for(requested), body),
            _ => response(404, "text/plain; charset=utf-8", b"Not found".to_vec()),
        }
    }
}

fn response(status: u16, content_type: &'static str, body: Vec<u8>) -> PreviewResponse {
    PreviewResponse { status, content_type, body, location: None }
}

fn normalize_slashes(path: &str) -> String {
    path.trim_start_matches(['/', '\\']).replace('\\', "/")
}

fn contains_private_component(path: &str) -> bool {
    path.split(['/', '\\']).any(|part| part == ".." || part.starts_with('.'))
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(path: &str) -> Result<String, ()> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = *bytes.get(i + 1).ok_or(())?;
            let lo = *bytes.get(i + 2).ok_or(())?;
            out.push(hex_value(hi).ok_or(())? * 16 + hex_value(lo).ok_or(())?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "wasm" => "application/wasm",
        "xml" => "application/xml; charset=utf-8",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn preview_policy(origin: &str) -> String {
    let dev = if cfg!(debug_assertions) { " http://localhost:1420" } else { "" };
    format!("sandbox allow-scripts allow-same-origin; default-src 'none'; script-src {origin} 'unsafe-inline' 'wasm-unsafe-eval'; style-src {origin} 'unsafe-inline'; img-src {origin} data: blob:; font-src {origin} data:; media-src {origin} blob:; connect-src {origin}; frame-src 'none'; base-uri {origin}; form-action 'none'; frame-ancestors tauri://localhost http://tauri.localhost{dev}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    const ORIGIN: &str = "http://anylm-preview.localhost:12345";

    #[test]
    fn serves_unsaved_entry_and_relative_project_assets() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pages/assets")).unwrap();
        fs::write(dir.path().join("pages/assets/site.css"), "body{color:#123}").unwrap();
        let registry = PreviewRegistry::default();
        let preview = registry.create(
            ORIGIN,
            Some(dir.path().to_path_buf()),
            "pages/index.html".into(),
            b"<link rel=\"stylesheet\" href=\"assets/site.css\"><h1>edited</h1>".to_vec(),
        ).unwrap();

        let html = registry.serve(ORIGIN, &format!("/{}/pages/index.html", preview.token), None);
        assert_eq!(html.status, 200);
        assert_eq!(html.content_type, "text/html; charset=utf-8");
        assert!(String::from_utf8(html.body).unwrap().contains("edited"));

        let css = registry.serve(ORIGIN, &format!("/{}/pages/assets/site.css", preview.token), None);
        assert_eq!(css.status, 200);
        assert_eq!(css.content_type, "text/css; charset=utf-8");
        assert_eq!(css.body, b"body{color:#123}");

        // `/assets/...` 같은 프로젝트 루트 기준 URL도 브라우저의 Referer에서 토큰을 복구해 제공한다.
        let absolute = registry.serve(
            ORIGIN,
            "/pages/assets/site.css?v=1",
            Some(&preview.url),
        );
        assert_eq!(absolute.status, 307);
        assert_eq!(absolute.location.as_deref(), Some(format!("/{}/pages/assets/site.css?v=1", preview.token).as_str()));
    }

    #[test]
    fn rejects_unknown_tokens_traversal_directories_and_hidden_secrets() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        fs::write(dir.path().join(".git/config"), "secret").unwrap();
        let registry = PreviewRegistry::default();
        let preview = registry.create(ORIGIN, Some(dir.path().to_path_buf()), "index.html".into(), b"ok".to_vec()).unwrap();
        let status = |path: &str| registry.serve(ORIGIN, path, None).status;

        assert_eq!(status("/not-a-token/index.html"), 404);
        assert_eq!(status(&format!("/{}/../secret.txt", preview.token)), 403);
        assert_eq!(status(&format!("/{}/%2e%2e/secret.txt", preview.token)), 403);
        assert_eq!(status(&format!("/{}/.git/config", preview.token)), 403);
        assert_eq!(status(&format!("/{}/", preview.token)), 404);
    }

    #[test]
    fn responses_isolate_scripts_from_the_app_and_external_network() {
        let csp = preview_policy(ORIGIN);
        assert!(csp.contains("sandbox allow-scripts allow-same-origin"));
        assert!(csp.contains(&format!("script-src {ORIGIN} 'unsafe-inline'")));
        assert!(csp.contains(&format!("connect-src {ORIGIN};")));
        assert!(csp.contains("form-action 'none'"));
        assert!(csp.contains("frame-ancestors tauri://localhost http://tauri.localhost"));
        assert!(!csp.contains("ipc:"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires Microsoft Edge and an available localhost:1420"]
    fn edge_renders_linked_css_and_module_scripts_in_sandbox() {
        let dir = tempdir().unwrap();
        let fixture = dir.path();
        fs::create_dir(fixture.join("pages")).unwrap();
        fs::create_dir(fixture.join("assets")).unwrap();
        fs::write(fixture.join("assets/main.css"), "@import 'theme.css';").unwrap();
        fs::write(fixture.join("assets/theme.css"), "body { color: rgb(12, 34, 56); }").unwrap();
        fs::write(fixture.join("pages/payload.json"), r#"{"result":"preview-ok"}"#).unwrap();
        fs::write(fixture.join("pages/result.js"), "export const path = './payload.json';").unwrap();
        fs::write(fixture.join("pages/main.js"), r#"
            import { path } from './result.js';
            window.addEventListener('load', async () => {
                try {
                    const {result} = await (await fetch(path)).json();
                    const style = getComputedStyle(document.body).color;
                    let isolated = false;
                    try { void parent.document; } catch { isolated = true; }
                    parent.postMessage([result, style, isolated].join(' '), '*');
                } catch (err) {
                    parent.postMessage(String(err), '*');
                }
            });
        "#).unwrap();
        let server = PreviewServer::start().unwrap();
        let preview = server.create(Some(fixture.to_path_buf()), "pages/index.html".into(), br#"
            <!doctype html><link rel="stylesheet" href="/assets/main.css">
            <script type="module" src="main.js"></script><body>HTML preview</body>
        "#.to_vec()).unwrap();
        let parent = std::net::TcpListener::bind("127.0.0.1:1420").unwrap();
        std::thread::spawn(move || {
            use std::io::Write;
            for mut stream in parent.incoming().flatten() {
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let html = format!(r#"<!doctype html>
                    <script>addEventListener('message', e => document.querySelector('p').textContent = e.data);</script>
                    <p>preview-pending</p><iframe sandbox="allow-scripts allow-same-origin" src="{}"></iframe>"#, preview.url);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", html.len(), html);
            }
        });
        let edge = std::path::Path::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe");
        let output = std::process::Command::new(edge)
            .args(["--headless", "--disable-gpu", "--no-first-run", "--disable-background-networking", "--dump-dom", "--virtual-time-budget=10000", "--enable-logging=stderr"])
            .arg(format!("--user-data-dir={}", fixture.join("edge-profile").display()))
            .arg("http://localhost:1420/")
            .output().unwrap();
        let dom = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success() && dom.contains("preview-ok rgb(12, 34, 56) true"),
            "DOM: {dom}\n{}", String::from_utf8_lossy(&output.stderr));
    }
}
