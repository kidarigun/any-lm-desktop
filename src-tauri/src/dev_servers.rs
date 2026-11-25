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
        for url in ["javascript:alert(1)", "http://remote.test:7", "http://user@localhost:9",
            "http://tauri.localhost:5173/x", "http://ipc.localhost:1/", "http://asset.localhost:80/"] {
            assert!(local_url(url).is_err(), "{url}");
        }
    }

    #[tokio::test]
    async fn ipv6_only_server_is_seen_through_localhost() {
        // Vite처럼 ::1에만 열린 서버. 127.0.0.1 쪽 연결은 Windows에서 약 2초 뒤에야 거절된다.
        let Ok(listener) = std::net::TcpListener::bind("[::1]:0") else { return };
        let url = local_url(&format!("http://localhost:{}/", listener.local_addr().unwrap().port())).unwrap();
        let started = Instant::now();
        assert!(is_listening(&url).await);
        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
        drop(listener);
        let started = Instant::now();
        assert!(!is_listening(&url).await);
        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn server_commands_run_in_the_project_folder_not_windows() {
        let dir = tempfile::tempdir().unwrap();
        let app = tauri::test::mock_app();
        app.manage(TerminalManager::new(terminal::tauri_sink(app.handle().clone())));
        app.manage(DevServers::default());
        let project = Project {
            id: "p".into(), name: "작업 폴더 확인".into(), root_path: dir.path().display().to_string(),
            instructions: String::new(), created_at: String::new(), auto_approve: Vec::new(),
            knowledge_folders: Vec::new(), memory: Vec::new(), dev_servers: Vec::new(),
        };
        let expected = fs_ops::process_path(&dir.path().canonicalize().unwrap()).display().to_string();
        // cmd 설정(셸 자체가 cmd)과 PowerShell에서 부른 cmd(npm 스크립트가 도는 방식) 둘 다
        for (shell, command) in [("cmd", "cd"), ("powershell", "cmd /c cd")] {
            let config = DevServerConfig {
                id: shell.into(), name: format!("{shell} 위치"), cwd: ".".into(), url: None, command: command.into(),
            };
            let info = start(app.handle(), &project, shell, &config).unwrap();
            assert_eq!(info.terminal.cwd, expected);
            let terminals = app.state::<TerminalManager>();
            let done = tokio::time::timeout(Duration::from_secs(15), async {
                while terminals.list().iter().any(|t| t.id == info.terminal.id && t.running) {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await;
            let text = terminal::plain_text(&terminals.text(&info.terminal.id).unwrap());
            let _ = stop(app.handle(), "p", &info.terminal.id);
            done.unwrap_or_else(|_| panic!("{shell}: 명령이 끝나지 않았습니다: {text}"));
            assert!(text.to_lowercase().contains(&expected.to_lowercase()), "{shell}: {text}");
            assert!(!text.contains("UNC"), "{shell}: {text}");
            assert!(!text.to_lowercase().contains(r"c:\windows"), "{shell}: {text}");
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
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_url = format!("http://{}/", occupied.local_addr().unwrap());
        assert!(start(app.handle(), &project, "powershell",
            &DevServerConfig { url: Some(occupied_url), ..config.clone() }).is_err());
        assert!(list(app.handle(), "p").unwrap().is_empty());
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
        let args = serde_json::json!({"id":first.terminal.id,"lines":2});
        assert!(logs(app.handle(), "other", &args, 4_000).is_err());
        assert!(stop(app.handle(), "other", &first.terminal.id).is_err());
        let logged = logs(app.handle(), "p", &args, 4_000).unwrap();
        stop(app.handle(), "p", &first.terminal.id).unwrap();
        let ready = result.expect("테스트 서버 시작 시간 초과");
        assert_eq!(ready.status, "ready");
        assert_eq!(ready.url.as_deref(), Some(expected.as_str()));
        assert!(logged.contains(&expected));
        assert!(!is_listening(&local_url(&expected).unwrap()).await);

        let flag = AtomicBool::new(false);
        let args = serde_json::json!({"name":"취소 시험","command":"Start-Sleep -Seconds 30"});
        let start = start_for_model(app.handle(), &project, &args, &flag);
        let interrupt = async {
            while list(app.handle(), "p").unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            flag.store(true, Ordering::Relaxed);
        };
        let (stopped, ()) = tokio::time::timeout(Duration::from_secs(4), async {
            tokio::join!(start, interrupt)
        }).await.unwrap();
        assert!(stopped.unwrap_err().contains("중지"));
        assert!(list(app.handle(), "p").unwrap().is_empty());
    }

    #[test]
    fn long_server_logs_keep_the_newest_lines_within_the_tool_cap() {
        let logs: String = (0..2_000).map(|i| format!("[vite] \"line\" {i}: hmr update /src/App.tsx")).collect::<Vec<_>>().join("\n");
        let cap = 4_000;
        let out = output_json("t1", "dev", "ready", Some("http://127.0.0.1:5173/"), None, &logs, cap);
        let text = out.to_string();
        assert!(text.chars().count() <= cap, "{}", text.chars().count());
        let kept = out["logs"].as_str().unwrap();
        assert!(kept.starts_with("[앞부분 "), "{kept}");
        assert!(kept.lines().next().unwrap().ends_with("자 생략]"));
        assert!(kept.ends_with("\"line\" 1999: hmr update /src/App.tsx"), "가장 최근 줄이 남아야 한다");
        assert!(!kept.contains("\"line\" 0: "), "오래된 앞부분을 버린다");

        let short = output_json("t1", "dev", "ready", None, None, "ready in 300ms", cap);
        assert_eq!(short["logs"], "ready in 300ms");
    }
}
// 프로젝트 개발 서버 감지, ConPTY 세션과 연결, 로컬 주소 확인.
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::{Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::{AppState, fs_ops, runtime, store::Project};
use crate::terminal::{self, OpenRequest, TerminalInfo, TerminalKind, TerminalManager};

const START_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// 주소 하나에 연결해 보는 시간 (후보 주소마다 따로 센다).
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);
pub(crate) const SERVER_EVENT: &str = "anylm-dev-server";
const PREVIEW_EVENT: &str = "anylm-open-preview";

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
        Some(host) if crate::ipc_guard::is_reserved_host(host) => return Err(invalid()),
        Some("localhost" | "127.0.0.1" | "[::1]") => {}
        Some("0.0.0.0") => url.set_host(Some("localhost")).map_err(|_| invalid())?,
        Some(host) if host.ends_with(".localhost") => {}
        _ => return Err(invalid()),
    }
    if url.port_or_known_default() == Some(0) {
        return Err(invalid());
    }
    if crate::is_app_url(&url, &crate::ipc_guard::remembered_origins()) {
        return Err("Any LM Desktop 자체 주소는 미리보기에서 열 수 없습니다.".into());
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

pub(crate) fn start_args(root: &Path, args: &serde_json::Value) -> Result<DevServerConfig, String> {
    #[derive(Deserialize)]
    struct Args {
        command: String,
        name: Option<String>,
        cwd: Option<String>,
        url: Option<String>,
    }
    let parsed: Args = serde_json::from_value(args.clone())
        .map_err(|e| format!("서버 실행 인자가 올바르지 않습니다: {e}"))?;
    let config = DevServerConfig {
        id: String::new(),
        name: parsed.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "개발 서버".into()),
        command: parsed.command,
        cwd: parsed.cwd.unwrap_or_default(),
        url: parsed.url,
    };
    validate(root, &config)?;
    Ok(config)
}

fn opened<R: Runtime>(
    app: &AppHandle<R>,
    project: &Project,
    shell: &str,
    config: &DevServerConfig,
) -> Result<(ServerInfo, bool), String> {
    let (cwd, url) = validate(Path::new(&project.root_path), config)?;
    // 포함 검사는 정규 경로로 끝났다. 터미널과 세션 비교에는 `\\?\` 없는 경로를 쓴다.
    let cwd = fs_ops::process_path(&cwd);
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
    if let Some(address) = url.as_deref().and_then(|s| local_url(s).ok()) {
        if probe_addrs(&address).into_iter().any(|addr|
            std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
        ) {
            return Err(format!("이미 연결을 받고 있는 주소입니다: {address}"));
        }
    }
    let request = OpenRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        cols: 100, rows: 30, kind: TerminalKind::Server,
        title: Some(config.name.trim().to_string()), command: Some(config.command.trim().to_string()),
    };
    let command_shell = terminal::effective_shell(shell, std::env::var_os("PATH").as_deref());
    let env: Vec<_> = runtime::child_env().into_iter().collect();
    let mut dirs = runtime::path_dirs();
    if runtime::python_installed() {
        if let Some(bin) = runtime::pyenv_python().parent() {
            dirs.push(bin.to_path_buf());
        }
    }
    let terminal = terminals.open(&request, &cwd, command_shell, &dirs, &env)?;
    let info = ServerInfo { project_id: project.id.clone(), terminal, config: config.clone(), url, status: "starting" };
    sessions.insert(info.terminal.id.clone(), Session { info: info.clone(), started: Instant::now() });
    Ok((info, true))
}

fn probe_addrs(url: &Url) -> Vec<SocketAddr> {
    let Some(port) = url.port_or_known_default() else { return Vec::new() };
    let ipv4 = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let ipv6 = SocketAddr::from((Ipv6Addr::LOCALHOST, port));
    match url.host_str() {
        Some("127.0.0.1") => vec![ipv4],
        Some("[::1]") => vec![ipv6],
        _ => vec![ipv4, ipv6],
    }
}

/// 후보 주소에 동시에 연결해 본다. Windows는 거절된 127.0.0.1 연결이 약 2초 걸리고,
/// Node 17+의 `listen('localhost')`(Vite·Astro)는 ::1에만 열리므로 차례로 시도하면 ::1을 못 본다.
async fn is_listening(url: &Url) -> bool {
    let attempts: Vec<_> = probe_addrs(url).into_iter().map(|addr| Box::pin(async move {
        match tokio::time::timeout(PROBE_TIMEOUT, tokio::net::TcpStream::connect(addr)).await {
            Ok(Ok(_)) => Ok(()),
            _ => Err(()),
        }
    })).collect();
    // select_ok는 빈 목록이면 패닉하므로 먼저 거른다.
    !attempts.is_empty() && futures_util::future::select_ok(attempts).await.is_ok()
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
    start_tracked(app, project, shell, config).map(|(info, _)| info)
}

fn start_tracked<R: Runtime>(
    app: &AppHandle<R>, project: &Project, shell: &str, config: &DevServerConfig,
) -> Result<(ServerInfo, bool), String> {
    let (info, fresh) = opened(app, project, shell, config)?;
    if fresh {
        let _ = app.emit(SERVER_EVENT, &info);
        tauri::async_runtime::spawn(watch(app.clone(), info.terminal.id.clone()));
    }
    Ok((info, fresh))
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

fn find_server<R: Runtime>(app: &AppHandle<R>, project_id: &str, id: &str) -> Result<ServerInfo, String> {
    list(app, project_id)?.into_iter().find(|s| s.terminal.id == id)
        .ok_or_else(|| "이 프로젝트에서 해당 개발 서버 세션을 찾을 수 없습니다.".into())
}

/// 서버 출력은 최근 줄이 중요하므로, 길면 끝을 남기고 앞을 잘라 낸 뒤 첫 줄에 생략한 글자 수를 적는다.
fn keep_tail(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let tail: String = text.chars().skip(total - max_chars).collect();
    format!("[앞부분 {}자 생략]\n{}", total - max_chars, tail)
}

/// 도구 결과(JSON 문자열 전체)가 `cap`자를 넘지 않게 로그 끝부분만 담는다.
/// 모델에 돌려줄 때의 공통 자르기(`truncate_for_model`)는 앞을 남기므로, 그보다 먼저 여기서 끝을 남겨 둔다.
fn output_json(
    id: &str, name: &str, status: &str, url: Option<&str>, exit_code: Option<u32>, logs: &str, cap: usize,
) -> serde_json::Value {
    let make = |logs: String| serde_json::json!({
        "id": id, "name": name, "status": status, "url": url, "exitCode": exit_code, "logs": logs
    });
    let mut budget = cap.saturating_sub(300);
    loop {
        let value = make(keep_tail(logs, budget));
        let len = value.to_string().chars().count();
        if len <= cap || budget == 0 {
            return value;
        }
        // JSON으로 바꾸면 줄바꿈·따옴표가 늘어나므로 넘친 만큼 더 줄여 다시 만든다.
        budget = budget.saturating_sub(len - cap);
    }
}

fn server_output<R: Runtime>(app: &AppHandle<R>, server: &ServerInfo, lines: usize, cap: usize) -> serde_json::Value {
    let text = app.try_state::<TerminalManager>()
        .and_then(|terminals| terminals.text(&server.terminal.id).ok()).unwrap_or_default();
    let mut lines: Vec<_> = text.lines().rev().take(lines).collect();
    lines.reverse();
    output_json(
        &server.terminal.id, &server.terminal.title, server.status, server.url.as_deref(),
        server.terminal.exit_code, &lines.join("\n"), cap,
    )
}

pub(crate) async fn start_for_model<R: Runtime>(
    app: &AppHandle<R>, project: &Project, args: &serde_json::Value, cancel: &AtomicBool,
) -> Result<String, String> {
    let cancelled = "사용자가 개발 서버 시작을 중지했습니다.";
    if cancel.load(Ordering::Relaxed) {
        return Err(cancelled.into());
    }
    let config = start_args(Path::new(&project.root_path), args)?;
    let shell = app.try_state::<AppState>().and_then(|state|
        state.db.lock().ok().map(|db| db.settings.terminal_shell.clone())
    ).unwrap_or_else(|| "powershell".into());
    let (mut info, created) = start_tracked(app, project, &shell, &config)?;
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if cancel.load(Ordering::Relaxed) {
            if created && info.terminal.running {
                stop(app, &project.id, &info.terminal.id)?;
            }
            return Err(cancelled.into());
        }
        if info.status != "starting" {
            break;
        }
        if Instant::now() >= deadline {
            info.status = "unconfirmed";
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        info = find_server(app, &project.id, &info.terminal.id)?;
    }
    let output = server_output(app, &info, 40, crate::agent::current_tool_cap()).to_string();
    if info.status == "exited" {
        return Err(output);
    }
    Ok(output)
}

pub(crate) fn logs<R: Runtime>(
    app: &AppHandle<R>, project_id: &str, args: &serde_json::Value, cap: usize,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Args { id: Option<String>, lines: Option<u32> }
    let args: Args = serde_json::from_value(args.clone())
        .map_err(|e| format!("서버 로그 인자가 올바르지 않습니다: {e}"))?;
    if let Some(id) = args.id {
        let info = find_server(app, project_id, &id)?;
        return Ok(server_output(app, &info, args.lines.unwrap_or(80).clamp(1, 400) as usize, cap).to_string());
    }
    let servers = list(app, project_id)?;
    let running: Vec<_> = servers.iter().filter(|s| s.terminal.running).collect();
    // 배열 괄호와 항목 사이 쉼표(n + 1자)를 빼고 나눈다: 합친 출력이 도구 출력 상한을 넘지 않게.
    let n = running.len().max(1);
    let each = cap.saturating_sub(n + 1) / n;
    let output: Vec<_> = running.into_iter().map(|s| server_output(app, s, 10, each)).collect();
    Ok(serde_json::json!(output).to_string())
}

pub(crate) fn stop<R: Runtime>(app: &AppHandle<R>, project_id: &str, id: &str) -> Result<String, String> {
    let mut server = find_server(app, project_id, id)?;
    let terminals = app.try_state::<TerminalManager>().ok_or("터미널을 사용할 수 없습니다.")?;
    if let Some(manager) = app.try_state::<DevServers>() {
        manager.0.lock().map_err(|e| e.to_string())?.remove(id);
    }
    terminals.close(id)?;
    server.status = "exited";
    server.terminal.running = false;
    let _ = app.emit(terminal::EXIT_EVENT, serde_json::json!({"id": id, "code": null}));
    let _ = app.emit(SERVER_EVENT, &server);
    Ok(format!("개발 서버 '{}'을(를) 종료했습니다.", server.terminal.title))
}

pub(crate) fn preview<R: Runtime>(app: &AppHandle<R>, project_id: &str, raw: &str) -> Result<String, String> {
    let url = local_url(raw)?;
    app.emit(PREVIEW_EVENT, serde_json::json!({"projectId": project_id, "url": url.as_str()}))
        .map_err(|e| e.to_string())?;
    Ok(format!("미리보기 탭을 열었습니다: {url}"))
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
        let _ = app.emit(crate::agent::PROJECT_UPDATED_EVENT, updated);
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
