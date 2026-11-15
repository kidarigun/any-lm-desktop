//! 실제 K-EXAONE API로 앱의 에이전트 루프를 그대로 돌려 보는 테스트 하니스.
//!
//! 앱과 같은 코드(`run_exaone_turn`, 도구 실행, 승인 처리)를 쓰고, 화면 대신 시나리오 파일이 사용자를 맡는다.
//! API 키는 앱처럼 Windows 자격 증명 관리자에서 읽으며 어디에도 출력하지 않는다.
//!
//! 실행 (src-tauri 폴더에서):
//!   cargo test --lib harness::live -- --ignored --nocapture
//!
//! 환경 변수:
//!   EXA_SCENARIOS   시나리오 JSON 파일 (기본: ../harness/scenarios.json)
//!   EXA_OUT         결과 폴더 (기본: ../harness/runs/default)
//!   EXA_ONLY        실행할 시나리오 id를 쉼표로 (없으면 전부)
//!   EXA_MAX_CALLS   이번 실행에서 쓸 API 호출 수 상한 (기본 20)
//!   EXA_DAY_RESERVE 하루 남은 호출 수가 이 값 아래로 내려가면 멈춘다 (기본 100)
//!   EXA_PAUSE_SEC   시나리오 사이에 쉬는 시간 (기본 5초)
//!   EXA_RERUN       1이면 이미 결과가 있는 시나리오도 다시 실행
//!
//! 이용약관상 부하 시험·같은 요청의 반복은 금지되어 있으므로, 한 번에 하나씩 천천히 실행한다.

use crate::exaone::{self, RoundTrace, TurnRequest, Tuning};
use crate::store::{AppSettings, ChatMessage, Project};
use crate::{doc_bridge, run_approved_tool, runtime, secrets, skills};
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
    /// fileExists | fileMissing | fileContains | fileNotContains | toolCalled | toolNotCalled
    /// | answerContains | answerNotContains | answerMinChars | answerMaxChars | noToolErrors | finished
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
    /// low | high | max (없으면 high)
    effort: Option<String>,
    tuning: Tuning,
    instructions: String,
    /// 프로젝트 폴더에 미리 만들어 둘 파일 (상대 경로 → 내용)
    files: BTreeMap<String, String>,
    /// 프로젝트 Skill (이름 → SKILL.md 내용)
    skills: BTreeMap<String, String>,
    /// 사용자가 차례로 보내는 메시지
    turns: Vec<String>,
    /// 승인을 요청받으면 허락할 도구 이름 (그 밖에는 거부)
    approve: Vec<String>,
    checks: Vec<Check>,
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

fn run_check(c: &Check, root: &Path, turns: &[TurnLog]) -> (bool, String) {
    let last = turns.iter().rev().find_map(|t| t.message.as_ref());
    let answer = last.map(|m| m.content.clone()).unwrap_or_default();
    let all_tools: Vec<&crate::store::ToolExecutionRecord> =
        turns.iter().filter_map(|t| t.message.as_ref()).flat_map(|m| m.tool_records.iter()).collect();
    let file = || std::fs::read_to_string(root.join(&c.path)).unwrap_or_default();
    let ok = match c.kind.as_str() {
        "fileExists" => root.join(&c.path).is_file(),
        "fileMissing" => !root.join(&c.path).exists(),
        "fileContains" => file().contains(&c.text),
        // text를 '|'로 나눈 것 중 하나라도 있으면 통과 (숫자 표기가 여러 가지일 때)
        "fileContainsAny" => c.text.split('|').any(|t| file().contains(t)),
        "answerContainsAny" => c.text.split('|').any(|t| answer.contains(t)),
        "fileMinChars" => file().chars().count() >= c.n,
        // 이 시나리오에서 쓴 API 호출 수가 n회 이하인지 (호출을 아끼는지 보는 검사)
        "maxCalls" => turns.iter().map(|t| t.traces.len()).sum::<usize>() <= c.n,
        "fileNotContains" => root.join(&c.path).is_file() && !file().contains(&c.text),
        "toolCalled" => all_tools.iter().any(|t| t.tool_name == c.text),
        "toolNotCalled" => !all_tools.iter().any(|t| t.tool_name == c.text),
        "answerContains" => answer.contains(&c.text),
        "answerNotContains" => !answer.contains(&c.text),
        "answerMinChars" => answer.chars().count() >= c.n,
        "answerMaxChars" => answer.chars().count() <= c.n,
        "noToolErrors" => !all_tools.iter().any(|t| t.status == "error"),
        "finished" => {
            turns.iter().all(|t| t.error.is_none())
                && !answer.contains("오류로 답변이 중간에 멈췄습니다")
                && !answer.contains("출력 길이 제한에 걸려")
                && !answer.contains("도구 호출 한도")
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

fn rate_value(t: &RoundTrace, key: &str) -> Option<i64> {
    t.rate_limits.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.trim().parse().ok())
}

/// 실행을 통째로 멈춰야 하는 오류인지 (키 문제, 한도 초과)
fn is_fatal(err: &str) -> bool {
    err.contains("API 키가") || err.contains("계속 혼잡") || err.contains("(401") || err.contains("(403")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "실제 K-EXAONE API를 호출한다 (사용 한도를 쓴다)"]
async fn live() {
    let scenarios_path = PathBuf::from(env_or("EXA_SCENARIOS", "../harness/scenarios.json"));
    let out_dir = PathBuf::from(env_or("EXA_OUT", "../harness/runs/default"));
    let max_calls = env_num("EXA_MAX_CALLS", 20) as usize;
    let day_reserve = env_num("EXA_DAY_RESERVE", 100) as i64;
    let pause = Duration::from_secs(env_num("EXA_PAUSE_SEC", 5));
    let rerun = env_or("EXA_RERUN", "0") == "1";
    let only: Vec<String> =
        env_or("EXA_ONLY", "").split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

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

    let api_key = secrets::load_api_key().expect("자격 증명 관리자를 열 수 없습니다").unwrap_or_default();
    assert!(!api_key.trim().is_empty(), "저장된 API 키가 없습니다. 앱의 [API 및 모델 설정]에서 먼저 입력하세요.");
    let base_settings = AppSettings { api_key, api_key_set: true, ..AppSettings::default() };

    // 앱이 설치해 둔 Python을 쓰고, 사용자 Skill은 섞이지 않게 빈 폴더를 쓴다.
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        runtime::set_root(Path::new(&local).join("io.github.kidarigun.kexaone-desktop").join("runtimes"));
    }
    skills::set_user_dir(out_dir.join("user-skills"));

    let app = mock_app_with_converter();
    exaone::trace_start();
    let mut calls_used = 0usize;
    let mut stop_reason: Option<String> = None;

    for sc in &scenarios {
        if !only.is_empty() && !only.contains(&sc.id) {
            continue;
        }
        if !rerun && done_ids.contains(&sc.id) {
            println!("[건너뜀] {} (이미 결과 있음)", sc.id);
            continue;
        }
        if calls_used >= max_calls {
            stop_reason = Some(format!("호출 상한 {}회에 도달", max_calls));
            break;
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
        let project = (sc.mode == "project").then(|| Project {
            id: format!("harness-{}", sc.id),
            name: format!("시험 프로젝트 {}", sc.id),
            // 앱과 같은 모양(\\?\ 없는 경로)으로 준다.
            root_path: work.to_string_lossy().trim_start_matches(r"\\?\").to_string(),
            instructions: sc.instructions.clone(),
            created_at: chrono::Local::now().to_rfc3339(),
            auto_approve: Vec::new(),
        });
        let root = project.as_ref().map(|p| PathBuf::from(&p.root_path)).unwrap_or_else(|| work.clone());

        let mut settings = base_settings.clone();
        settings.reasoning_effort = sc.effort.clone().unwrap_or_else(|| "high".to_string());
        exaone::set_tuning(sc.tuning.clone());

        println!("\n=== {} [{} / 추론 {}] {}", sc.id, sc.mode, settings.reasoning_effort, sc.note);
        let started_at = chrono::Local::now().to_rfc3339();
        let sc_start = Instant::now();
        let mut history: Vec<ChatMessage> = Vec::new();
        let mut turns: Vec<TurnLog> = Vec::new();
        let mut fatal = false;

        'turns: for prompt in &sc.turns {
            if calls_used >= max_calls {
                stop_reason = Some(format!("호출 상한 {}회에 도달", max_calls));
                break;
            }
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

            // 승인 대기로 멈추면 승인/거부를 처리하고 같은 메시지를 이어 간다 (앱의 승인 흐름과 같다).
            for _ in 0..12 {
                let res = exaone::run_exaone_turn(
                    app.handle(),
                    TurnRequest {
                        settings: &settings,
                        project: project.as_ref(),
                        conversation_id: &sc.id,
                        history: &history,
                        continue_from: continue_from.take(),
                        mode: &sc.mode,
                        checkpoint: None,
                    },
                    Arc::new(AtomicBool::new(false)),
                )
                .await;
                let traces = exaone::trace_take();
                calls_used += traces.len();
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
                    let allowed = sc.approve.contains(&tool_name);
                    let (status, output, produced) = if !allowed {
                        ("rejected".to_string(), "사용자가 실행을 거부했습니다.".to_string(), Vec::new())
                    } else if let Some(why) = &blocked {
                        ("rejected".to_string(), format!("사용자가 실행을 거부했습니다. ({})", why), Vec::new())
                    } else {
                        let run = run_approved_tool(app.handle(), project.as_ref(), &tool_id, &tool_name, &args, false).await;
                        (run.status.to_string(), run.output, run.produced)
                    };
                    println!("  · 승인 요청 {} → {}", tool_name, status);
                    log.approvals.push(json!({ "tool": tool_name, "args": args, "allowed": allowed, "blocked": blocked, "status": status }));
                    let r = &mut msg.tool_records[i];
                    r.status = status;
                    r.output = output;
                    r.produced_files = produced;
                }
                history.push(msg.clone());
                log.message = Some(msg.clone());
                continue_from = Some(msg);
                if calls_used >= max_calls {
                    stop_reason = Some(format!("호출 상한 {}회에 도달", max_calls));
                    break;
                }
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
                    usage_num(t, "reasoning_tokens"),
                    t.error.as_deref().map(|e| format!("오류: {}", e.chars().take(120).collect::<String>())).unwrap_or_default(),
                );
            }
            let day_left = log.traces.iter().rev().find_map(|t| rate_value(t, "x-day-rate-limit-remaining"));
            let stop_turns = log.error.is_some();
            turns.push(log);
            if let Some(left) = day_left {
                if left < day_reserve {
                    stop_reason = Some(format!("하루 남은 호출 수 {}회 (예비 {}회 아래)", left, day_reserve));
                    break 'turns;
                }
            }
            if stop_turns || stop_reason.is_some() {
                break;
            }
        }

        let checks: Vec<Value> = sc
            .checks
            .iter()
            .map(|c| {
                let (ok, label) = run_check(c, &root, &turns);
                println!("  [{}] {}", if ok { "통과" } else { "실패" }, label);
                json!({ "ok": ok, "check": label })
            })
            .collect();
        let all_traces: Vec<&RoundTrace> = turns.iter().flat_map(|t| t.traces.iter()).collect();
        let last_rate: Vec<(String, String)> = all_traces.last().map(|t| t.rate_limits.clone()).unwrap_or_default();
        let incomplete = turns.len() < sc.turns.len();
        let summary = json!({
            "id": sc.id,
            "startedAt": started_at,
            "mode": sc.mode,
            "effort": settings.reasoning_effort,
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
            "reasoningTokens": all_traces.iter().map(|t| usage_num(t, "reasoning_tokens")).sum::<u64>(),
            "reasoningChars": all_traces.iter().map(|t| t.reasoning_chars).sum::<usize>(),
            "contentChars": all_traces.iter().map(|t| t.content_chars).sum::<usize>(),
            "maxGapMs": all_traces.iter().map(|t| t.max_gap_ms).max().unwrap_or(0),
            "checksPassed": checks.iter().filter(|c| c["ok"] == true).count(),
            "checksTotal": checks.len(),
            "checks": checks,
            "rateLimits": last_rate,
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
            "  → 호출 {}회, {}초, 검사 {}/{} | 한도 {:?}",
            summary["apiCalls"],
            sc_start.elapsed().as_secs(),
            summary["checksPassed"],
            summary["checksTotal"],
            summary["rateLimits"]
        );

        if fatal {
            stop_reason = Some("키 또는 한도 문제로 중단".to_string());
        }
        if stop_reason.is_some() {
            break;
        }
        tokio::time::sleep(pause).await;
    }

    println!("\n이번 실행의 API 호출: {}회", calls_used);
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
