//! 실제 LM Studio 서버로 앱의 에이전트 루프를 그대로 돌려 보는 테스트 하니스.
//!
//! 앱과 같은 코드(`run_exaone_turn`, 도구 실행, 승인 처리)를 쓰고, 화면 대신 시나리오 파일이 사용자를 맡는다.
//! 앱 설정 파일이나 자격 증명 관리자는 읽지 않는다. 서버 주소와 모델은 환경 변수로만 받는다.
//!
//! 실행 (PowerShell, 저장소 루트에서):
//!   $env:ANYLM_MODEL = "qwen3.5-9b"
//!   cd src-tauri
//!   cargo test --lib harness::live -- --ignored --nocapture
//!
//! 환경 변수:
//!   ANYLM_MODEL      시험할 모델 key (필수)
//!   ANYLM_SERVER     LM Studio 서버 주소 (기본: http://localhost:1234)
//!   ANYLM_CONTEXT    모델이 로드된 컨텍스트 길이 (기본: 32768) — 줄이기 기준 계산에 쓴다
//!   ANYLM_TOKEN      LM Studio API 토큰 (선택)
//!   ANYLM_SCENARIOS  시나리오 JSON 파일 (기본: ../harness/scenarios.json)
//!   ANYLM_OUT        결과 폴더 (기본: ../harness/runs/default)
//!   ANYLM_ONLY       실행할 시나리오 id를 쉼표로 (없으면 전부)
//!   ANYLM_PAUSE_SEC  시나리오 사이에 쉬는 시간 (기본 1초)
//!   ANYLM_RERUN      1이면 이미 결과가 있는 시나리오도 다시 실행
//!
//! 시나리오의 `mcpServers`에 적은 MCP 서버는 앱의 `mcp` 모듈(McpManager)로 연결해 앱과 같은 도구 목록·[MCP] 절로 시험하고,
//! 시나리오가 끝나면 지운다. 비밀 값(헤더·환경 변수)은 메모리에만 둔다(자격 증명 관리자에 쓰지 않는다).
//!
//! 이 하니스는 모델을 로드하거나 내리지 않는다. LM Studio에서 모델을 미리 로드해 둘 것.

use crate::exaone::{self, RoundTrace, TurnRequest, Tuning};
use crate::store::{AppSettings, ChatMessage, Project};
use crate::{doc_bridge, knowledge, lmstudio, mcp, run_approved_tool, runtime, skills};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct Check {
    /// fileExists | fileMissing | fileContains | fileNotContains | toolCalled | toolCalledContains | toolNotCalled
    /// | skillAutoLoaded | answerContains | answerNotContains | answerMinChars | answerMaxChars | noToolErrors | finished
    /// path·text 안의 `{today}`는 시나리오를 시작한 날짜(YYYY-MM-DD)로 바뀐다.
    /// path의 파일 이름에 `*`가 있으면 맞는 파일 중 가장 최근에 바뀐 하나를 본다(예: `reports/주간보고_*.md`).
    kind: String,
    path: String,
    text: String,
    n: usize,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct Scenario {
    id: String,
    /// 무엇을 보려는 시나리오인지 (사람이 읽는 설명)
    note: String,
    /// "chat" 또는 "project"
    mode: String,
    tuning: Tuning,
    /// 시나리오에서만 `on`이나 `off`를 지정한다. 비어 있으면 모델 기본 동작을 따른다.
    reasoning: Option<String>,
    instructions: String,
    /// 프로젝트 폴더에 미리 만들어 둘 파일 (상대 경로 → 내용)
    files: BTreeMap<String, String>,
    /// 프로젝트 Skill (이름 → SKILL.md 내용)
    skills: BTreeMap<String, String>,
    /// 지식 폴더로 쓸 파일 (상대 경로 → 내용). 프로젝트 폴더 밖 `work/<id>-kb`에 만든다.
    knowledge_files: BTreeMap<String, String>,
    /// 이미 있는 지식 폴더(절대 경로). 하니스는 읽기만 한다.
    knowledge_folders: Vec<String>,
    /// 사용자가 차례로 보내는 메시지
    turns: Vec<String>,
    /// 승인을 요청받으면 허락할 도구 이름 (그 밖에는 거부). MCP 도구는 여기 없어도 하니스가 승인하고 기록에 남긴다.
    approve: Vec<String>,
    /// 연결할 MCP 서버 (앱의 mcp 모듈로 연결한다)
    mcp_servers: Vec<HarnessMcp>,
    /// false면 Skill 자동 불러오기를 끈다 (없으면 앱 기본값: 켜짐)
    skill_auto_load: Option<bool>,
    checks: Vec<Check>,
}

/// 시나리오에서 연결할 MCP 서버 하나. `url`이 있으면 원격(HTTP), 없으면 `command`를 실행하는 로컬 서버.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct HarnessMcp {
    name: String,
    url: String,
    headers: BTreeMap<String, String>,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    /// 찾아보기 항목 ID (예: "exa" — 내장 검색 서버면 [MCP] 절에 웹 검색 안내 줄이 붙는다)
    catalog_id: String,
    /// 승인 없이 실행할 도구 (서버의 원래 도구 이름). 나머지는 승인 요청으로 멈추고 하니스가 승인한다.
    auto_approve: Vec<String>,
}

impl HarnessMcp {
    fn config(&self) -> mcp::McpServerConfig {
        mcp::McpServerConfig {
            name: self.name.clone(),
            enabled: true,
            transport: if self.url.trim().is_empty() { "stdio" } else { "http" }.to_string(),
            command: self.command.clone(),
            args: self.args.clone(),
            url: self.url.clone(),
            env_keys: self.env.keys().cloned().collect(),
            header_keys: self.headers.keys().cloned().collect(),
            auto_approve: self.auto_approve.clone(),
            catalog_id: self.catalog_id.clone(),
            ..Default::default()
        }
    }
}

/// MCP 서버 연결 시도의 최대 대기 시간
const MCP_CONNECT_WAIT: Duration = Duration::from_secs(60);

/// 시나리오의 MCP 서버를 앱과 같은 관리자에 추가하고 연결될 때까지 기다린다. (서버 ID, 기록) 목록.
async fn connect_mcp_servers(mgr: &Arc<mcp::McpManager>, servers: &[HarnessMcp]) -> Vec<(Option<String>, Value)> {
    let mut out = Vec::new();
    for s in servers {
        let env = s.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let headers = s.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let cfg = match mgr.save(s.config(), env, headers, None) {
            Ok(c) => c,
            Err(e) => {
                println!("  ! MCP '{}' 설정 오류: {}", s.name, e);
                out.push((None, json!({ "name": s.name, "status": "error", "error": e })));
                continue;
            }
        };
        let started = Instant::now();
        let view = loop {
            let view = mgr.list().into_iter().find(|v| v.config.id == cfg.id);
            let settled = view.as_ref().is_some_and(|v| !matches!(v.state.status.as_str(), "connecting" | ""));
            if settled || started.elapsed() > MCP_CONNECT_WAIT {
                break view;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        let (status, error, tools) = match &view {
            Some(v) => (v.state.status.clone(), v.state.error.clone(), v.state.tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>()),
            None => ("error".to_string(), Some("서버를 찾지 못했습니다.".to_string()), Vec::new()),
        };
        println!(
            "  · MCP '{}' {} ({}ms){} 도구 {:?}",
            s.name,
            status,
            started.elapsed().as_millis(),
            error.as_deref().map(|e| format!(" 오류: {}", e.chars().take(160).collect::<String>())).unwrap_or_default(),
            tools
        );
        out.push((Some(cfg.id.clone()), json!({ "name": s.name, "status": status, "error": error, "tools": tools, "connectMs": started.elapsed().as_millis() as u64 })));
    }
    out
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnLog {
    prompt: String,
    error: Option<String>,
    message: Option<ChatMessage>,
    approvals: Vec<Value>,
    traces: Vec<RoundTrace>,
    wall_ms: u64,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.to_string())
}

fn env_num(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

/// 하니스 실행 설정. 앱 설정 대신 환경 변수에서 만든다.
#[derive(Debug, Clone, PartialEq)]
pub struct HarnessConfig {
    pub server_url: String,
    pub model: String,
    pub context_length: u32,
    pub out_dir: PathBuf,
}

/// `get`으로 환경 변수를 읽어 설정을 만든다(테스트에서 실제 환경을 건드리지 않으려고 getter를 받는다).
pub fn config_from_env(get: impl Fn(&str) -> Option<String>) -> Result<HarnessConfig, String> {
    let val = |name: &str| get(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let model = val("ANYLM_MODEL")
        .ok_or_else(|| "ANYLM_MODEL 환경 변수에 시험할 모델 key를 넣어 주세요. (예: qwen3.5-9b)".to_string())?;
    let context_length = match val("ANYLM_CONTEXT") {
        Some(v) => v.parse::<u32>().map_err(|_| format!("ANYLM_CONTEXT는 숫자여야 합니다: {}", v))?,
        None => 32768,
    };
    Ok(HarnessConfig {
        server_url: val("ANYLM_SERVER").unwrap_or_else(|| "http://localhost:1234".to_string()),
        model,
        context_length,
        out_dir: PathBuf::from(val("ANYLM_OUT").unwrap_or_else(|| "../harness/runs/default".to_string())),
    })
}

/// 화면의 문서 변환기 대신 쓰는 흉내: docx/xlsx/pptx를 Markdown 글 그대로 저장하고 읽는다.
/// (모델이 어떤 내용을 쓰는지 보는 것이 목적이라 실제 파일 형식은 만들지 않는다.)
fn mock_app_with_converter() -> tauri::App<tauri::test::MockRuntime> {
    use base64::Engine;
    use tauri::{Listener, Manager};
    let app = tauri::test::mock_app();
    app.manage(doc_bridge::DocBridge::default());
    let handle = app.handle().clone();
    let responder = handle.clone();
    handle.listen(doc_bridge::REQUEST_EVENT, move |event| {
        let v: Value = serde_json::from_str(event.payload()).unwrap_or_default();
        let id = v["requestId"].as_str().unwrap_or("").to_string();
        let b64 = base64::engine::general_purpose::STANDARD;
        let decode = |v: &Value| -> String {
            v["dataBase64"]
                .as_str()
                .and_then(|s| b64.decode(s).ok())
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default()
        };
        let reply = match v["op"].as_str().unwrap_or("") {
            "generate" => doc_bridge::BridgeReply {
                ok: true,
                data_base64: Some(b64.encode(v["markdown"].as_str().unwrap_or(""))),
                ..Default::default()
            },
            "extract" => doc_bridge::BridgeReply { ok: true, text: Some(decode(&v)), ..Default::default() },
            _ => {
                let text = decode(&v);
                let target = v["target"].as_str().unwrap_or("");
                if !target.is_empty() && text.contains(target) {
                    let new = text.replacen(target, v["replacement"].as_str().unwrap_or(""), 1);
                    doc_bridge::BridgeReply { ok: true, data_base64: Some(b64.encode(new)), ..Default::default() }
                } else {
                    doc_bridge::BridgeReply {
                        ok: false,
                        error: Some("바꿀 문구를 문서에서 찾지 못했습니다.".into()),
                        ..Default::default()
                    }
                }
            }
        };
        let _ = responder.state::<doc_bridge::DocBridge>().respond(&id, reply);
    });
    app
}

/// 모델이 만든 명령·스크립트를 사람 확인 없이 실행하므로, 시험 폴더 밖을 건드리거나
/// 네트워크·삭제·다른 프로그램 실행을 하는 것은 실행하지 않고 거부한다.
fn unsafe_reason(tool: &str, args: &Value, root: &Path) -> Option<String> {
    if tool == "start_server" {
        let fixture: Value = serde_json::from_str(include_str!("../../harness/dev-server.json"))
            .expect("하니스 개발 서버 fixture");
        let expected = fixture[0]["files"]["server.cjs"].as_str();
        let script = crate::fs_ops::resolve_safe_path(root, "server.cjs").ok()
            .and_then(|p| std::fs::read_to_string(p).ok());
        if args["command"].as_str() == Some("node server.cjs")
            && matches!(args["cwd"].as_str(), None | Some("" | "."))
            && (args["url"].is_null() || args["url"].as_str() == Some(""))
            && expected.is_some() && script.as_deref() == expected
        {
            return None;
        }
        return Some("하니스의 검증용 로컬 서버 명령만 허용합니다.".into());
    }
    let text = match tool {
        "run_command" => args["command"].as_str().unwrap_or("").to_string(),
        "run_python" => {
            if !args["skill"].as_str().unwrap_or("").trim().is_empty() {
                return Some("Skill 스크립트는 하니스에서 실행하지 않습니다.".into());
            }
            let rel = args["relative_path"].as_str().unwrap_or("");
            if rel.contains("..") || Path::new(rel).is_absolute() {
                return Some("프로젝트 밖 경로".into());
            }
            match std::fs::read_to_string(root.join(rel)) {
                Ok(s) => s,
                Err(_) => return None, // 없는 파일: 실행 단계에서 오류로 돌아간다
            }
        }
        _ => return None,
    };
    let lower = text.to_lowercase();
    let common = ["..\\", "../", "~", ":\\", ":/", "\\\\", "appdata", "userprofile", "homepath", "http://", "https://"];
    let command_only = [
        "remove-item", "rmdir", "del ", "erase ", "rm ", "rd ", "format", "invoke-webrequest", "invoke-restmethod", "curl",
        "wget", "start-process", "reg ", "set-executionpolicy", "shutdown", "restart-computer", "$env:", "iex",
        "invoke-expression", "net ", "schtasks", "new-service", "sc ", "pip ", "npm ", "winget", "choco", "git push",
        "set-itemproperty", "stop-process", "taskkill", "powershell", "cmd ", "start ", "move-item", "copy-item",
    ];
    let python_only = [
        "shutil.rmtree", "os.remove", "os.unlink", "os.rmdir", "subprocess", "os.system", "os.popen", "socket", "urllib",
        "requests", "http.client", "httpx", "winreg", "ctypes", "expanduser", "os.environ", "eval(", "exec(", "__import__",
        "ftplib", "smtplib", "webbrowser", "os.startfile", "shutil.move", "pathlib.path.home", "path.home",
    ];
    let extra: &[&str] = if tool == "run_command" { &command_only } else { &python_only };
    common
        .iter()
        .chain(extra.iter())
        .find(|p| lower.contains(**p))
        .map(|p| format!("안전 규칙에 걸리는 표현: {}", p.trim()))
}

fn list_files(root: &Path) -> Vec<(String, u64)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, u64)>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.push((rel.to_string_lossy().replace('\\', "/"), e.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// 검사 안의 `{today}`를 시나리오 시작 날짜로 바꾼다 (날짜가 들어간 파일 이름을 검사할 때).
fn expand_today(c: &Check, today: &str) -> Check {
    Check { path: c.path.replace("{today}", today), text: c.text.replace("{today}", today), ..c.clone() }
}

/// `*`(아무 글자나 0개 이상)만 아는 이름 맞추기
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// 검사할 파일: 이름에 `*`가 있으면 그 폴더에서 맞는 파일 중 가장 최근에 바뀐 것(없으면 원래 경로).
fn check_file_path(root: &Path, rel: &str) -> PathBuf {
    let direct = root.join(rel);
    let Some(name) = Path::new(rel).file_name().and_then(|n| n.to_str()).filter(|n| n.contains('*')) else {
        return direct;
    };
    let dir = direct.parent().map(Path::to_path_buf).unwrap_or_else(|| root.to_path_buf());
    std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file() && e.file_name().to_str().is_some_and(|n| wildcard_match(name, n)))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
        .unwrap_or(direct)
}

fn run_check(c: &Check, root: &Path, turns: &[TurnLog], today: &str) -> (bool, String) {
    let c = &expand_today(c, today);
    let last = turns.iter().rev().find_map(|t| t.message.as_ref());
    let answer = last.map(|m| m.content.clone()).unwrap_or_default();
    let all_records: Vec<&crate::store::ToolExecutionRecord> =
        turns.iter().filter_map(|t| t.message.as_ref()).flat_map(|m| m.tool_records.iter()).collect();
    // 앱이 미리 넣은 Skill 카드(automatic)는 모델이 부른 도구가 아니므로 도구 검사에서 뺀다.
    let all_tools: Vec<&crate::store::ToolExecutionRecord> = all_records.iter().copied().filter(|t| !t.automatic).collect();
    let target = check_file_path(root, &c.path);
    let file = || std::fs::read_to_string(&target).unwrap_or_default();
    let ok = match c.kind.as_str() {
        "fileExists" => target.is_file(),
        "fileMissing" => !target.exists(),
        "fileContains" => file().contains(&c.text),
        // text를 '|'로 나눈 것 중 하나라도 있으면 통과 (숫자 표기가 여러 가지일 때)
        "fileContainsAny" => c.text.split('|').any(|t| file().contains(t)),
        "answerContainsAny" => c.text.split('|').any(|t| answer.contains(t)),
        "fileMinChars" => file().chars().count() >= c.n,
        // 이 시나리오에서 쓴 API 호출 수가 n회 이하인지 (호출을 아끼는지 보는 검사)
        "maxCalls" => turns.iter().map(|t| t.traces.len()).sum::<usize>() <= c.n,
        "fileNotContains" => target.is_file() && !file().contains(&c.text),
        "toolCalled" => all_tools.iter().any(|t| t.tool_name == c.text),
        // MCP 도구는 모델에 mcp__<서버>__<도구>로 보이므로 이름의 일부로 찾는다.
        "toolCalledContains" => all_tools.iter().any(|t| t.tool_name.contains(&c.text)),
        "toolNotCalled" => !all_tools.iter().any(|t| t.tool_name == c.text),
        // 앱이 이 Skill을 자동으로 미리 넣었는지 (text = Skill 이름)
        "skillAutoLoaded" => all_records
            .iter()
            .any(|t| skills::auto_record_skill(t).is_some_and(|(name, _)| name == c.text)),
        "answerContains" => answer.contains(&c.text),
        "answerNotContains" => !answer.contains(&c.text),
        "answerMinChars" => answer.chars().count() >= c.n,
        "answerMaxChars" => answer.chars().count() <= c.n,
        "noToolErrors" => !all_tools.iter().any(|t| t.status == "error"),
        "finished" => {
            turns.iter().all(|t| t.error.is_none())
                && !answer.contains("오류로 답변이 중간에 멈췄습니다")
                && !answer.contains("출력 길이 제한에 걸려")
        }
        other => return (false, format!("알 수 없는 검사: {}", other)),
    };
    let label = [c.kind.as_str(), c.path.as_str(), c.text.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    (ok, if c.n > 0 { format!("{} {}", label, c.n) } else { label })
}

fn usage_num(t: &RoundTrace, key: &str) -> u64 {
    t.usage.as_ref().and_then(|u| u.get(key)).and_then(Value::as_u64).unwrap_or(0)
}

/// 실행을 통째로 멈춰야 하는 오류인지 (토큰 문제)
fn is_fatal(err: &str) -> bool {
    err.contains("인증을 요구") || err.contains("(401") || err.contains("(403")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "실제 LM Studio 서버를 호출한다"]
async fn live() {
    let config = config_from_env(|k| std::env::var(k).ok()).unwrap_or_else(|e| panic!("{}", e));
    let scenarios_path = PathBuf::from(env_or("ANYLM_SCENARIOS", "../harness/scenarios.json"));
    let out_dir = config.out_dir.clone();
    let pause = Duration::from_secs(env_num("ANYLM_PAUSE_SEC", 1));
    let rerun = env_or("ANYLM_RERUN", "0") == "1";
    let only: Vec<String> =
        env_or("ANYLM_ONLY", "").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

    let raw = std::fs::read_to_string(&scenarios_path)
        .unwrap_or_else(|e| panic!("시나리오 파일을 읽지 못했습니다 ({}): {}", scenarios_path.display(), e));
    let scenarios: Vec<Scenario> = serde_json::from_str(&raw).expect("시나리오 JSON 형식 오류");

    std::fs::create_dir_all(out_dir.join("transcripts")).unwrap();
    std::fs::create_dir_all(out_dir.join("work")).unwrap();
    let out_dir = std::fs::canonicalize(&out_dir).unwrap();
    let results_path = out_dir.join("results.jsonl");
    let done_ids: Vec<String> = std::fs::read_to_string(&results_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| v["id"].as_str().map(str::to_string))
        .collect();

    // 토큰은 선택이다. 없으면 인증 헤더 없이 보낸다.
    let api_key = std::env::var("ANYLM_TOKEN").unwrap_or_default();
    let base_settings = AppSettings {
        api_key_set: !api_key.trim().is_empty(),
        api_key,
        server_url: config.server_url.clone(),
        model: config.model.clone(),
        ..AppSettings::default()
    };
    let info = lmstudio::turn_model_info(&base_settings, &config.model).await;
    let display_name = info.display_name.clone().unwrap_or_else(|| config.model.clone());
    println!("서버 {} / 모델 {} ({}) / 컨텍스트 {}", config.server_url, config.model, display_name, config.context_length);

    // 앱이 설치해 둔 Python을 쓰고, 사용자 Skill은 섞이지 않게 빈 폴더를 쓴다.
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        runtime::set_root(Path::new(&local).join("io.github.kidarigun.anylm-desktop").join("runtimes"));
    }
    skills::set_user_dir(out_dir.join("user-skills"));

    let app = mock_app_with_converter();
    // MCP: 앱과 같은 관리자. 설정 파일은 결과 폴더에 두고(지난 실행의 것은 지운다), 비밀 값은 메모리에만.
    let mcp_path = out_dir.join("mcp-servers.json");
    let _ = std::fs::remove_file(&mcp_path);
    let mcp_manager = mcp::McpManager::new(mcp_path, Box::new(mcp::MemorySecrets::default()));
    {
        use tauri::Manager;
        // 지식 폴더 색인(앱 상태가 없어 폴더 목록은 시나리오에서 받는다). 문서 변환은 위의 흉내 변환기.
        app.manage(knowledge::KnowledgeManager::new(out_dir.join("knowledge")));
        app.manage(mcp_manager.clone());
        app.manage(crate::terminal::TerminalManager::new(Arc::new(|_| {})));
        app.manage(crate::dev_servers::DevServers::default());
    }
    exaone::trace_start();
    let mut stop_reason: Option<String> = None;

    for sc in &scenarios {
        if !only.is_empty() && !only.contains(&sc.id) {
            continue;
        }
        if !rerun && done_ids.contains(&sc.id) {
            println!("[건너뜀] {} (이미 결과 있음)", sc.id);
            continue;
        }
        let work = out_dir.join("work").join(&sc.id);
        if work.exists() {
            std::fs::remove_dir_all(&work).unwrap();
        }
        std::fs::create_dir_all(&work).unwrap();
        for (rel, content) in &sc.files {
            let p = work.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        for (name, md) in &sc.skills {
            let p = work.join(".skills").join(name);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("SKILL.md"), md).unwrap();
        }
        let mut kb_folders = sc.knowledge_folders.clone();
        if !sc.knowledge_files.is_empty() {
            let kb = out_dir.join("work").join(format!("{}-kb", sc.id));
            if kb.exists() {
                std::fs::remove_dir_all(&kb).unwrap();
            }
            for (rel, content) in &sc.knowledge_files {
                let p = kb.join(rel);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, content).unwrap();
            }
            kb_folders.push(kb.to_string_lossy().trim_start_matches(r"\\?\").to_string());
        }
        let project = (sc.mode == "project").then(|| Project {
            id: format!("harness-{}", sc.id),
            name: format!("시험 프로젝트 {}", sc.id),
            // 앱과 같은 모양(\\?\ 없는 경로)으로 준다.
            root_path: work.to_string_lossy().trim_start_matches(r"\\?\").to_string(),
            instructions: sc.instructions.clone(),
            created_at: chrono::Local::now().to_rfc3339(),
            auto_approve: Vec::new(),
            memory: Vec::new(),
            knowledge_folders: kb_folders.clone(),
            dev_servers: Vec::new(),
        });
        let root = project.as_ref().map(|p| PathBuf::from(&p.root_path)).unwrap_or_else(|| work.clone());
        // 앱에서는 폴더를 더할 때 바로 색인이 돈다. 같게 맞추려고 시나리오 시작 전에 끝까지 색인해 둔다
        // (턴 시작은 최대 1.5초만 기다리므로, 첫 턴이 색인을 놓치지 않게).
        if let Some(p) = project.as_ref().filter(|p| !p.knowledge_folders.is_empty()) {
            knowledge::refresh_project(app.handle(), &p.id, p.knowledge_folders.clone(), knowledge::RefreshMode::Full).await;
        }

        let mut settings = base_settings.clone();
        if let Some(reasoning @ ("on" | "off")) = sc.reasoning.as_deref() {
            settings.model_prefs.entry(config.model.clone()).or_default().reasoning = Some(reasoning.into());
        }
        if let Some(on) = sc.skill_auto_load {
            settings.skill_auto_load = on;
        }
        exaone::set_tuning(sc.tuning.clone());

        println!("\n=== {} [{}] {}", sc.id, sc.mode, sc.note);
        let mcp_conn = connect_mcp_servers(&mcp_manager, &sc.mcp_servers).await;
        let mut auto_skills: Vec<String> = Vec::new();
        let started_at = chrono::Local::now().to_rfc3339();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let sc_start = Instant::now();
        let mut history: Vec<ChatMessage> = Vec::new();
        let mut turns: Vec<TurnLog> = Vec::new();
        let mut fatal = false;

        for prompt in &sc.turns {
            let turn_start = Instant::now();
            history.push(ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: "user".into(),
                content: prompt.clone(),
                reasoning_content: None,
                tool_records: Vec::new(),
                created_at: chrono::Local::now().to_rfc3339(),
                ..Default::default()
            });
            let mut log = TurnLog {
                prompt: prompt.clone(),
                error: None,
                message: None,
                approvals: Vec::new(),
                traces: Vec::new(),
                wall_ms: 0,
            };
            let mut continue_from: Option<ChatMessage> = None;
            // 앱의 턴 시작과 같다: 빠른 갱신 뒤 이번 메시지로 참고 자료를 찾는다.
            let knowledge_section =
                knowledge::context_for_turn(app.handle(), project.as_ref(), prompt, config.context_length).await;
            if !knowledge_section.is_empty() {
                println!("  · 참고 자료 {}자 ({}조각)", knowledge_section.chars().count(), knowledge_section.matches("### 출처: ").count());
            }
            // 앱의 턴 시작과 같다: 이번 메시지에 맞는 Skill을 골라 미리 넣는다(설정이 켜져 있을 때).
            let project_ctx = project.as_ref().map(|p| (p.id.as_str(), Path::new(&p.root_path)));
            let auto_skill = skills::auto_for_turn(project_ctx, prompt, &history, config.context_length, settings.skill_auto_load);
            // 앱과 같다: 참고 자료와 Skill을 함께 넣을 때 둘을 합쳐 맥락의 25% 안으로 (참고 자료가 먼저 줄어든다).
            let knowledge_section = match &auto_skill {
                Some(s) => knowledge::fit_beside_skill(&knowledge_section, s.text.chars().count(), config.context_length),
                None => knowledge_section,
            };
            if let Some(s) = &auto_skill {
                println!("  · Skill 자동 불러옴: {} ({:?}, {}자{})", s.name, s.reason, s.text.chars().count(), if s.truncated { ", 잘림" } else { "" });
                auto_skills.push(s.name.clone());
            }

            // 승인 대기로 멈추면 승인/거부를 처리하고 같은 메시지를 이어 간다 (앱의 승인 흐름과 같다).
            let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
            loop {
                let turn = exaone::run_exaone_turn(
                    app.handle(),
                    TurnRequest {
                        settings: &settings,
                        model_key: &config.model,
                        model_display_name: &display_name,
                        reasoning_options: &info.reasoning_options,
                        context_length: config.context_length,
                        project: project.as_ref(),
                        conversation_id: &sc.id,
                        history: &history,
                        compaction: None,
                        continue_from: continue_from.take(),
                        mode: &sc.mode,
                        checkpoint: None,
                        knowledge: &knowledge_section,
                        auto_skill: auto_skill.as_ref(),
                    },
                    Arc::new(AtomicBool::new(false)),
                );
                let res = match tokio::time::timeout_at(deadline, turn).await {
                    Ok(res) => res,
                    Err(_) => {
                        log.error = Some("하니스 턴의 10분 검증 시간이 지났습니다.".into());
                        break;
                    }
                };
                let traces = exaone::trace_take();
                log.traces.extend(traces);

                let mut msg = match res {
                    Ok((m, _)) => m,
                    Err(e) => {
                        println!("  ! 오류: {}", e.chars().take(200).collect::<String>());
                        fatal = is_fatal(&e);
                        log.error = Some(e);
                        break;
                    }
                };
                // 이어 가는 중이면 기록의 마지막(같은 메시지)을 새 내용으로 바꾼다.
                if history.last().is_some_and(|m| m.id == msg.id) {
                    history.pop();
                }

                let pending: Vec<usize> = msg
                    .tool_records
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.status == "pending_approval")
                    .map(|(i, _)| i)
                    .collect();
                if pending.is_empty() {
                    history.push(msg.clone());
                    log.message = Some(msg);
                    break;
                }
                for i in pending {
                    let (tool_id, tool_name, args) = {
                        let r = &msg.tool_records[i];
                        (r.id.clone(), r.tool_name.clone(), serde_json::from_str::<Value>(&r.arguments_json).unwrap_or_default())
                    };
                    let blocked = unsafe_reason(&tool_name, &args, &root);
                    // MCP 도구: 앱이라면 사용자에게 승인을 물었을 도구다. 하니스는 승인하고 기록에 남긴다.
                    let is_mcp = tool_name.starts_with(mcp::TOOL_PREFIX);
                    let allowed = sc.approve.contains(&tool_name) || is_mcp;
                    let (status, output, produced) = if !allowed {
                        ("rejected".to_string(), "사용자가 실행을 거부했습니다.".to_string(), Vec::new())
                    } else if let Some(why) = &blocked {
                        ("rejected".to_string(), format!("사용자가 실행을 거부했습니다. ({})", why), Vec::new())
                    } else {
                        let approval_cancel = AtomicBool::new(false);
                        let run = run_approved_tool(app.handle(), project.as_ref(), &tool_id, &tool_name, &args, false, &approval_cancel);
                        match tokio::time::timeout_at(deadline, run).await {
                            Ok(run) => (run.status.to_string(), run.output, run.produced),
                            Err(_) => ("error".into(), "하니스 승인 실행 검증 시간이 지났습니다.".into(), Vec::new()),
                        }
                    };
                    println!("  · 승인 요청 {} → {}{}", tool_name, status, if is_mcp { " (MCP: 앱이면 승인을 물었을 도구, 하니스가 승인)" } else { "" });
                    log.approvals.push(json!({ "tool": tool_name, "args": args, "allowed": allowed, "blocked": blocked, "status": status, "wouldAskApproval": true, "harnessApproved": is_mcp }));
                    let r = &mut msg.tool_records[i];
                    r.status = status;
                    r.output = output;
                    r.produced_files = produced;
                }
                history.push(msg.clone());
                log.message = Some(msg.clone());
                continue_from = Some(msg);
            }

            log.wall_ms = turn_start.elapsed().as_millis() as u64;
            for t in &log.traces {
                println!(
                    "  라운드: {}ms (헤더 {:?} / 추론 시작 {:?} / 글 시작 {:?}) 추론 {}자 글 {}자 도구 {:?} 종료 {:?} 토큰 in {} out {} (추론 {}) {}",
                    t.total_ms,
                    t.headers_ms,
                    t.first_reasoning_ms,
                    t.first_content_ms,
                    t.reasoning_chars,
                    t.content_chars,
                    t.tool_calls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
                    t.finish_reason,
                    usage_num(t, "prompt_tokens"),
                    usage_num(t, "completion_tokens"),
                    t.usage.as_ref().and_then(crate::exaone::reasoning_tokens_of).unwrap_or(0),
                    t.error.as_deref().map(|e| format!("오류: {}", e.chars().take(120).collect::<String>())).unwrap_or_default(),
                );
            }
            let stop_turns = log.error.is_some();
            turns.push(log);
            if stop_turns || stop_reason.is_some() {
                break;
            }
        }

        // MCP 서버 연결 끊기·지우기 (다음 시나리오에 남지 않게)
        for id in mcp_conn.iter().filter_map(|(id, _)| id.as_ref()) {
            let _ = mcp_manager.delete(id);
        }
        {
            use tauri::Manager;
            app.state::<crate::terminal::TerminalManager>().close_all();
        }

        let checks: Vec<Value> = sc
            .checks
            .iter()
            .map(|c| {
                let (ok, label) = run_check(c, &root, &turns, &today);
                println!("  [{}] {}", if ok { "통과" } else { "실패" }, label);
                json!({ "ok": ok, "check": label })
            })
            .collect();
        let all_traces: Vec<&RoundTrace> = turns.iter().flat_map(|t| t.traces.iter()).collect();
        let incomplete = turns.len() < sc.turns.len();
        let summary = json!({
            "id": sc.id,
            "startedAt": started_at,
            "mode": sc.mode,
            "reasoning": sc.reasoning,
            "tuning": sc.tuning,
            "incomplete": incomplete,
            "wallMs": sc_start.elapsed().as_millis() as u64,
            "apiCalls": all_traces.len(),
            "turns": turns.len(),
            "errors": turns.iter().filter_map(|t| t.error.clone()).collect::<Vec<_>>(),
            "roundErrors": all_traces.iter().filter_map(|t| t.error.clone()).collect::<Vec<_>>(),
            "finishReasons": all_traces.iter().map(|t| t.finish_reason.clone().unwrap_or_default()).collect::<Vec<_>>(),
            "tools": all_traces.iter().flat_map(|t| t.tool_calls.iter().map(|c| c.name.clone())).collect::<Vec<_>>(),
            "badToolJson": all_traces.iter().flat_map(|t| t.tool_calls.iter()).filter(|c| !c.args_valid_json).count(),
            "toolErrors": turns.iter().filter_map(|t| t.message.as_ref()).flat_map(|m| m.tool_records.iter()).filter(|r| r.status == "error").count(),
            "promptTokens": all_traces.iter().map(|t| usage_num(t, "prompt_tokens")).max().unwrap_or(0),
            "completionTokens": all_traces.iter().map(|t| usage_num(t, "completion_tokens")).sum::<u64>(),
            "reasoningTokens": all_traces.iter().map(|t| t.usage.as_ref().and_then(crate::exaone::reasoning_tokens_of).unwrap_or(0)).sum::<u64>(),
            "reasoningChars": all_traces.iter().map(|t| t.reasoning_chars).sum::<usize>(),
            "contentChars": all_traces.iter().map(|t| t.content_chars).sum::<usize>(),
            "maxGapMs": all_traces.iter().map(|t| t.max_gap_ms).max().unwrap_or(0),
            // 앱이 미리 불러온 Skill (모델이 부른 load_skill은 "tools"에 있다)
            "autoSkills": auto_skills,
            "mcp": mcp_conn.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>(),
            "checksPassed": checks.iter().filter(|c| c["ok"] == true).count(),
            "checksTotal": checks.len(),
            "checks": checks,
        });

        let transcript = json!({
            "scenario": sc,
            "summary": summary,
            "turns": turns,
            "files": list_files(&root),
        });
        std::fs::write(
            out_dir.join("transcripts").join(format!("{}.json", sc.id)),
            serde_json::to_string_pretty(&transcript).unwrap(),
        )
        .unwrap();
        // 중간에 끊긴 시나리오는 결과로 치지 않는다 (다음 실행에서 다시 한다).
        if !incomplete || fatal {
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&results_path).unwrap();
            writeln!(f, "{}", summary).unwrap();
        }
        println!(
            "  → 호출 {}회, {}초, 검사 {}/{}",
            summary["apiCalls"],
            sc_start.elapsed().as_secs(),
            summary["checksPassed"],
            summary["checksTotal"]
        );

        if fatal {
            stop_reason = Some("서버 연결 또는 인증 문제로 중단".to_string());
        }
        if stop_reason.is_some() {
            break;
        }
        tokio::time::sleep(pause).await;
    }

    if let Some(r) = stop_reason {
        println!("중단 이유: {}", r);
    }
}

#[test]
fn guard_blocks_risky_commands_and_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let cmd = |c: &str| unsafe_reason("run_command", &json!({ "command": c }), dir.path());
    assert!(cmd("Get-ChildItem").is_none());
    assert!(cmd("python make_chart.py").is_none());
    assert!(cmd("Remove-Item -Recurse .").is_some());
    assert!(cmd("Get-Content C:\\Users\\a\\secret.txt").is_some());
    assert!(cmd("curl https://example.com").is_some());
    assert!(cmd("type ..\\other.txt").is_some());

    std::fs::write(dir.path().join("ok.py"), "import openpyxl\nwb = openpyxl.Workbook()\nwb.save('out.xlsx')\nprint('out.xlsx')\n").unwrap();
    std::fs::write(dir.path().join("bad.py"), "import shutil\nshutil.rmtree('data')\n").unwrap();
    std::fs::write(dir.path().join("net.py"), "import requests\nrequests.get('x')\n").unwrap();
    let py = |p: &str| unsafe_reason("run_python", &json!({ "relative_path": p }), dir.path());
    assert!(py("ok.py").is_none());
    assert!(py("bad.py").is_some());
    assert!(py("net.py").is_some());
    assert!(py("../x.py").is_some());
    assert!(unsafe_reason("delete_path", &json!({ "relative_path": "a.txt" }), dir.path()).is_none());
}

#[test]
fn config_requires_model() {
    let err = config_from_env(|_| None).unwrap_err();
    assert!(err.contains("ANYLM_MODEL"), "{}", err);
    let err = config_from_env(|k| (k == "ANYLM_MODEL").then(|| "  ".to_string())).unwrap_err();
    assert!(err.contains("ANYLM_MODEL"), "{}", err);
}

#[test]
fn config_defaults() {
    let c = config_from_env(|k| (k == "ANYLM_MODEL").then(|| "qwen3.5-9b".to_string())).unwrap();
    assert_eq!(c.server_url, "http://localhost:1234");
    assert_eq!(c.context_length, 32768);
    assert_eq!(c.model, "qwen3.5-9b");
    assert_eq!(c.out_dir, PathBuf::from("../harness/runs/default"));
}

#[test]
fn config_reads_overrides() {
    let c = config_from_env(|k| match k {
        "ANYLM_MODEL" => Some("m".into()),
        "ANYLM_SERVER" => Some("http://127.0.0.1:5000".into()),
        "ANYLM_CONTEXT" => Some("8192".into()),
        "ANYLM_OUT" => Some("out".into()),
        _ => None,
    })
    .unwrap();
    assert_eq!((c.server_url.as_str(), c.context_length), ("http://127.0.0.1:5000", 8192));
    assert_eq!(c.out_dir, PathBuf::from("out"));
    assert!(config_from_env(|k| match k { "ANYLM_MODEL" => Some("m".into()), "ANYLM_CONTEXT" => Some("x".into()), _ => None }).is_err());
}

#[test]
fn scenario_reads_mcp_servers_and_skill_switch() {
    let sc: Scenario = serde_json::from_value(json!({
        "id": "s",
        "mode": "project",
        "skillAutoLoad": false,
        "mcpServers": [
            { "name": "Exa", "url": "https://mcp.exa.ai/mcp", "catalogId": "exa", "autoApprove": ["web_search_exa"] },
            { "name": "Local", "command": "npx", "args": ["-y", "pkg"], "env": { "KEY": "v" } }
        ]
    }))
    .unwrap();
    assert_eq!(sc.skill_auto_load, Some(false));
    let exa = sc.mcp_servers[0].config();
    assert_eq!((exa.transport.as_str(), exa.url.as_str(), exa.catalog_id.as_str()), ("http", "https://mcp.exa.ai/mcp", "exa"));
    assert_eq!(exa.auto_approve, vec!["web_search_exa".to_string()]);
    let local = sc.mcp_servers[1].config();
    assert_eq!((local.transport.as_str(), local.command.as_str()), ("stdio", "npx"));
    assert_eq!(local.env_keys, vec!["KEY".to_string()]);
}

#[test]
fn tool_called_contains_matches_prefixed_mcp_names() {
    let mut m = ChatMessage { role: "assistant".into(), content: "https://lmstudio.ai 참고".into(), ..Default::default() };
    m.tool_records.push(crate::store::ToolExecutionRecord { tool_name: "mcp__exa__web_search_exa".into(), status: "completed".into(), ..Default::default() });
    let turns = vec![TurnLog { prompt: "q".into(), error: None, message: Some(m), approvals: vec![], traces: vec![], wall_ms: 0 }];
    let check = |kind: &str, text: &str| run_check(&Check { kind: kind.into(), text: text.into(), ..Default::default() }, Path::new("."), &turns, "2026-01-02").0;
    assert!(check("toolCalledContains", "web_search_exa"));
    assert!(!check("toolCalled", "web_search_exa"), "toolCalled는 이름 전체가 같아야 한다");
    assert!(!check("toolCalledContains", "crawling_exa"));
    assert!(check("answerContains", "http"));
}

#[test]
fn tool_checks_ignore_the_app_skill_card_and_skill_auto_loaded_reads_it() {
    let mut m = ChatMessage { role: "assistant".into(), content: "완료".into(), ..Default::default() };
    m.tool_records.push(crate::store::ToolExecutionRecord {
        tool_name: "load_skill".into(),
        arguments_json: json!({ "name": "weekly-report", "reason": "implicit" }).to_string(),
        status: "completed".into(),
        automatic: true,
        ..Default::default()
    });
    let turns = vec![TurnLog { prompt: "q".into(), error: None, message: Some(m), approvals: vec![], traces: vec![], wall_ms: 0 }];
    let check = |kind: &str, text: &str| run_check(&Check { kind: kind.into(), text: text.into(), ..Default::default() }, Path::new("."), &turns, "2026-01-02").0;
    assert!(!check("toolCalled", "load_skill"), "앱이 넣은 카드는 모델이 부른 도구가 아니다");
    assert!(check("toolNotCalled", "load_skill"));
    assert!(check("skillAutoLoaded", "weekly-report"));
    assert!(!check("skillAutoLoaded", "pdf"));
}

#[test]
fn checks_expand_today_in_path_and_text() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("reports")).unwrap();
    std::fs::write(dir.path().join("reports/주간보고_2026-01-02.md"), "# 주간보고 (2026-01-02)").unwrap();
    let c = |kind: &str, path: &str, text: &str| Check { kind: kind.into(), path: path.into(), text: text.into(), ..Default::default() };
    let (ok, label) = run_check(&c("fileExists", "reports/주간보고_{today}.md", ""), dir.path(), &[], "2026-01-02");
    assert!(ok);
    assert_eq!(label, "fileExists reports/주간보고_2026-01-02.md");
    assert!(run_check(&c("fileContains", "reports/주간보고_{today}.md", "({today})"), dir.path(), &[], "2026-01-02").0);
    assert!(!run_check(&c("fileExists", "reports/주간보고_{today}.md", ""), dir.path(), &[], "2026-01-03").0);
    // 파일 이름의 *: 날짜가 달라도 양식은 따로 본다
    assert!(run_check(&c("fileContains", "reports/주간보고_*.md", "주간보고"), dir.path(), &[], "2026-01-03").0);
    assert!(!run_check(&c("fileExists", "reports/회의록_*.md", ""), dir.path(), &[], "2026-01-03").0);
    assert!(wildcard_match("a*b*c", "a12b34c") && !wildcard_match("a*b", "ab_c") && wildcard_match("*.md", "x.md"));
    assert!(!wildcard_match("ab*ba", "aba"), "앞뒤가 겹치면 안 된다");
}

#[tokio::test]
async fn scenario_mcp_servers_connect_through_the_app_manager_and_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let url = crate::mcp::tests::http_server(false).await;
    let mgr = mcp::McpManager::new(dir.path().join("mcp.json"), Box::new(mcp::MemorySecrets::default()));
    let servers = vec![HarnessMcp {
        name: "Remote".into(),
        url,
        headers: BTreeMap::from([("Authorization".to_string(), "Bearer tok".to_string())]),
        catalog_id: "exa".into(),
        auto_approve: vec!["search".into()],
        ..Default::default()
    }];
    let conn = connect_mcp_servers(&mgr, &servers).await;
    assert_eq!(conn.len(), 1);
    assert_eq!(conn[0].1["status"], "connected", "{}", conn[0].1);
    assert_eq!(conn[0].1["tools"], json!(["search"]));
    // 앱과 같은 도구 목록: mcp__<서버>__<도구>, 찾아보기 ID(내장 검색이면 [MCP] 절에 검색 안내 줄)
    let specs = mgr.tool_specs().await;
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].exposed, "mcp__remote__search");
    assert!(specs[0].auto_approve);
    assert!(mcp::is_builtin_search_catalog_id(&specs[0].catalog_id));
    let (text, is_err) = mgr.call(&specs[0].server_id, "search", &json!({ "q": "모델" })).await.unwrap();
    assert_eq!((text.as_str(), is_err), ("찾음: 모델", false));
    for id in conn.iter().filter_map(|(id, _)| id.as_ref()) {
        mgr.delete(id).unwrap();
    }
    assert!(mgr.tool_specs().await.is_empty());
    assert!(mgr.list().is_empty());
}
