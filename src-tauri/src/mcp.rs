//! MCP(Model Context Protocol) 클라이언트.
//!
//! 사용자가 추가한 MCP 서버에 연결해, 그 서버가 알려 주는 도구를 모델의 도구 목록에 더한다.
//! - 로컬 서버(stdio): 프로그램을 직접 실행하고 표준 입출력으로 JSON-RPC 메시지를 주고받는다.
//! - 원격 서버(Streamable HTTP): URL로 POST하고, 응답은 JSON 또는 SSE 스트림으로 받는다.
//!
//! 필요한 기능(연결·도구 목록·도구 실행)만 구현한다. 환경 변수·헤더 값(API 키 등)은
//! 설정 파일이 아니라 Windows 자격 증명 관리자에 둔다.

use crate::exaone::SseDecoder;
use crate::mcp_oauth::{self, OAuthSettings, TokenSet};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// 처음 연결: npx/uvx가 패키지를 내려받는 시간까지 기다린다 (Python 서버는 의존 패키지가 많아 오래 걸릴 수 있다).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(300);
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(180);
const STDERR_TAIL_CHARS: usize = 4000;
const MAX_TOOLS_PER_SERVER: usize = 128;
const MAX_DESCRIPTION_CHARS: usize = 1024;
/// OpenAI 호환 API의 도구 이름 길이 제한
const MAX_TOOL_NAME: usize = 64;
pub const TOOL_PREFIX: &str = "mcp__";
/// 로그인이 필요할 때 오류 글 앞에 붙는 표시
pub const AUTH_REQUIRED: &str = "[로그인 필요]";

// ---------------------------------------------------------------------------
// 설정
// ---------------------------------------------------------------------------

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// "stdio"(로컬 프로그램) 또는 "http"(원격 URL)
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// 실행 위치 (비우면 사용자 폴더)
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub url: String,
    /// 환경 변수 이름 (값은 자격 증명 관리자에)
    #[serde(default)]
    pub env_keys: Vec<String>,
    /// HTTP 헤더 이름 (값은 자격 증명 관리자에)
    #[serde(default)]
    pub header_keys: Vec<String>,
    /// 모델에게 주지 않을 도구
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    /// 승인 없이 실행할 도구
    #[serde(default)]
    pub auto_approve: Vec<String>,
    /// 찾아보기 목록에서 설치했으면 그 항목 ID
    #[serde(default)]
    pub catalog_id: String,
    /// 원격 서버의 OAuth 로그인 (없으면 로그인하지 않는다)
    #[serde(default)]
    pub oauth: Option<OAuthSettings>,
}

/// 비밀 값 저장소 (실제로는 Windows 자격 증명 관리자, 테스트에서는 메모리)
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Option<String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str);
}

pub struct KeyringSecrets;

impl SecretStore for KeyringSecrets {
    fn get(&self, account: &str) -> Option<String> {
        crate::secrets::load_secret(account).ok().flatten()
    }
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        crate::secrets::store_secret(account, value)
    }
    fn delete(&self, account: &str) {
        let _ = crate::secrets::delete_secret(account);
    }
}

#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, account: &str) -> Option<String> {
        self.0.lock().ok()?.get(account).cloned()
    }
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.insert(account.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, account: &str) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(account);
        }
    }
}

fn env_account(id: &str, key: &str) -> String {
    format!("mcp:{}:env:{}", id, key)
}

fn header_account(id: &str, key: &str) -> String {
    format!("mcp:{}:header:{}", id, key)
}

fn valid_env_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 128
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !k.starts_with(|c: char| c.is_ascii_digit())
}

fn valid_header_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 128 && k.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

pub fn validate_url(url: &str) -> Result<(), String> {
    let u = url.trim();
    let lower = u.to_ascii_lowercase();
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        .iter()
        .any(|p| lower.strip_prefix(p).is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/'])));
    if !(lower.starts_with("https://") || local) {
        return Err("원격 MCP 서버 주소는 https://로 시작해야 합니다. (내 PC의 localhost만 http 허용)".to_string());
    }
    if u.contains(char::is_whitespace) {
        return Err("주소에 공백이 있습니다.".to_string());
    }
    Ok(())
}

fn validate_config(c: &McpServerConfig) -> Result<(), String> {
    if c.name.trim().is_empty() {
        return Err("서버 이름을 입력하세요.".to_string());
    }
    if c.oauth.is_some() && c.transport != "http" {
        return Err("로그인(OAuth)은 원격 서버(URL)에서만 쓸 수 있습니다.".to_string());
    }
    match c.transport.as_str() {
        "stdio" => {
            if c.command.trim().is_empty() {
                return Err("실행할 명령을 입력하세요. (예: npx)".to_string());
            }
        }
        "http" => validate_url(&c.url)?,
        other => return Err(format!("알 수 없는 연결 방식입니다: {}", other)),
    }
    if let Some(k) = c.env_keys.iter().find(|k| !valid_env_key(k)) {
        return Err(format!("환경 변수 이름이 올바르지 않습니다: {}", k));
    }
    if let Some(k) = c.header_keys.iter().find(|k| !valid_header_key(k)) {
        return Err(format!("헤더 이름이 올바르지 않습니다: {}", k));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 도구 정보
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub input_schema: Value,
}

/// 모델에게 줄 MCP 도구 하나
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    /// 모델에 보이는 이름 (mcp__서버__도구)
    pub exposed: String,
    pub server_id: String,
    pub server_name: String,
    pub tool: String,
    pub description: String,
    pub schema: Value,
    pub auto_approve: bool,
}

impl ToolSpec {
    pub fn schema_json(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.exposed,
                "description": self.description,
                "parameters": self.schema,
            }
        })
    }
}

fn slug(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' };
        if c == '_' && out.ends_with('_') {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_matches('_').chars().take(max).collect::<String>();
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

/// 서버 이름과 도구 이름으로 모델에 보일 도구 이름을 만든다 (64자 이내, 겹치지 않게).
fn exposed_name(server: &str, tool: &str, used: &mut HashSet<String>) -> String {
    let server_slug = slug(server, 20);
    let room = MAX_TOOL_NAME - TOOL_PREFIX.len() - server_slug.len() - 2;
    let base = format!("{}{}__{}", TOOL_PREFIX, server_slug, slug(tool, room));
    let mut name = base.clone();
    let mut n = 2;
    while !used.insert(name.clone()) {
        let suffix = format!("_{}", n);
        name = format!("{}{}", &base[..base.len().min(MAX_TOOL_NAME - suffix.len())], suffix);
        n += 1;
    }
    name
}

/// 도구 입력 스키마를 API가 받는 모양(type: object)으로 맞춘다.
fn normalize_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(m) if m.get("type").and_then(|t| t.as_str()) == Some("object") => {
            let mut m = m.clone();
            m.remove("$schema");
            if !m.contains_key("properties") {
                m.insert("properties".to_string(), json!({}));
            }
            Value::Object(m)
        }
        _ => json!({ "type": "object", "properties": {} }),
    }
}

fn parse_tools(result: &Value) -> Vec<RemoteTool> {
    result
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let name = t.get("name")?.as_str()?.to_string();
                    let description = t
                        .get("description")
                        .and_then(|d| d.as_str())
                        .or_else(|| t.get("title").and_then(|d| d.as_str()))
                        .unwrap_or("")
                        .to_string();
                    Some(RemoteTool {
                        name,
                        description,
                        input_schema: normalize_schema(t.get("inputSchema").unwrap_or(&Value::Null)),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// tools/call 결과를 모델에 줄 글로 바꾼다. (글, 오류 여부)
pub fn result_text(result: &Value) -> (String, bool) {
    let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut parts: Vec<String> = Vec::new();
    for c in result.get("content").and_then(|c| c.as_array()).into_iter().flatten() {
        let mime = c.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
        match c.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "text" => parts.push(c.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string()),
            "image" => parts.push(format!("[이미지 {} — 모델은 이미지 내용을 볼 수 없습니다]", mime)),
            "audio" => parts.push(format!("[오디오 {}]", mime)),
            "resource" => {
                let r = c.get("resource").unwrap_or(&Value::Null);
                match r.get("text").and_then(|t| t.as_str()) {
                    Some(t) => parts.push(t.to_string()),
                    None => parts.push(format!("[리소스 {}]", r.get("uri").and_then(|u| u.as_str()).unwrap_or(""))),
                }
            }
            "resource_link" => parts.push(format!(
                "[링크] {} {}",
                c.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                c.get("uri").and_then(|u| u.as_str()).unwrap_or("")
            )),
            _ => parts.push(c.to_string()),
        }
    }
    if parts.is_empty() {
        if let Some(s) = result.get("structuredContent") {
            parts.push(serde_json::to_string_pretty(s).unwrap_or_default());
        }
    }
    let text = parts.join("\n\n");
    (if text.trim().is_empty() { "(결과 없음)".to_string() } else { text }, is_error)
}

fn rpc_result(v: &Value) -> Result<Value, String> {
    if let Some(e) = v.get("error") {
        let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("알 수 없는 오류");
        return Err(format!("MCP 서버 오류: {}", msg));
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

// ---------------------------------------------------------------------------
// 프로그램 실행 (Windows의 npx.cmd 같은 배치 파일 포함)
// ---------------------------------------------------------------------------

fn install_hint(cmd: &str) -> &'static str {
    let c = cmd.to_ascii_lowercase();
    if ["npx", "node", "npm", "pnpm", "bunx"].iter().any(|n| c == *n || c.starts_with(&format!("{}.", n))) {
        " Node.js(https://nodejs.org)를 설치한 뒤 앱을 다시 시작하세요."
    } else if c == "uvx" || c == "uv" || c.starts_with("uvx.") || c.starts_with("uv.") {
        " uv(https://docs.astral.sh/uv/)를 설치한 뒤 앱을 다시 시작하세요."
    } else if c.starts_with("python") || c == "py" {
        " Python을 설치한 뒤 앱을 다시 시작하세요."
    } else if c == "docker" {
        " Docker Desktop을 설치하고 실행하세요."
    } else {
        ""
    }
}

#[cfg(windows)]
fn resolve_program(cmd: &str) -> Option<PathBuf> {
    let p = Path::new(cmd);
    let exts: Vec<String> = if p.extension().is_some() {
        vec![String::new()]
    } else {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    };
    let try_base = |base: &Path| -> Option<PathBuf> {
        exts.iter().find_map(|e| {
            let cand = PathBuf::from(format!("{}{}", base.to_string_lossy(), e));
            cand.is_file().then_some(cand)
        })
    };
    if p.is_absolute() || cmd.contains(['/', '\\']) {
        return try_base(p);
    }
    // 앱이 설치한 Node.js·uv를 먼저 찾는다.
    crate::runtime::path_dirs()
        .into_iter()
        .find_map(|dir| try_base(&dir.join(cmd)))
        .or_else(|| std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| try_base(&dir.join(cmd))))
}

/// PC에 따로 설치된 프로그램인지 (앱이 설치한 실행 환경은 빼고 PATH만 본다)
pub fn system_program_available(cmd: &str) -> bool {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| exts.iter().any(|e| d.join(format!("{}{}", cmd, e)).is_file())))
        .unwrap_or(false)
}

/// 이 PC에서 그 명령을 찾을 수 있는지 (앱이 설치한 실행 환경 포함)
pub fn program_available(cmd: &str) -> bool {
    #[cfg(windows)]
    {
        resolve_program(cmd).is_some()
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
            .unwrap_or(false)
    }
}

/// cmd.exe 명령줄에 넣을 인자를 따옴표로 감싼다.
#[cfg(windows)]
fn cmd_quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains(|c: char| c.is_whitespace() || "&|<>^()%!\",;=".contains(c)) {
        arg.to_string()
    } else {
        format!("\"{}\"", arg.replace('"', "\"\""))
    }
}

pub fn build_command(command: &str, args: &[String]) -> Result<tokio::process::Command, String> {
    let command = command.trim();
    #[cfg(windows)]
    {
        let program = resolve_program(command)
            .ok_or_else(|| format!("'{}' 명령을 찾을 수 없습니다.{}", command, install_hint(command)))?;
        let ext = program.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let mut c = if ext == "cmd" || ext == "bat" {
            if args.iter().any(|a| a.contains(['\r', '\n'])) {
                return Err("인자에 줄바꿈을 넣을 수 없습니다.".to_string());
            }
            let line = std::iter::once(cmd_quote(&program.to_string_lossy()))
                .chain(args.iter().map(|a| cmd_quote(a)))
                .collect::<Vec<_>>()
                .join(" ");
            let mut c = tokio::process::Command::new("cmd.exe");
            c.arg("/d").arg("/s").arg("/c").raw_arg(format!("\"{}\"", line));
            c
        } else {
            let mut c = tokio::process::Command::new(&program);
            c.args(args);
            c
        };
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
        Ok(c)
    }
    #[cfg(not(windows))]
    {
        let _ = install_hint;
        let mut c = tokio::process::Command::new(command);
        c.args(args);
        Ok(c)
    }
}

/// 프로세스와 그 자식(npx가 띄운 node 등)까지 끝낸다.
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).spawn();
    }
}

// ---------------------------------------------------------------------------
// 연결 (JSON-RPC)
// ---------------------------------------------------------------------------

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>;

struct StdioConn {
    stdin: AsyncMutex<tokio::process::ChildStdin>,
    pid: Option<u32>,
    _child: Mutex<Option<tokio::process::Child>>,
}

impl Drop for StdioConn {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            kill_tree(pid);
        }
    }
}

struct HttpConn {
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    /// OAuth 접근 토큰 (로그인을 쓰는 서버)
    bearer: Mutex<Option<String>>,
    session: Mutex<Option<String>>,
    protocol: Mutex<String>,
}

enum Conn {
    Stdio(StdioConn),
    Http(HttpConn),
}

pub struct Client {
    conn: Conn,
    next_id: AtomicI64,
    pending: Pending,
    closed: Arc<AtomicBool>,
    tools_dirty: Arc<AtomicBool>,
    stderr: Arc<Mutex<String>>,
}

fn push_tail(buf: &Mutex<String>, line: &str) {
    if let Ok(mut b) = buf.lock() {
        b.push_str(line);
        b.push('\n');
        let n = b.chars().count();
        if n > STDERR_TAIL_CHARS {
            *b = b.chars().skip(n - STDERR_TAIL_CHARS).collect();
        }
    }
}

/// 서버가 보낸 메시지 하나를 처리한다. 서버의 요청(ping 등)에 보낼 답을 돌려준다.
fn handle_incoming(msg: &Value, pending: &Pending, tools_dirty: &AtomicBool) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str());
    match (method, msg.get("id")) {
        // 응답
        (None, Some(id)) => {
            if let Some(tx) = id.as_i64().and_then(|id| pending.lock().ok()?.remove(&id)) {
                let _ = tx.send(msg.clone());
            }
            None
        }
        // 서버의 요청
        (Some(m), Some(id)) => Some(if m == "ping" {
            json!({ "jsonrpc": "2.0", "id": id, "result": {} })
        } else {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("지원하지 않는 요청: {}", m) } })
        }),
        // 알림
        (Some(m), None) => {
            if m == "notifications/tools/list_changed" {
                tools_dirty.store(true, Ordering::Relaxed);
            }
            None
        }
        _ => None,
    }
}

pub struct Connected {
    pub client: Arc<Client>,
    pub server_info: String,
    pub tools: Vec<RemoteTool>,
}

impl Client {
    fn spawn_stdio(cfg: &McpServerConfig, env: &[(String, String)]) -> Result<Arc<Client>, String> {
        let mut cmd = build_command(&cfg.command, &cfg.args)?;
        let cwd = if cfg.cwd.trim().is_empty() {
            dirs_home()
        } else {
            PathBuf::from(cfg.cwd.trim())
        };
        if !cwd.is_dir() {
            return Err(format!("실행 위치 폴더가 없습니다: {}", cwd.display()));
        }
        cmd.current_dir(cwd)
            // 앱이 설치한 Node.js·uv를 PATH 앞에 두고, 그다음 사용자가 넣은 환경 변수
            .envs(crate::runtime::child_env())
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("'{}'을(를) 실행하지 못했습니다: {}{}", cfg.command, e, install_hint(&cfg.command)))?;
        let pid = child.id();
        let stdin = child.stdin.take().ok_or("stdin을 열 수 없습니다.")?;
        let stdout = child.stdout.take().ok_or("stdout을 열 수 없습니다.")?;
        let stderr_pipe = child.stderr.take();

        let client = Arc::new(Client {
            conn: Conn::Stdio(StdioConn { stdin: AsyncMutex::new(stdin), pid, _child: Mutex::new(Some(child)) }),
            next_id: AtomicI64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
            closed: Arc::new(AtomicBool::new(false)),
            tools_dirty: Arc::new(AtomicBool::new(false)),
            stderr: Arc::new(Mutex::new(String::new())),
        });

        if let Some(err) = stderr_pipe {
            let buf = client.stderr.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    push_tail(&buf, &line);
                }
            });
        }

        // 읽기 작업: 서버가 보낸 줄을 처리한다. 클라이언트를 약하게 잡아, 연결을 버리면 함께 끝난다.
        let weak = Arc::downgrade(&client);
        let pending = client.pending.clone();
        let closed = client.closed.clone();
        let dirty = client.tools_dirty.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else { continue };
                let msgs = match msg {
                    Value::Array(a) => a,
                    m => vec![m],
                };
                for m in msgs {
                    if let Some(reply) = handle_incoming(&m, &pending, &dirty) {
                        let Some(c) = weak.upgrade() else { break };
                        let _ = c.write_stdio(&reply).await;
                    }
                }
            }
            closed.store(true, Ordering::Relaxed);
            // 기다리던 요청은 모두 실패시킨다 (보낸 쪽 채널이 닫혀 오류가 된다).
            if let Ok(mut p) = pending.lock() {
                p.clear();
            }
        });
        Ok(client)
    }

    fn new_http(cfg: &McpServerConfig, headers: Vec<(String, String)>, bearer: Option<String>) -> Result<Arc<Client>, String> {
        validate_url(&cfg.url)?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(Client {
            conn: Conn::Http(HttpConn {
                client,
                url: cfg.url.trim().to_string(),
                headers,
                bearer: Mutex::new(bearer),
                session: Mutex::new(None),
                protocol: Mutex::new(PROTOCOL_VERSION.to_string()),
            }),
            next_id: AtomicI64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
            closed: Arc::new(AtomicBool::new(false)),
            tools_dirty: Arc::new(AtomicBool::new(false)),
            stderr: Arc::new(Mutex::new(String::new())),
        }))
    }

    pub fn stderr_tail(&self) -> String {
        self.stderr.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    async fn write_stdio(&self, msg: &Value) -> Result<(), String> {
        let Conn::Stdio(s) = &self.conn else { return Ok(()) };
        let mut line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        line.push('\n');
        let mut w = s.stdin.lock().await;
        w.write_all(line.as_bytes()).await.map_err(|e| format!("MCP 서버에 보내지 못했습니다: {}", e))?;
        w.flush().await.map_err(|e| e.to_string())
    }

    fn closed_error(&self) -> String {
        let tail = self.stderr_tail();
        let tail = tail.trim();
        if tail.is_empty() {
            "MCP 서버가 종료되었습니다.".to_string()
        } else {
            let last: String = tail.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            format!("MCP 서버가 종료되었습니다. 서버 출력:\n{}", last)
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        match &self.conn {
            Conn::Stdio(_) => self.write_stdio(&msg).await,
            Conn::Http(h) => h.post(&msg, None, &self.pending, &self.tools_dirty).await.map(|_| ()),
        }
    }

    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        if self.is_closed() {
            return Err(self.closed_error());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let reply = match &self.conn {
            Conn::Stdio(_) => {
                let (tx, rx) = oneshot::channel();
                self.pending.lock().map_err(|e| e.to_string())?.insert(id, tx);
                if let Err(e) = self.write_stdio(&msg).await {
                    self.pending.lock().map_err(|e| e.to_string())?.remove(&id);
                    return Err(if self.is_closed() { self.closed_error() } else { e });
                }
                match tokio::time::timeout(timeout, rx).await {
                    Ok(Ok(v)) => v,
                    Ok(Err(_)) => {
                        // 서버가 남긴 마지막 오류 출력을 마저 읽을 시간을 준다.
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        return Err(self.closed_error());
                    }
                    Err(_) => {
                        if let Ok(mut p) = self.pending.lock() {
                            p.remove(&id);
                        }
                        let _ = self
                            .notify("notifications/cancelled", json!({ "requestId": id, "reason": "timeout" }))
                            .await;
                        return Err(format!("MCP 서버가 {}초 안에 답하지 않았습니다.", timeout.as_secs()));
                    }
                }
            }
            Conn::Http(h) => tokio::time::timeout(timeout, h.post(&msg, Some(id), &self.pending, &self.tools_dirty))
                .await
                .map_err(|_| format!("MCP 서버가 {}초 안에 답하지 않았습니다.", timeout.as_secs()))??
                .ok_or_else(|| "MCP 서버가 응답을 보내지 않았습니다.".to_string())?,
        };
        rpc_result(&reply)
    }

    /// 연결하고 초기화 후 도구 목록까지 받는다.
    /// 로그인하는 서버면 접근 토큰을 바꾼다 (갱신된 토큰).
    pub fn set_bearer(&self, token: Option<String>) {
        if let Conn::Http(h) = &self.conn {
            if let Ok(mut b) = h.bearer.lock() {
                *b = token;
            }
        }
    }

    pub async fn connect(cfg: &McpServerConfig, secrets: &dyn SecretStore, bearer: Option<String>) -> Result<Connected, String> {
        let client = if cfg.transport == "http" {
            let headers = cfg
                .header_keys
                .iter()
                .filter_map(|k| secrets.get(&header_account(&cfg.id, k)).map(|v| (k.clone(), v)))
                .collect();
            Self::new_http(cfg, headers, bearer)?
        } else {
            let env: Vec<(String, String)> = cfg
                .env_keys
                .iter()
                .filter_map(|k| secrets.get(&env_account(&cfg.id, k)).map(|v| (k.clone(), v)))
                .collect();
            Self::spawn_stdio(cfg, &env)?
        };

        let init = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "Any LM Desktop", "version": env!("CARGO_PKG_VERSION") }
                }),
                CONNECT_TIMEOUT,
            )
            .await?;
        if let (Conn::Http(h), Some(v)) = (&client.conn, init.get("protocolVersion").and_then(|v| v.as_str())) {
            if let Ok(mut p) = h.protocol.lock() {
                *p = v.to_string();
            }
        }
        client.notify("notifications/initialized", json!({})).await?;

        let info = init.get("serverInfo").cloned().unwrap_or(Value::Null);
        let server_info = format!(
            "{} {}",
            info.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            info.get("version").and_then(|v| v.as_str()).unwrap_or("")
        )
        .trim()
        .to_string();
        let tools = if init.get("capabilities").and_then(|c| c.get("tools")).is_some() {
            client.list_tools().await?
        } else {
            Vec::new()
        };
        Ok(Connected { client, server_info, tools })
    }

    pub async fn list_tools(&self) -> Result<Vec<RemoteTool>, String> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let res = self.request("tools/list", params, LIST_TIMEOUT).await?;
            tools.extend(parse_tools(&res));
            cursor = res.get("nextCursor").and_then(|c| c.as_str()).map(String::from);
            if cursor.is_none() || tools.len() >= MAX_TOOLS_PER_SERVER {
                break;
            }
        }
        tools.truncate(MAX_TOOLS_PER_SERVER);
        self.tools_dirty.store(false, Ordering::Relaxed);
        Ok(tools)
    }

    pub async fn call_tool(&self, tool: &str, args: &Value) -> Result<(String, bool), String> {
        let arguments = match args {
            Value::Object(_) => args.clone(),
            _ => Value::Object(Map::new()),
        };
        let res = self.request("tools/call", json!({ "name": tool, "arguments": arguments }), CALL_TIMEOUT).await?;
        Ok(result_text(&res))
    }

    pub async fn close(&self) {
        if let Conn::Http(h) = &self.conn {
            let session = h.session.lock().ok().and_then(|s| s.clone());
            if let Some(sid) = session {
                let _ = tokio::time::timeout(
                    Duration::from_secs(3),
                    h.client.delete(&h.url).header("Mcp-Session-Id", sid).send(),
                )
                .await;
            }
        }
        self.closed.store(true, Ordering::Relaxed);
    }
}

impl HttpConn {
    /// 메시지 하나를 POST한다. `id`가 있으면 그 응답을 돌려준다 (알림이면 None).
    async fn post(
        &self,
        msg: &Value,
        id: Option<i64>,
        pending: &Pending,
        dirty: &AtomicBool,
    ) -> Result<Option<Value>, String> {
        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let bearer = self.bearer.lock().ok().and_then(|b| b.clone());
        if let Some(t) = &bearer {
            req = req.header("Authorization", format!("Bearer {}", t));
        }
        let session = self.session.lock().ok().and_then(|s| s.clone());
        if let Some(sid) = &session {
            req = req.header("Mcp-Session-Id", sid.as_str());
        }
        if msg.get("method").and_then(|m| m.as_str()) != Some("initialize") {
            let proto = self.protocol.lock().map(|p| p.clone()).unwrap_or_default();
            req = req.header("MCP-Protocol-Version", proto);
        }
        let resp = req.json(msg).send().await.map_err(|e| format!("MCP 서버에 연결하지 못했습니다: {}", e))?;
        let status = resp.status();
        if let Some(sid) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            if let Ok(mut s) = self.session.lock() {
                *s = Some(sid.to_string());
            }
        }
        if status.as_u16() == 404 && session.is_some() {
            return Err("MCP 서버 세션이 만료되었습니다. 다시 연결하세요.".to_string());
        }
        if status.as_u16() == 401 {
            return Err(format!(
                "{} MCP 서버가 접근을 거부했습니다 (HTTP 401). 로그인을 쓰는 서버면 MCP 서버 창에서 [로그인]을 누르고, 아니면 인증 헤더(예: Authorization: Bearer ...)를 확인하세요.",
                AUTH_REQUIRED
            ));
        }
        if status.as_u16() == 403 {
            let body: String = resp.text().await.unwrap_or_default().chars().take(300).collect();
            return Err(format!(
                "MCP 서버가 접근을 거부했습니다 (HTTP 403). 권한이 부족하거나 서버(API)가 켜져 있지 않습니다. {}",
                body.trim()
            ));
        }
        if !status.is_success() {
            let body: String = resp.text().await.unwrap_or_default().chars().take(300).collect();
            return Err(format!("MCP 서버 오류 (HTTP {}): {}", status.as_u16(), body.trim()));
        }
        let Some(want) = id else { return Ok(None) };
        let ctype = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        if ctype.starts_with("text/event-stream") {
            use futures_util::StreamExt;
            let mut stream = resp.bytes_stream();
            let mut decoder = SseDecoder::default();
            let mut data = String::new();
            let handle = |data: &str| -> Option<Value> {
                let v: Value = serde_json::from_str(data).ok()?;
                let msgs = match v {
                    Value::Array(a) => a,
                    m => vec![m],
                };
                for m in msgs {
                    if m.get("method").is_none() && m.get("id").and_then(|i| i.as_i64()) == Some(want) {
                        return Some(m);
                    }
                    if let Some(reply) = handle_incoming(&m, pending, dirty) {
                        // 서버의 ping 등에 답한다 (기다리지 않는다).
                        let req = self.client.post(&self.url).json(&reply);
                        let req = match &session {
                            Some(s) => req.header("Mcp-Session-Id", s.as_str()),
                            None => req,
                        };
                        tokio::spawn(async move {
                            let _ = req.send().await;
                        });
                    }
                }
                None
            };
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| format!("MCP 응답 수신 오류: {}", e))?;
                for line in decoder.push(&chunk) {
                    if line.is_empty() {
                        if !data.is_empty() {
                            if let Some(m) = handle(&data) {
                                return Ok(Some(m));
                            }
                            data.clear();
                        }
                    } else if let Some(rest) = line.strip_prefix("data:") {
                        if !data.is_empty() {
                            data.push('\n');
                        }
                        data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                    }
                }
            }
            if let Some(line) = decoder.finish() {
                if let Some(rest) = line.strip_prefix("data:") {
                    data.push_str(rest.trim_start());
                }
            }
            if !data.is_empty() {
                if let Some(m) = handle(&data) {
                    return Ok(Some(m));
                }
            }
            return Err("MCP 서버가 응답 없이 스트림을 닫았습니다.".to_string());
        }

        let mut body = Vec::new();
        let mut resp = resp;
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("MCP 응답 수신 오류: {}", e))? {
            body.extend_from_slice(&chunk);
            if body.len() > 16 * 1024 * 1024 {
                return Err("MCP 응답이 너무 큽니다.".to_string());
            }
        }
        let v: Value = serde_json::from_slice(&body).map_err(|_| "MCP 서버 응답을 해석하지 못했습니다.".to_string())?;
        let msgs = match v {
            Value::Array(a) => a,
            m => vec![m],
        };
        msgs.into_iter()
            .find(|m| m.get("id").and_then(|i| i.as_i64()) == Some(want))
            .map(Some)
            .ok_or_else(|| "MCP 서버 응답에 요청 결과가 없습니다.".to_string())
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

// ---------------------------------------------------------------------------
// 서버 관리
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerState {
    /// "off" | "connecting" | "connected" | "error"
    pub status: String,
    pub error: Option<String>,
    pub server_info: String,
    pub tools: Vec<RemoteTool>,
    /// 서버가 표준 오류로 남긴 마지막 출력 (로컬 서버)
    pub log: String,
    /// 로그인(OAuth)을 쓰는 서버에서 로그인되어 있는지
    pub signed_in: bool,
    /// 지금 브라우저 로그인을 기다리는지
    pub signing_in: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    pub config: McpServerConfig,
    pub state: ServerState,
}

pub struct McpManager {
    path: PathBuf,
    secrets: Box<dyn SecretStore>,
    configs: Mutex<Vec<McpServerConfig>>,
    states: Mutex<HashMap<String, ServerState>>,
    clients: Mutex<HashMap<String, Arc<Client>>>,
    /// 연결 시도 번호: 오래된 시도의 결과가 새 상태를 덮지 않게 한다.
    generation: AtomicU64,
    attempts: Mutex<HashMap<String, u64>>,
    /// 토큰 묶음 키 → 토큰 (자격 증명 관리자의 사본)
    tokens: Mutex<HashMap<String, TokenSet>>,
    /// 진행 중인 로그인: (서버 ID, 취소 신호)
    login: Mutex<Option<(String, oneshot::Sender<()>)>>,
    /// 로그인 주소를 여는 방법 (테스트에서 바꾼다)
    browser: Mutex<Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>>,
}

impl McpManager {
    pub fn new(path: PathBuf, secrets: Box<dyn SecretStore>) -> Arc<Self> {
        let configs: Vec<McpServerConfig> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| serde_json::from_value(v.get("servers").cloned().unwrap_or(Value::Null)).ok())
            .unwrap_or_default();
        Arc::new(Self {
            path,
            secrets,
            configs: Mutex::new(configs),
            states: Mutex::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            attempts: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            login: Mutex::new(None),
            browser: Mutex::new(Arc::new(mcp_oauth::open_browser)),
        })
    }

    fn persist(&self, configs: &[McpServerConfig]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let json = serde_json::to_string_pretty(&json!({ "servers": configs })).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("MCP 설정을 저장하지 못했습니다: {}", e))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("MCP 설정을 저장하지 못했습니다: {}", e))
    }

    fn config(&self, id: &str) -> Option<McpServerConfig> {
        self.configs.lock().ok()?.iter().find(|c| c.id == id).cloned()
    }

    fn set_state(&self, id: &str, f: impl FnOnce(&mut ServerState)) {
        if let Ok(mut s) = self.states.lock() {
            f(s.entry(id.to_string()).or_default());
        }
    }

    pub fn list(&self) -> Vec<ServerView> {
        let configs = self.configs.lock().map(|c| c.clone()).unwrap_or_default();
        let states = self.states.lock().map(|s| s.clone()).unwrap_or_default();
        let clients = self.clients.lock().map(|c| c.clone()).unwrap_or_default();
        let login_id = self.login.lock().ok().and_then(|l| l.as_ref().map(|(id, _)| id.clone()));
        configs
            .into_iter()
            .map(|config| {
                let mut state = states.get(&config.id).cloned().unwrap_or_default();
                if state.status.is_empty() {
                    state.status = if config.enabled { "connecting" } else { "off" }.to_string();
                }
                if let Some(key) = token_key(&config) {
                    state.signed_in = self.load_token(&key).is_some();
                    state.signing_in = login_id.as_deref() == Some(config.id.as_str());
                }
                if let Some(c) = clients.get(&config.id) {
                    state.log = c.stderr_tail();
                    if c.is_closed() && state.status == "connected" {
                        state.status = "error".to_string();
                        state.error = Some(c.closed_error());
                    }
                }
                ServerView { config, state }
            })
            .collect()
    }

    /// 서버를 추가하거나 고친다. `env`·`headers`에 값이 빈 항목은 저장된 값을 그대로 둔다.
    pub fn save(
        self: &Arc<Self>,
        mut cfg: McpServerConfig,
        env: HashMap<String, String>,
        headers: HashMap<String, String>,
        oauth_client_secret: Option<String>,
    ) -> Result<McpServerConfig, String> {
        cfg.name = cfg.name.trim().to_string();
        if let Some(o) = cfg.oauth.as_mut() {
            o.client_id = o.client_id.trim().to_string();
            o.scopes = dedup(o.scopes.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        }
        cfg.command = cfg.command.trim().to_string();
        cfg.url = cfg.url.trim().to_string();
        cfg.env_keys = dedup(cfg.env_keys.iter().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()));
        cfg.header_keys = dedup(cfg.header_keys.iter().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()));
        validate_config(&cfg)?;
        let is_new = cfg.id.is_empty();
        if is_new {
            cfg.id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        }
        let old = self.config(&cfg.id);
        if !is_new && old.is_none() {
            return Err("MCP 서버를 찾을 수 없습니다.".to_string());
        }
        if let Some(old) = &old {
            // 도구별 설정은 이 함수로 바꾸지 않는다.
            cfg.disabled_tools = old.disabled_tools.clone();
            cfg.auto_approve = old.auto_approve.clone();
        }
        // 새 이름의 값이 없으면 저장할 수 없다.
        for k in &cfg.env_keys {
            let has_old = old.as_ref().is_some_and(|o| o.env_keys.contains(k))
                && self.secrets.get(&env_account(&cfg.id, k)).is_some();
            if env.get(k).map_or(true, |v| v.is_empty()) && !has_old {
                return Err(format!("환경 변수 {}의 값을 입력하세요.", k));
            }
        }
        for k in &cfg.header_keys {
            let has_old = old.as_ref().is_some_and(|o| o.header_keys.contains(k))
                && self.secrets.get(&header_account(&cfg.id, k)).is_some();
            if headers.get(k).map_or(true, |v| v.is_empty()) && !has_old {
                return Err(format!("헤더 {}의 값을 입력하세요.", k));
            }
        }
        if let (Some(key), Some(secret)) = (token_key(&cfg), oauth_client_secret.filter(|s| !s.trim().is_empty())) {
            self.secrets.set(&oauth_account(&key, "client_secret"), secret.trim())?;
        }
        for (k, v) in env.iter().filter(|(k, v)| !v.is_empty() && cfg.env_keys.contains(k)) {
            self.secrets.set(&env_account(&cfg.id, k), v)?;
        }
        for (k, v) in headers.iter().filter(|(k, v)| !v.is_empty() && cfg.header_keys.contains(k)) {
            self.secrets.set(&header_account(&cfg.id, k), v)?;
        }
        if let Some(old) = &old {
            for k in old.env_keys.iter().filter(|k| !cfg.env_keys.contains(k)) {
                self.secrets.delete(&env_account(&cfg.id, k));
            }
            for k in old.header_keys.iter().filter(|k| !cfg.header_keys.contains(k)) {
                self.secrets.delete(&header_account(&cfg.id, k));
            }
        }
        {
            let mut configs = self.configs.lock().map_err(|e| e.to_string())?;
            let mut next = configs.clone();
            match next.iter_mut().find(|c| c.id == cfg.id) {
                Some(c) => *c = cfg.clone(),
                None => next.push(cfg.clone()),
            }
            self.persist(&next)?;
            *configs = next;
        }
        self.restart(&cfg.id);
        Ok(cfg)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let cfg = self.config(id).ok_or_else(|| "MCP 서버를 찾을 수 없습니다.".to_string())?;
        {
            let mut configs = self.configs.lock().map_err(|e| e.to_string())?;
            let next: Vec<McpServerConfig> = configs.iter().filter(|c| c.id != id).cloned().collect();
            self.persist(&next)?;
            *configs = next;
        }
        self.disconnect(id);
        for k in &cfg.env_keys {
            self.secrets.delete(&env_account(id, k));
        }
        for k in &cfg.header_keys {
            self.secrets.delete(&header_account(id, k));
        }
        if let Ok(mut s) = self.states.lock() {
            s.remove(id);
        }
        if let Some(key) = token_key(&cfg) {
            let shared = self.configs.lock().map(|c| c.iter().any(|o| token_key(o).as_deref() == Some(key.as_str()))).unwrap_or(true);
            if !shared {
                self.forget_login(&key);
                self.secrets.delete(&oauth_account(&key, "client_secret"));
                self.secrets.delete(&oauth_account(&key, "client"));
            }
        }
        Ok(())
    }

    fn update_config(&self, id: &str, f: impl FnOnce(&mut McpServerConfig)) -> Result<McpServerConfig, String> {
        let mut configs = self.configs.lock().map_err(|e| e.to_string())?;
        let mut next = configs.clone();
        let c = next.iter_mut().find(|c| c.id == id).ok_or_else(|| "MCP 서버를 찾을 수 없습니다.".to_string())?;
        f(c);
        let updated = c.clone();
        self.persist(&next)?;
        *configs = next;
        Ok(updated)
    }

    pub fn set_enabled(self: &Arc<Self>, id: &str, enabled: bool) -> Result<(), String> {
        self.update_config(id, |c| c.enabled = enabled)?;
        self.restart(id);
        Ok(())
    }

    /// 도구 하나를 켜고 끄거나(enabled), 승인 없이 실행하게(auto_approve) 한다.
    pub fn set_tool(&self, id: &str, tool: &str, enabled: Option<bool>, auto_approve: Option<bool>) -> Result<(), String> {
        let toggle = |list: &mut Vec<String>, on: bool| {
            list.retain(|t| t != tool);
            if on {
                list.push(tool.to_string());
            }
        };
        self.update_config(id, |c| {
            if let Some(e) = enabled {
                toggle(&mut c.disabled_tools, !e);
            }
            if let Some(a) = auto_approve {
                toggle(&mut c.auto_approve, a);
            }
        })
        .map(|_| ())
    }

    pub fn disconnect(&self, id: &str) {
        if let Ok(mut a) = self.attempts.lock() {
            a.insert(id.to_string(), self.generation.fetch_add(1, Ordering::Relaxed) + 1);
        }
        let client = self.clients.lock().ok().and_then(|mut c| c.remove(id));
        if let Some(c) = client {
            tauri::async_runtime::spawn(async move { c.close().await });
        }
        self.set_state(id, |s| *s = ServerState { status: "off".to_string(), ..Default::default() });
    }

    /// 끊고, 켜져 있으면 다시 연결한다.
    pub fn restart(self: &Arc<Self>, id: &str) {
        self.disconnect(id);
        if self.config(id).is_some_and(|c| c.enabled) {
            self.connect(id);
        }
    }

    pub fn connect(self: &Arc<Self>, id: &str) {
        let Some(cfg) = self.config(id) else { return };
        let attempt = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut a) = self.attempts.lock() {
            a.insert(id.to_string(), attempt);
        }
        self.set_state(id, |s| *s = ServerState { status: "connecting".to_string(), ..Default::default() });
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let res = match me.bearer_for(&cfg).await {
                Ok(bearer) => Client::connect(&cfg, me.secrets.as_ref(), bearer).await,
                Err(e) => Err(e),
            };
            let current = me.attempts.lock().ok().and_then(|a| a.get(&cfg.id).copied()) == Some(attempt);
            if !current {
                if let Ok(c) = res {
                    c.client.close().await;
                }
                return;
            }
            match res {
                Ok(c) => {
                    me.set_state(&cfg.id, |s| {
                        *s = ServerState {
                            status: "connected".to_string(),
                            error: None,
                            server_info: c.server_info.clone(),
                            tools: c.tools.clone(),
                            ..Default::default()
                        }
                    });
                    if let Ok(mut m) = me.clients.lock() {
                        m.insert(cfg.id.clone(), c.client);
                    }
                }
                Err(e) if e.starts_with(AUTH_REQUIRED) => me.set_state(&cfg.id, |s| {
                    *s = ServerState {
                        status: "needs_login".to_string(),
                        error: Some(e.trim_start_matches(AUTH_REQUIRED).trim().to_string()),
                        ..Default::default()
                    }
                }),
                Err(e) => me.set_state(&cfg.id, |s| {
                    *s = ServerState { status: "error".to_string(), error: Some(e), ..Default::default() }
                }),
            }
        });
    }

    pub fn start_all(self: &Arc<Self>) {
        let ids: Vec<String> = self
            .configs
            .lock()
            .map(|c| c.iter().filter(|c| c.enabled).map(|c| c.id.clone()).collect())
            .unwrap_or_default();
        for id in ids {
            self.connect(&id);
        }
    }

    /// 앱을 끝낼 때: 로컬 서버 프로세스를 모두 끝낸다.
    pub fn shutdown_all(&self) {
        if let Ok(mut c) = self.clients.lock() {
            c.clear();
        }
    }

    fn client(&self, id: &str) -> Option<Arc<Client>> {
        self.clients.lock().ok()?.get(id).cloned()
    }

    /// 도구 목록이 바뀌었다고 알려 온 서버는 목록을 다시 받는다.
    async fn refresh_dirty(&self) {
        let dirty: Vec<(String, Arc<Client>)> = self
            .clients
            .lock()
            .map(|m| {
                m.iter()
                    .filter(|(_, c)| c.tools_dirty.load(Ordering::Relaxed) && !c.is_closed())
                    .map(|(id, c)| (id.clone(), c.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (id, c) in dirty {
            if let Ok(tools) = c.list_tools().await {
                self.set_state(&id, |s| s.tools = tools);
            }
        }
    }

    /// 지금 모델에게 줄 MCP 도구 (연결된 서버의 켜진 도구)
    pub async fn tool_specs(&self) -> Vec<ToolSpec> {
        self.refresh_dirty().await;
        let configs = self.configs.lock().map(|c| c.clone()).unwrap_or_default();
        let states = self.states.lock().map(|s| s.clone()).unwrap_or_default();
        let mut used = HashSet::new();
        let mut out = Vec::new();
        for cfg in configs.iter().filter(|c| c.enabled) {
            let Some(state) = states.get(&cfg.id).filter(|s| s.status == "connected") else { continue };
            if self.client(&cfg.id).map_or(true, |c| c.is_closed()) {
                continue;
            }
            for t in state.tools.iter().filter(|t| !cfg.disabled_tools.contains(&t.name)) {
                let desc: String = format!("[MCP 서버 '{}'] {}", cfg.name, t.description.trim())
                    .chars()
                    .take(MAX_DESCRIPTION_CHARS)
                    .collect();
                out.push(ToolSpec {
                    exposed: exposed_name(&cfg.name, &t.name, &mut used),
                    server_id: cfg.id.clone(),
                    server_name: cfg.name.clone(),
                    tool: t.name.clone(),
                    description: desc,
                    schema: t.input_schema.clone(),
                    auto_approve: cfg.auto_approve.contains(&t.name),
                });
            }
        }
        out
    }

    /// MCP 도구를 실행한다. (결과 글, 오류 여부)
    pub async fn call(&self, server_id: &str, tool: &str, args: &Value) -> Result<(String, bool), String> {
        let client = self
            .client(server_id)
            .ok_or_else(|| "MCP 서버가 연결되어 있지 않습니다. MCP 설정에서 서버 상태를 확인하세요.".to_string())?;
        let cfg = self.config(server_id);
        let uses_login = cfg.as_ref().is_some_and(|c| c.oauth.is_some());
        if uses_login {
            match self.bearer_for(cfg.as_ref().unwrap()).await {
                Ok(t) => client.set_bearer(t),
                Err(e) => return Err(self.mark_needs_login(server_id, e)),
            }
        }
        let mut res = client.call_tool(tool, args).await;
        if uses_login && res.as_ref().is_err_and(|e| e.starts_with(AUTH_REQUIRED)) {
            // 토큰이 서버에서 만료되었을 수 있다: 한 번 갱신해 다시 시도한다.
            match self.bearer_for_refreshed(cfg.as_ref().unwrap()).await {
                Ok(t) => {
                    client.set_bearer(t);
                    res = client.call_tool(tool, args).await;
                }
                Err(e) => return Err(self.mark_needs_login(server_id, e)),
            }
            if let Err(e) = &res {
                if e.starts_with(AUTH_REQUIRED) {
                    return Err(self.mark_needs_login(server_id, e.clone()));
                }
            }
        }
        if client.is_closed() {
            let err = client.closed_error();
            self.set_state(server_id, |s| {
                s.status = "error".to_string();
                s.error = Some(err);
            });
        }
        res
    }
}

// ---------------------------------------------------------------------------
// OAuth 로그인
// ---------------------------------------------------------------------------

/// 토큰을 함께 쓰는 단위: 묶음 이름이 있으면 (묶음, 클라이언트 ID), 없으면 서버 하나
pub fn token_key(cfg: &McpServerConfig) -> Option<String> {
    let o = cfg.oauth.as_ref()?;
    Some(if o.group.trim().is_empty() {
        format!("server:{}", cfg.id)
    } else {
        format!("{}:{}", o.group.trim(), o.client_id)
    })
}

fn oauth_account(key: &str, what: &str) -> String {
    format!("mcp-oauth:{}:{}", key, what)
}

impl McpManager {
    fn load_token(&self, key: &str) -> Option<TokenSet> {
        if let Some(t) = self.tokens.lock().ok()?.get(key) {
            return Some(t.clone());
        }
        let t: TokenSet = serde_json::from_str(&self.secrets.get(&oauth_account(key, "token"))?).ok()?;
        if let Ok(mut m) = self.tokens.lock() {
            m.insert(key.to_string(), t.clone());
        }
        Some(t)
    }

    fn store_token(&self, key: &str, t: &TokenSet) {
        let _ = self.secrets.set(&oauth_account(key, "token"), &t.for_storage());
        if let Ok(mut m) = self.tokens.lock() {
            m.insert(key.to_string(), t.clone());
        }
    }

    fn forget_login(&self, key: &str) {
        self.secrets.delete(&oauth_account(key, "token"));
        if let Ok(mut m) = self.tokens.lock() {
            m.remove(key);
        }
    }

    fn mark_needs_login(&self, server_id: &str, err: String) -> String {
        let msg = err.trim_start_matches(AUTH_REQUIRED).trim().to_string();
        self.set_state(server_id, |s| {
            s.status = "needs_login".to_string();
            s.error = Some(msg.clone());
        });
        format!("{} {}", AUTH_REQUIRED, msg)
    }

    /// 같은 토큰을 쓰는 서버들이 필요로 하는 권한을 모두 모은다.
    fn group_scopes(&self, key: &str) -> Vec<String> {
        let configs = self.configs.lock().map(|c| c.clone()).unwrap_or_default();
        dedup(
            configs
                .iter()
                .filter(|c| token_key(c).as_deref() == Some(key))
                .flat_map(|c| c.oauth.as_ref().map(|o| o.scopes.clone()).unwrap_or_default()),
        )
    }

    /// 요청에 붙일 접근 토큰. 로그인을 쓰지 않는 서버면 None, 로그인이 필요하면 AUTH_REQUIRED 오류.
    pub async fn bearer_for(&self, cfg: &McpServerConfig) -> Result<Option<String>, String> {
        let Some(key) = token_key(cfg) else { return Ok(None) };
        let Some(t) = self.load_token(&key) else {
            return Err(format!("{} 아직 로그인하지 않았습니다. [로그인]을 눌러 계정을 연결하세요.", AUTH_REQUIRED));
        };
        let needed = cfg.oauth.as_ref().map(|o| o.scopes.clone()).unwrap_or_default();
        if !t.covers(&needed) {
            return Err(format!("{} 이 서버에 필요한 권한을 아직 허락받지 않았습니다. [로그인]을 다시 눌러 권한을 허락하세요.", AUTH_REQUIRED));
        }
        if t.is_fresh(mcp_oauth::now_secs()) {
            return Ok(Some(t.access_token));
        }
        self.refresh_token(&key, &t).await.map(Some)
    }

    async fn bearer_for_refreshed(&self, cfg: &McpServerConfig) -> Result<Option<String>, String> {
        let Some(key) = token_key(cfg) else { return Ok(None) };
        let t = self
            .load_token(&key)
            .ok_or_else(|| format!("{} 다시 로그인하세요.", AUTH_REQUIRED))?;
        self.refresh_token(&key, &t).await.map(Some)
    }

    async fn refresh_token(&self, key: &str, t: &TokenSet) -> Result<String, String> {
        let secret = self.client_secret(key);
        match mcp_oauth::refresh(t, secret.as_deref()).await {
            Ok(new) => {
                self.store_token(key, &new);
                Ok(new.access_token)
            }
            Err(e) => Err(format!("{} 로그인이 만료되었습니다. 다시 로그인하세요. ({})", AUTH_REQUIRED, e)),
        }
    }

    fn client_secret(&self, key: &str) -> Option<String> {
        self.secrets.get(&oauth_account(key, "client_secret")).or_else(|| {
            let v: Value = serde_json::from_str(&self.secrets.get(&oauth_account(key, "client"))?).ok()?;
            v.get("client_secret").and_then(|s| s.as_str()).map(String::from)
        })
    }

    /// 같은 토큰을 쓰는 서버를 모두 다시 연결한다.
    fn restart_group(self: &Arc<Self>, key: &str) {
        let ids: Vec<String> = self
            .configs
            .lock()
            .map(|c| c.iter().filter(|c| token_key(c).as_deref() == Some(key)).map(|c| c.id.clone()).collect())
            .unwrap_or_default();
        for id in ids {
            self.restart(&id);
        }
    }

    /// 브라우저로 로그인한다. 끝날 때까지(최대 5분) 기다린다.
    pub async fn login(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let cfg = self.config(id).ok_or_else(|| "MCP 서버를 찾을 수 없습니다.".to_string())?;
        let settings = cfg.oauth.clone().ok_or_else(|| "이 서버는 로그인을 쓰지 않습니다.".to_string())?;
        let key = token_key(&cfg).unwrap_or_default();

        let ep = mcp_oauth::discover(&cfg.url, &settings).await?;
        let (client_id, client_secret) = if !settings.client_id.is_empty() {
            (settings.client_id.clone(), self.client_secret(&key))
        } else if let Some(v) = self
            .secrets
            .get(&oauth_account(&key, "client"))
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .filter(|v| v.get("token_endpoint").and_then(|t| t.as_str()) == Some(ep.token.as_str()))
        {
            (
                v.get("client_id").and_then(|s| s.as_str()).unwrap_or_default().to_string(),
                v.get("client_secret").and_then(|s| s.as_str()).map(String::from),
            )
        } else {
            let reg = ep.registration.clone().ok_or_else(|| {
                "이 서버는 앱 자동 등록을 지원하지 않습니다. 서버 안내에 따라 OAuth 클라이언트 ID를 만들어 서버 설정에 입력하세요.".to_string()
            })?;
            let (cid, csec) = mcp_oauth::register_client(&reg).await?;
            let _ = self.secrets.set(
                &oauth_account(&key, "client"),
                &json!({ "client_id": cid, "client_secret": csec, "token_endpoint": ep.token }).to_string(),
            );
            (cid, csec)
        };
        let mut scopes = self.group_scopes(&key);
        if scopes.is_empty() {
            scopes = ep.scopes_supported.clone();
        }

        // 이전 로그인 시도가 있으면 멈춘다 (결과를 받는 포트는 하나다).
        let (tx, rx) = oneshot::channel();
        if let Ok(mut l) = self.login.lock() {
            if let Some((_, old)) = l.replace((id.to_string(), tx)) {
                let _ = old.send(());
            }
        }
        let release = |me: &Arc<Self>| {
            if let Ok(mut l) = me.login.lock() {
                if l.as_ref().is_some_and(|(i, _)| i == id) {
                    *l = None;
                }
            }
        };
        let result = async {
            if self.login.lock().ok().and_then(|l| l.as_ref().map(|(i, _)| i.clone())).as_deref() != Some(id) {
                return Err("다른 로그인이 시작되어 멈췄습니다.".to_string());
            }
            let pkce = mcp_oauth::pkce();
            let state = uuid::Uuid::new_v4().simple().to_string();
            let url = mcp_oauth::authorization_url(&ep, &client_id, &scopes, &state, &pkce.challenge, &settings.extra_params)?;
            // 결과를 받을 준비를 먼저 한 뒤 브라우저를 연다.
            let waiter = tokio::spawn({
                let state = state.clone();
                async move { mcp_oauth::wait_for_code(&state, rx).await }
            });
            tokio::time::sleep(Duration::from_millis(100)).await;
            let open = self.browser.lock().map(|b| b.clone()).map_err(|e| e.to_string())?;
            if let Err(e) = open(&url) {
                waiter.abort();
                return Err(e);
            }
            let code = waiter.await.map_err(|e| e.to_string())??;
            mcp_oauth::exchange_code(&ep, &client_id, client_secret.as_deref(), &code, &pkce.verifier, &scopes).await
        }
        .await;
        release(self);
        let token = result?;
        self.store_token(&key, &token);
        self.restart_group(&key);
        Ok(())
    }

    pub fn cancel_login(&self) {
        if let Ok(mut l) = self.login.lock() {
            if let Some((_, tx)) = l.take() {
                let _ = tx.send(());
            }
        }
    }

    /// 로그아웃: 저장한 토큰을 지우고 다시 연결한다 (로그인 필요 상태가 된다).
    pub fn logout(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let cfg = self.config(id).ok_or_else(|| "MCP 서버를 찾을 수 없습니다.".to_string())?;
        let key = token_key(&cfg).ok_or_else(|| "이 서버는 로그인을 쓰지 않습니다.".to_string())?;
        self.forget_login(&key);
        self.restart_group(&key);
        Ok(())
    }

    #[cfg(test)]
    pub fn set_browser(&self, f: Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>) {
        if let Ok(mut b) = self.browser.lock() {
            *b = f;
        }
    }
}

fn dedup(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items.filter(|k| seen.insert(k.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_fit_api_limits() {
        let mut used = HashSet::new();
        let a = exposed_name("파일 시스템", "read_file", &mut used);
        assert_eq!(a, "mcp__x__read_file");
        let b = exposed_name("GitHub Server", "create-or-update-file", &mut used);
        assert_eq!(b, "mcp__github_server__create_or_update_file");
        let long = exposed_name("a very long server name indeed", &"t".repeat(100), &mut used);
        assert!(long.len() <= MAX_TOOL_NAME, "{}", long);
        let dup = exposed_name("파일 시스템", "read_file", &mut used);
        assert_eq!(dup, "mcp__x__read_file_2");
    }

    #[test]
    fn tool_results_become_text() {
        let (t, err) = result_text(&json!({
            "content": [
                { "type": "text", "text": "첫째" },
                { "type": "image", "data": "...", "mimeType": "image/png" },
                { "type": "resource", "resource": { "uri": "file:///a.txt", "text": "파일 내용" } }
            ]
        }));
        assert!(!err);
        assert!(t.starts_with("첫째") && t.contains("image/png") && t.ends_with("파일 내용"));
        let (t, err) = result_text(&json!({ "content": [], "structuredContent": { "n": 1 }, "isError": true }));
        assert!(err && t.contains("\"n\": 1"));
        assert_eq!(result_text(&json!({})).0, "(결과 없음)");
    }

    #[test]
    fn schemas_are_normalized() {
        assert_eq!(normalize_schema(&Value::Null), json!({ "type": "object", "properties": {} }));
        let s = normalize_schema(&json!({ "$schema": "x", "type": "object", "required": ["a"] }));
        assert_eq!(s, json!({ "type": "object", "required": ["a"], "properties": {} }));
    }

    #[test]
    fn urls_must_be_https_or_local() {
        assert!(validate_url("https://mcp.example.com/mcp").is_ok());
        assert!(validate_url("http://localhost:3000/mcp").is_ok());
        assert!(validate_url("http://127.0.0.1/mcp").is_ok());
        assert!(validate_url("http://example.com/mcp").is_err());
        assert!(validate_url("http://localhost.evil.com/mcp").is_err());
        assert!(validate_url("file:///c:/x").is_err());
    }

    fn manager(dir: &Path) -> Arc<McpManager> {
        McpManager::new(dir.join("mcp.json"), Box::new(MemorySecrets::default()))
    }

    fn stdio_config(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            enabled: false,
            transport: "stdio".into(),
            command: "node".into(),
            ..Default::default()
        }
    }

    #[test]
    fn secrets_are_kept_out_of_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let m = manager(dir.path());
        let mut cfg = stdio_config("srv");
        cfg.env_keys = vec!["API_KEY".into()];
        // 값이 없으면 저장할 수 없다.
        assert!(m.save(cfg.clone(), HashMap::new(), HashMap::new(), None).is_err());
        let saved = m
            .save(cfg, HashMap::from([("API_KEY".to_string(), "secret-123".to_string())]), HashMap::new(), None)
            .unwrap();
        let file = std::fs::read_to_string(dir.path().join("mcp.json")).unwrap();
        assert!(file.contains("API_KEY") && !file.contains("secret-123"));
        assert_eq!(m.secrets.get(&env_account(&saved.id, "API_KEY")).as_deref(), Some("secret-123"));

        // 빈 값으로 다시 저장하면 기존 값을 유지한다.
        m.save(saved.clone(), HashMap::from([("API_KEY".to_string(), String::new())]), HashMap::new(), None).unwrap();
        assert_eq!(m.secrets.get(&env_account(&saved.id, "API_KEY")).as_deref(), Some("secret-123"));

        // 도구 설정
        m.set_tool(&saved.id, "danger", Some(false), Some(true)).unwrap();
        let c = m.config(&saved.id).unwrap();
        assert_eq!(c.disabled_tools, vec!["danger"]);
        assert_eq!(c.auto_approve, vec!["danger"]);

        // 다시 읽어도 같다
        let m2 = manager(dir.path());
        assert_eq!(m2.list().len(), 1);
        assert_eq!(m2.list()[0].state.status, "off");

        m.delete(&saved.id).unwrap();
        assert!(m.secrets.get(&env_account(&saved.id, "API_KEY")).is_none());
        assert!(m.list().is_empty());
    }

    #[test]
    fn invalid_configs_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let m = manager(dir.path());
        let mut c = stdio_config("");
        assert!(m.save(c.clone(), HashMap::new(), HashMap::new(), None).is_err());
        c.name = "x".into();
        c.command = " ".into();
        assert!(m.save(c.clone(), HashMap::new(), HashMap::new(), None).is_err());
        let http = McpServerConfig { name: "h".into(), transport: "http".into(), url: "http://example.com".into(), ..Default::default() };
        assert!(m.save(http, HashMap::new(), HashMap::new(), None).is_err());
        let mut bad_env = stdio_config("e");
        bad_env.env_keys = vec!["BAD KEY".into()];
        assert!(m.save(bad_env, HashMap::from([("BAD KEY".into(), "v".into())]), HashMap::new(), None).is_err());
    }

    /// 간단한 가짜 MCP 서버 (Node.js로 실행)
    const FAKE_SERVER: &str = r#"
const rl = require('readline').createInterface({ input: process.stdin });
const send = (m) => process.stdout.write(JSON.stringify(m) + '\n');
console.error('fake server ready');
rl.on('line', (line) => {
  const m = JSON.parse(line);
  if (m.method === 'initialize') send({ jsonrpc: '2.0', id: m.id, result: { protocolVersion: m.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: 'fake', version: '1.0' } } });
  else if (m.method === 'tools/list' && !m.params.cursor) send({ jsonrpc: '2.0', id: m.id, result: { tools: [{ name: 'echo', description: '그대로 돌려줌', inputSchema: { type: 'object', properties: { text: { type: 'string' } } } }], nextCursor: 'p2' } });
  else if (m.method === 'tools/list') send({ jsonrpc: '2.0', id: m.id, result: { tools: [{ name: 'env', description: '환경 변수' }] } });
  else if (m.method === 'tools/call' && m.params.name === 'echo') {
    send({ jsonrpc: '2.0', id: 99, method: 'ping' });
    send({ jsonrpc: '2.0', id: m.id, result: { content: [{ type: 'text', text: 'echo: ' + m.params.arguments.text }] } });
  }
  else if (m.method === 'tools/call' && m.params.name === 'env') send({ jsonrpc: '2.0', id: m.id, result: { content: [{ type: 'text', text: process.env.MY_TOKEN || '' }] } });
  else if (m.method === 'tools/call') { console.error('crashing'); process.exit(3); }
  else if (m.id !== undefined && m.method) send({ jsonrpc: '2.0', id: m.id, error: { code: -32601, message: 'nope' } });
});
"#;

    fn node_available() -> bool {
        std::process::Command::new(if cfg!(windows) { "node.exe" } else { "node" })
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[tokio::test]
    async fn stdio_server_lists_and_calls_tools() {
        if !node_available() {
            eprintln!("node가 없어 건너뜀");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake server.js");
        std::fs::write(&script, FAKE_SERVER).unwrap();
        let secrets = MemorySecrets::default();
        secrets.set(&env_account("s1", "MY_TOKEN"), "토큰값").unwrap();
        let cfg = McpServerConfig {
            id: "s1".into(),
            name: "fake".into(),
            enabled: true,
            transport: "stdio".into(),
            command: "node".into(),
            args: vec![script.to_string_lossy().to_string()],
            env_keys: vec!["MY_TOKEN".into()],
            ..Default::default()
        };
        let c = Client::connect(&cfg, &secrets, None).await.unwrap();
        assert_eq!(c.server_info, "fake 1.0");
        let names: Vec<&str> = c.tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["echo", "env"], "여러 쪽의 도구 목록을 모두 받는다");
        assert_eq!(c.tools[1].input_schema, json!({ "type": "object", "properties": {} }));

        let (out, err) = c.client.call_tool("echo", &json!({ "text": "안녕" })).await.unwrap();
        assert_eq!((out.as_str(), err), ("echo: 안녕", false));
        let (out, _) = c.client.call_tool("env", &json!({})).await.unwrap();
        assert_eq!(out, "토큰값", "환경 변수 값은 비밀 저장소에서 넘겨준다");

        // 서버가 죽으면 표준 오류 출력과 함께 알려 준다.
        let err = c.client.call_tool("crash", &json!({})).await.unwrap_err();
        assert!(err.contains("종료") && err.contains("crashing"), "{}", err);
        assert!(c.client.is_closed());
    }

    #[test]
    fn missing_program_gives_install_hint() {
        let err = build_command("npx-does-not-exist-xyz", &[]).err();
        // 이름이 npx로 시작하지 않으므로 일반 메시지
        assert!(err.unwrap().contains("찾을 수 없습니다"));
        assert!(install_hint("npx").contains("Node.js"));
        assert!(install_hint("uvx").contains("uv"));
    }

    /// 응답을 JSON 또는 SSE로 주는 가짜 HTTP MCP 서버
    async fn http_server(sse: bool) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 4096];
                    // 머리글과 본문을 다 읽는다
                    let (head_end, len) = loop {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                            let len = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                                .unwrap_or(0);
                            break (p + 4, len);
                        }
                    };
                    while buf.len() < head_end + len {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_lowercase();
                    let body: Value = serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null);
                    let method = body["method"].as_str().unwrap_or("").to_string();
                    if method != "initialize" && !head.contains("mcp-session-id: sess-1") {
                        let _ = sock.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n").await;
                        return;
                    }
                    if !head.contains("authorization: bearer tok") {
                        let _ = sock.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n").await;
                        return;
                    }
                    let result = match method.as_str() {
                        "initialize" => json!({ "protocolVersion": PROTOCOL_VERSION, "capabilities": { "tools": {} }, "serverInfo": { "name": "remote" } }),
                        "tools/list" => json!({ "tools": [{ "name": "search", "inputSchema": { "type": "object" } }] }),
                        "tools/call" => json!({ "content": [{ "type": "text", "text": format!("찾음: {}", body["params"]["arguments"]["q"].as_str().unwrap_or("")) }] }),
                        _ => {
                            let _ = sock.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n").await;
                            return;
                        }
                    };
                    let reply = json!({ "jsonrpc": "2.0", "id": body["id"], "result": result }).to_string();
                    let resp = if sse && method != "initialize" {
                        let payload = format!(
                            "event: message\ndata: {}\n\ndata: {}\n\n",
                            json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": {} }),
                            reply
                        );
                        format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}", payload.len(), payload)
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: sess-1\r\nContent-Length: {}\r\n\r\n{}",
                            reply.len(),
                            reply
                        )
                    };
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://127.0.0.1:{}/mcp", addr.port())
    }

    /// 가짜 OAuth 권한 서버 + 로그인이 필요한 MCP 서버 (한 주소에서)
    async fn oauth_mcp_server() -> (String, Arc<Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let challenge = Arc::new(Mutex::new(String::new()));
        let (b, lg) = (base.clone(), log.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                let (base, log, challenge) = (b.clone(), lg.clone(), challenge.clone());
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 4096];
                    let (head_end, len) = loop {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                            let len = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                                .unwrap_or(0);
                            break (p + 4, len);
                        }
                    };
                    while buf.len() < head_end + len {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                    let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
                    let first = head.lines().next().unwrap_or("").to_string();
                    let target = first.split_whitespace().nth(1).unwrap_or("").to_string();
                    let url = reqwest::Url::parse(&format!("{}{}", base, target)).unwrap();
                    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
                    let form: HashMap<String, String> = reqwest::Url::parse(&format!("http://x/?{}", body)).map(|u| u.query_pairs().into_owned().collect()).unwrap_or_default();
                    let auth = head.lines().find_map(|l| l.to_lowercase().strip_prefix("authorization: bearer ").map(String::from));
                    log.lock().unwrap().push(format!("{} {}", first.split_whitespace().next().unwrap_or(""), url.path()));
                    let json_resp = |status: &str, v: Value| {
                        let b = v.to_string();
                        format!("HTTP/1.1 {}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}", status, b.len(), b)
                    };
                    let resp = match url.path() {
                        "/.well-known/oauth-protected-resource" => json_resp("200 OK", json!({ "resource": format!("{}/mcp", base), "authorization_servers": [base] })),
                        "/.well-known/oauth-authorization-server" => json_resp("200 OK", json!({
                            "issuer": base, "authorization_endpoint": format!("{}/authorize", base),
                            "token_endpoint": format!("{}/token", base), "registration_endpoint": format!("{}/register", base)
                        })),
                        "/register" => json_resp("201 Created", json!({ "client_id": "dyn-client" })),
                        "/authorize" => {
                            assert_eq!(q["client_id"], "dyn-client");
                            assert_eq!(q["resource"], format!("{}/mcp", base), "발견한 resource를 보낸다");
                            *challenge.lock().unwrap() = q["code_challenge"].clone();
                            let loc = format!("{}?code=the-code&state={}", q["redirect_uri"], q["state"]);
                            format!("HTTP/1.1 302 Found\r\nLocation: {}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n", loc)
                        }
                        "/token" => match form.get("grant_type").map(String::as_str) {
                            Some("authorization_code") => {
                                use base64::Engine;
                                use sha2::Digest;
                                let calc = base64::engine::general_purpose::URL_SAFE_NO_PAD
                                    .encode(sha2::Sha256::digest(form["code_verifier"].as_bytes()));
                                if calc != *challenge.lock().unwrap() || form["code"] != "the-code" {
                                    json_resp("400 Bad Request", json!({ "error": "invalid_grant" }))
                                } else {
                                    json_resp("200 OK", json!({ "access_token": "acc-1", "refresh_token": "ref-1", "expires_in": 3600 }))
                                }
                            }
                            Some("refresh_token") if form["refresh_token"] == "ref-1" => {
                                json_resp("200 OK", json!({ "access_token": "acc-2", "expires_in": 3600 }))
                            }
                            _ => json_resp("400 Bad Request", json!({ "error": "invalid_grant" })),
                        },
                        "/mcp" => {
                            let msg: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                            if !matches!(auth.as_deref(), Some("acc-1") | Some("acc-2")) {
                                let b = "{}";
                                format!(
                                    "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                                    base, b.len(), b
                                )
                            } else {
                                let result = match msg["method"].as_str().unwrap_or("") {
                                    "initialize" => json!({ "protocolVersion": PROTOCOL_VERSION, "capabilities": { "tools": {} }, "serverInfo": { "name": "secure" } }),
                                    "tools/list" => json!({ "tools": [{ "name": "whoami", "inputSchema": { "type": "object" } }] }),
                                    "tools/call" => json!({ "content": [{ "type": "text", "text": format!("token={}", auth.clone().unwrap_or_default()) }] }),
                                    _ => {
                                        return {
                                            let _ = sock.write_all(b"HTTP/1.1 202 Accepted\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await;
                                        };
                                    }
                                };
                                json_resp("200 OK", json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result }))
                            }
                        }
                        _ => "HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".to_string(),
                    };
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        (base, log)
    }

    async fn wait_status(m: &McpManager, id: &str, want: &str) -> ServerState {
        for _ in 0..100 {
            if let Some(v) = m.list().into_iter().find(|v| v.config.id == id) {
                if v.state.status == want {
                    return v.state;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("상태가 {}가 되지 않음: {:?}", want, m.list().into_iter().find(|v| v.config.id == id).map(|v| v.state));
    }

    #[tokio::test]
    async fn oauth_login_discovery_registration_and_refresh() {
        let (base, log) = oauth_mcp_server().await;
        let dir = tempfile::tempdir().unwrap();
        let m = manager(dir.path());
        // 브라우저 대신: 로그인 주소를 열면 권한 서버가 내 PC 주소로 돌려보낸다.
        m.set_browser(Arc::new(|url: &str| {
            let url = url.to_string();
            tokio::spawn(async move {
                let _ = reqwest::get(&url).await;
            });
            Ok(())
        }));
        let cfg = McpServerConfig {
            name: "secure".into(),
            enabled: true,
            transport: "http".into(),
            url: format!("{}/mcp", base),
            oauth: Some(OAuthSettings::default()),
            ..Default::default()
        };
        let saved = m.save(cfg, HashMap::new(), HashMap::new(), None).unwrap();
        let st = wait_status(&m, &saved.id, "needs_login").await;
        assert!(!st.signed_in);

        m.login(&saved.id).await.unwrap();
        let st = wait_status(&m, &saved.id, "connected").await;
        assert!(st.signed_in);
        assert_eq!(st.tools.len(), 1);
        let (out, _) = m.call(&saved.id, "whoami", &json!({})).await.unwrap();
        assert_eq!(out, "token=acc-1");
        assert!(log.lock().unwrap().iter().any(|l| l == "POST /register"), "클라이언트를 자동 등록한다");

        // 토큰이 만료되면 갱신 토큰으로 새 토큰을 받는다.
        let key = token_key(&m.config(&saved.id).unwrap()).unwrap();
        let mut t = m.load_token(&key).unwrap();
        t.expires_at = Some(0);
        m.store_token(&key, &t);
        let (out, _) = m.call(&saved.id, "whoami", &json!({})).await.unwrap();
        assert_eq!(out, "token=acc-2");
        let stored: TokenSet = serde_json::from_str(&m.secrets.get(&oauth_account(&key, "token")).unwrap()).unwrap();
        assert_eq!(stored.refresh_token.as_deref(), Some("ref-1"));

        // 로그아웃하면 다시 로그인이 필요하다.
        m.logout(&saved.id).unwrap();
        wait_status(&m, &saved.id, "needs_login").await;
        m.delete(&saved.id).unwrap();
        assert!(m.secrets.get(&oauth_account(&key, "client")).is_none(), "마지막 서버를 지우면 등록 정보도 지운다");
    }

    #[tokio::test]
    async fn http_server_with_json_and_sse_replies() {
        for sse in [false, true] {
            let url = http_server(sse).await;
            let secrets = MemorySecrets::default();
            let cfg = McpServerConfig {
                id: "h1".into(),
                name: "remote".into(),
                transport: "http".into(),
                url: url.clone(),
                header_keys: vec!["Authorization".into()],
                ..Default::default()
            };
            // 헤더가 없으면 거부된다
            let err = Client::connect(&cfg, &secrets, None).await.err().unwrap();
            assert!(err.contains("401"), "{}", err);

            secrets.set(&header_account("h1", "Authorization"), "Bearer tok").unwrap();
            let c = Client::connect(&cfg, &secrets, None).await.unwrap();
            assert_eq!(c.tools.len(), 1);
            let (out, _) = c.client.call_tool("search", &json!({ "q": "모델" })).await.unwrap();
            assert_eq!(out, "찾음: 모델");
        }
    }
}
