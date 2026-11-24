#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_node_scripts_one_folder_deep_with_its_package_manager() {
        let dir = tempfile::tempdir().unwrap();
        let web = dir.path().join("web");
        std::fs::create_dir(&web).unwrap();
        std::fs::write(web.join("package.json"), r#"{"scripts":{"dev":"vite","lint":"eslint"}}"#).unwrap();
        std::fs::write(web.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(dir.path().join("index.html"), "<h1>root</h1>").unwrap();
        let entries = detect(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cwd, "web");
        assert_eq!(entries[0].command, "pnpm run dev");
    }

    #[test]
    fn detects_loopback_in_colored_logs_and_keeps_path() {
        assert_eq!(address_in_output("\x1b[32m✔\x1b[0m Local: http://0.0.0.0:5173/view\n").as_deref(),
            Some("http://localhost:5173/view"));
        assert_eq!(address_in_output("http://localhost.example.org:5173/\nhttp://[::1]:3000/").as_deref(),
            Some("http://[::1]:3000/"));
        for url in ["javascript:alert(1)", "http://remote.test:7", "http://user@localhost:9"] {
            assert!(local_url(url).is_err(), "{url}");
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn project_server_opens_once_reaches_ready_and_releases_port() {
        let dir = tempfile::tempdir().unwrap();
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let app = tauri::test::mock_app();
        app.manage(TerminalManager::new(terminal::tauri_sink(app.handle().clone())));
        app.manage(DevServers::default());
        let project = Project {
            id: "p".into(), name: "미리보기 검증".into(), root_path: dir.path().display().to_string(),
            instructions: String::new(), created_at: String::new(), auto_approve: Vec::new(),
            knowledge_folders: Vec::new(), memory: Vec::new(), dev_servers: Vec::new(),
        };
        let config = DevServerConfig {
            id: "probe".into(), name: "테스트 서버".into(), cwd: ".".into(), url: None,
            command: format!(
                "$listener = [System.Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, {port}); +                 $listener.Start(); Write-Output 'http://127.0.0.1:{port}/'; while ($true) {{ Start-Sleep -Seconds 1 }}"
            ),
        };
        assert!(validate(dir.path(), &DevServerConfig { cwd: "../".into(), ..config.clone() }).is_err());
        let first = start(app.handle(), &project, "powershell", &config).unwrap();
        let second = start(app.handle(), &project, "powershell", &config).unwrap();
        assert_eq!(first.terminal.id, second.terminal.id);
        let expected = format!("http://127.0.0.1:{port}/");
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let server = list(app.handle(), "p").unwrap().remove(0);
                if server.status != "starting" {
                    break server;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }).await;
        assert!(list(app.handle(), "other").unwrap().is_empty());
        app.state::<TerminalManager>().close(&first.terminal.id).unwrap();
        let ready = result.expect("테스트 서버 시작 시간 초과");
        assert_eq!(ready.status, "ready");
        assert_eq!(ready.url.as_deref(), Some(expected.as_str()));
        assert!(!is_listening(&local_url(&expected).unwrap()).await);
    }
}
// 프로젝트 개발 서버 감지, ConPTY 세션과 연결, 로컬 주소 확인.
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::{Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::{AppState, fs_ops, runtime, store::Project};
use crate::terminal::{self, OpenRequest, TerminalInfo, TerminalKind, TerminalManager};

const START_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const SERVER_EVENT: &str = "anylm-dev-server";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerConfig {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerInfo {
    pub project_id: String,
    pub terminal: TerminalInfo,
    pub config: DevServerConfig,
    pub url: Option<String>,
    pub status: &'static str,
}

struct Session {
    info: ServerInfo,
    started: Instant,
}

#[derive(Default)]
pub(crate) struct DevServers(Mutex<HashMap<String, Session>>);

pub(crate) fn local_url(raw: &str) -> Result<Url, String> {
    let invalid = || "미리보기에는 localhost, 127.0.0.1, [::1], *.localhost의 HTTP(S) 주소만 쓸 수 있습니다.".to_string();
    let mut url = Url::parse(raw.trim()).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() {
        return Err(invalid());
    }
    match url.host_str() {
        Some("ipc.localhost" | "tauri.localhost") => return Err(invalid()),
        Some("localhost" | "127.0.0.1" | "[::1]") => {}
        Some("0.0.0.0") => url.set_host(Some("localhost")).map_err(|_| invalid())?,
        Some(host) if host.ends_with(".localhost") => {}
        _ => return Err(invalid()),
    }
    if url.port_or_known_default() == Some(0) {
        return Err(invalid());
    }
    Ok(url)
}

fn address_in_output(raw: &str) -> Option<String> {
    let plain = terminal::plain_text(raw);
    plain.split_whitespace().filter_map(|word| {
        let start = word.find("http://").or_else(|| word.find("https://"))?;
        let address = word[start..].trim_end_matches([')', ',', '"', '\'', '>', ';']);
        let url = local_url(address).ok()?;
        // 포트 없는 도움말 링크는 실행 주소로 간주하지 않는다.
        let authority = address.split_once("://")?.1.split(['/','?','#']).next()?;
        let (_, port) = authority.rsplit_once(':')?;
        port.parse::<u16>().ok()?;
        Some(url.to_string())
    }).next()
}

fn folder_for(root: &Path, cwd: &str) -> Result<PathBuf, String> {
    let dir = if cwd.trim().is_empty() || cwd == "." {
        root.canonicalize().map_err(|e| e.to_string())?
    } else {
        fs_ops::resolve_safe_path(root, cwd)?
    };
    if !dir.is_dir() {
        return Err("개발 서버 작업 폴더를 찾을 수 없습니다.".into());
    }
    Ok(dir.canonicalize().map_err(|e| e.to_string())?)
}

fn candidate(cwd: &str, name: &str, command: String, url: Option<String>) -> DevServerConfig {
    let label = if cwd.is_empty() { name.to_string() } else { format!("{cwd} · {name}") };
    DevServerConfig { id: format!("{cwd}/{name}"), name: label, command, cwd: cwd.into(), url }
}

fn read_small(path: &Path) -> Option<String> {
    let allowed = path.symlink_metadata().ok()?;
    if !allowed.is_file() || allowed.len() > 512 * 1024 {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn detect(root: &Path) -> Result<Vec<DevServerConfig>, String> {
    let mut dirs = std::fs::read_dir(root).map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (!name.starts_with('.') && !["node_modules", "dist", "target", "build", "vendor", "__pycache__"].contains(&name.as_str()))
                .then_some(name)
        })
        .collect::<Vec<_>>();
    dirs.sort();
    let mut out = Vec::new();
    for cwd in std::iter::once(String::new()).chain(dirs) {
        let Ok(dir) = folder_for(root, &cwd) else { continue };
        let has = |file: &str| dir.join(file).symlink_metadata().is_ok_and(|m| m.is_file());
        if let Some(manifest) = read_small(&dir.join("package.json")).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) {
            let manager = if has("pnpm-lock.yaml") { "pnpm" }
                else if has("yarn.lock") { "yarn" }
                else if has("bun.lock") || has("bun.lockb") { "bun" }
                else { "npm" };
            for script in ["dev", "start", "serve", "preview"] {
                if manifest["scripts"][script].as_str().is_some_and(|s| !s.trim().is_empty()) {
                    out.push(candidate(&cwd, script, format!("{manager} run {script}"), None));
                }
            }
        }
        if has("manage.py") {
            out.push(candidate(&cwd, "Django", "python manage.py runserver".into(), None));
        }
        let streamlit = ["requirements.txt", "pyproject.toml", "Pipfile"].iter()
            .filter_map(|p| read_small(&dir.join(p)))
            .any(|s| s.lines().any(|line| {
                line.split('#').next().unwrap_or("").split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                    .any(|word| word.eq_ignore_ascii_case("streamlit"))
            }));
        if streamlit {
            if let Some(file) = ["streamlit_app.py", "app.py"].into_iter().find(|f| has(f)) {
                out.push(candidate(&cwd, "Streamlit", format!("python -m streamlit run {file}"), None));
            }
        }
    }
    if out.is_empty() && root.join("index.html").symlink_metadata().is_ok_and(|m| m.is_file()) {
        out.push(candidate("", "정적 사이트", "python -u -m http.server 8000 --bind 127.0.0.1".into(),
            Some("http://127.0.0.1:8000/".into())));
    }
    Ok(out)
}

fn load_project(state: &AppState, id: &str) -> Result<(Project, String), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let project = db.projects.iter().find(|p| p.id == id).cloned()
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;
    Ok((project, db.settings.terminal_shell.clone()))
}

fn validate(root: &Path, config: &DevServerConfig) -> Result<(PathBuf, Option<String>), String> {
    if config.name.trim().is_empty() || config.command.trim().is_empty() || config.command.contains('\0') {
        return Err("서버 이름과 실행 명령을 입력해 주세요.".into());
    }
    let folder = folder_for(root, &config.cwd)?;
    let url = config.url.as_deref().filter(|s| !s.trim().is_empty())
        .map(|s| local_url(s).map(|u| u.to_string())).transpose()?;
    Ok((folder, url))
}

fn opened<R: Runtime>(
    app: &AppHandle<R>,
    project: &Project,
    shell: &str,
    config: &DevServerConfig,
) -> Result<(ServerInfo, bool), String> {
    let (cwd, url) = validate(Path::new(&project.root_path), config)?;
    let terminals = app.try_state::<TerminalManager>().ok_or("터미널을 사용할 수 없습니다.")?;
    let servers = app.try_state::<DevServers>().ok_or("개발 서버를 사용할 수 없습니다.")?;
    let mut sessions = servers.0.lock().map_err(|e| e.to_string())?;
    let live = terminals.list();
    sessions.retain(|id, _| live.iter().any(|t| &t.id == id));
    if let Some(existing) = sessions.values().find(|s| {
        s.info.project_id == project.id
            && Path::new(&s.info.terminal.cwd) == cwd
            && s.info.terminal.command.as_deref() == Some(config.command.trim())
            && live.iter().any(|t| t.id == s.info.terminal.id && t.running)
    }) {
        return Ok((existing.info.clone(), false));
    }
    let request = OpenRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        cols: 100, rows: 30, kind: TerminalKind::Server,
        title: Some(config.name.trim().to_string()), command: Some(config.command.trim().to_string()),
    };
    let command_shell = terminal::effective_shell(shell, std::env::var_os("PATH").as_deref());
    let env: Vec<_> = runtime::child_env().into_iter().collect();
    let terminal = terminals.open(&request, &cwd, command_shell, &runtime::path_dirs(), &env)?;
    let info = ServerInfo { project_id: project.id.clone(), terminal, config: config.clone(), url, status: "starting" };
    sessions.insert(info.terminal.id.clone(), Session { info: info.clone(), started: Instant::now() });
    Ok((info, true))
}

async fn is_listening(url: &Url) -> bool {
    let Some(port) = url.port_or_known_default() else { return false };
    let ipv4 = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let ipv6 = SocketAddr::from((Ipv6Addr::LOCALHOST, port));
    let connect = |addr| async move {
        tokio::time::timeout(Duration::from_millis(200), tokio::net::TcpStream::connect(addr))
            .await.is_ok_and(|r| r.is_ok())
    };
    match url.host_str() {
        Some("127.0.0.1") => connect(ipv4).await,
        Some("[::1]") => connect(ipv6).await,
        _ => {
            let (a, b) = tokio::join!(connect(ipv4), connect(ipv6));
            a || b
        }
    }
}

async fn watch<R: Runtime>(app: AppHandle<R>, id: String) {
    loop {
        let (Some(terminals), Some(servers)) = (app.try_state::<TerminalManager>(), app.try_state::<DevServers>()) else { return };
        let (mut info, deadline) = {
            let Ok(sessions) = servers.0.lock() else { return };
            let Some(session) = sessions.get(&id) else { return };
            (session.info.clone(), session.started + START_TIMEOUT)
        };
        let Some(terminal) = terminals.list().into_iter().find(|t| t.id == id) else { return };
        info.terminal = terminal;
        if !info.terminal.running {
            info.status = "exited";
        } else {
            if info.url.is_none() {
                info.url = terminals.text(&id).ok().and_then(|s| address_in_output(&s));
            }
            if let Some(url) = info.url.as_deref().and_then(|s| local_url(s).ok()) {
                if is_listening(&url).await {
                    info.status = "ready";
                }
            }
            if info.status == "starting" && Instant::now() >= deadline {
                info.status = "unconfirmed";
            }
        }
        let changed = {
            let Ok(mut sessions) = servers.0.lock() else { return };
            let Some(session) = sessions.get_mut(&id) else { return };
            let changed = session.info.status != info.status || session.info.url != info.url;
            session.info = info.clone();
            changed
        };
        if changed {
            let _ = app.emit(SERVER_EVENT, &info);
        }
        if info.status != "starting" {
            return;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

pub(crate) fn start<R: Runtime>(
    app: &AppHandle<R>,
    project: &Project,
    shell: &str,
    config: &DevServerConfig,
) -> Result<ServerInfo, String> {
    let (info, fresh) = opened(app, project, shell, config)?;
    if fresh {
        let _ = app.emit(SERVER_EVENT, &info);
        tauri::async_runtime::spawn(watch(app.clone(), info.terminal.id.clone()));
    }
    Ok(info)
}

pub(crate) fn list<R: Runtime>(app: &AppHandle<R>, project_id: &str) -> Result<Vec<ServerInfo>, String> {
    let terminals = app.try_state::<TerminalManager>().ok_or("터미널을 사용할 수 없습니다.")?;
    let manager = app.try_state::<DevServers>().ok_or("개발 서버를 사용할 수 없습니다.")?;
    let sessions = manager.0.lock().map_err(|e| e.to_string())?;
    let live = terminals.list();
    let mut out = sessions.values().filter(|s| s.info.project_id == project_id).filter_map(|s| {
        let mut info = s.info.clone();
        info.terminal = live.iter().find(|t| t.id == info.terminal.id)?.clone();
        if !info.terminal.running {
            info.status = "exited";
        }
        Some(info)
    }).collect::<Vec<_>>();
    out.sort_by(|a, b| a.terminal.id.cmp(&b.terminal.id));
    Ok(out)
}

#[tauri::command]
pub(crate) async fn dev_server_options(state: State<'_, AppState>, project_id: String) -> Result<Vec<DevServerConfig>, String> {
    let (project, _) = load_project(&state, &project_id)?;
    let mut options = project.dev_servers;
    for detected in detect(Path::new(&project.root_path))? {
        if !options.iter().any(|o| o.cwd == detected.cwd && o.command == detected.command) {
            options.push(detected);
        }
    }
    Ok(options)
}

#[tauri::command]
pub(crate) async fn dev_server_start<R: Runtime>(
    app: AppHandle<R>, state: State<'_, AppState>, project_id: String, mut config: DevServerConfig,
    save: bool,
) -> Result<ServerInfo, String> {
    let (project, shell) = load_project(&state, &project_id)?;
    if save {
        let (_, url) = validate(Path::new(&project.root_path), &config)?;
        config.url = url;
        if config.id.trim().is_empty() {
            config.id = uuid::Uuid::new_v4().to_string();
        }
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let live = db.projects.iter_mut().find(|p| p.id == project_id).ok_or("프로젝트를 찾을 수 없습니다.")?;
        live.dev_servers.retain(|s| s.id != config.id && (s.cwd != config.cwd || s.command != config.command));
        live.dev_servers.insert(0, config.clone());
        let updated = live.clone();
        state.store.save(&db)?;
        let _ = app.emit(crate::exaone::PROJECT_UPDATED_EVENT, updated);
    }
    start(&app, &project, &shell, &config)
}

#[tauri::command]
pub(crate) fn dev_server_list<R: Runtime>(app: AppHandle<R>, project_id: String) -> Result<Vec<ServerInfo>, String> {
    list(&app, &project_id)
}

/// iframe의 load 이벤트는 차단·실패 시에도 올 수 있어 로컬 응답 헤더를 따로 확인한다.
#[tauri::command]
pub(crate) async fn dev_preview_check(raw: String) -> Result<(), String> {
    let url = local_url(&raw)?;
    let host = url.host_str().unwrap_or("localhost");
    let port = url.port_or_known_default().ok_or("잘못된 포트입니다.")?;
    let loopbacks = [SocketAddr::from((Ipv4Addr::LOCALHOST, port)), SocketAddr::from((Ipv6Addr::LOCALHOST, port))];
    let client = reqwest::Client::builder().no_proxy().redirect(Policy::none())
        .resolve_to_addrs(host, &loopbacks).timeout(Duration::from_secs(4)).build().map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|_| "미리보기 주소에 연결할 수 없습니다.".to_string())?;
    let headers = response.headers();
    let blocked = headers.get("x-frame-options").and_then(|v| v.to_str().ok())
        .is_some_and(|v| ["deny", "sameorigin"].iter().any(|x| v.eq_ignore_ascii_case(x)));
    let csp_blocked = headers.get_all("content-security-policy").iter().filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';')).any(|d| {
            let d = d.trim();
            d == "frame-ancestors 'none'" || d == "frame-ancestors 'self'"
        });
    if blocked || csp_blocked || response.status().is_server_error() {
        return Err("이 페이지는 앱 안에서 열 수 없습니다.".into());
    }
    Ok(())
}
