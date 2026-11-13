use crate::doc_bridge::{self, DocKind};
use crate::fs_ops;
use crate::store::{AppSettings, ChatMessage, FileSnapshot, Project, ToolExecutionRecord};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};
use uuid::Uuid;

/// 한 번의 사용자 요청에서 모델이 도구를 연속 호출할 수 있는 최대 라운드 수.
pub const MAX_TOOL_ROUNDS: usize = 10;
/// 모델에게 되돌려 주는 도구 출력의 최대 길이(문자).
pub const MAX_TOOL_OUTPUT_CHARS: usize = 40_000;

pub const STREAM_EVENT: &str = "exaone-stream";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// 요청 후 응답 헤더가 올 때까지 기다리는 최대 시간.
const RESPONSE_HEADER_TIMEOUT: Duration = Duration::from_secs(180);
/// 스트리밍 도중 아무 데이터도 오지 않을 때 허용하는 최대 시간.
/// (전체 응답 시간에는 제한을 두지 않는다. 긴 추론도 끊기지 않게 하기 위함.)
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const CANCEL_POLL: Duration = Duration::from_millis(200);
const RETRY_DELAYS_SEC: [u64; 5] = [2, 3, 5, 8, 12];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunkPayload {
    pub conversation_id: String,
    pub message_id: String,
    pub delta_reasoning: Option<String>,
    pub delta_content: Option<String>,
    pub tool_record: Option<ToolExecutionRecord>,
    pub snapshot: Option<FileSnapshot>,
    pub status_notice: Option<String>,
    pub done: bool,
    pub error: Option<String>,
}

pub fn build_tools_schema() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "list_directory",
                "description": "프로젝트 로컬 폴더의 파일 및 하위 폴더 목록을 조회합니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "relative_path": {
                            "type": "string",
                            "description": "조회할 상대 경로 (루트는 빈 문자열 '' 또는 '.')"
                        }
                    },
                    "required": []
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "프로젝트 폴더 내의 로컬 파일 내용을 읽습니다. PDF, Word(.docx), PowerPoint(.pptx)는 Markdown 텍스트로 변환해 돌려줍니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "relative_path": {
                            "type": "string",
                            "description": "읽을 파일의 프로젝트 내 상대 경로 (예: 'docs/report.md')"
                        }
                    },
                    "required": ["relative_path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "프로젝트 폴더 내에 새 파일을 작성하거나 전체 내용을 덮어씁니다. 경로가 .docx 또는 .pptx이면 content(Markdown)를 Word/PowerPoint 문서로 변환해 저장합니다. PowerPoint는 '---' 줄 또는 '## 제목'으로 슬라이드를 나누고, '노트:'로 시작하는 문단은 발표자 노트가 됩니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "relative_path": {
                            "type": "string",
                            "description": "저장할 파일 상대 경로"
                        },
                        "content": {
                            "type": "string",
                            "description": "파일에 기록할 전체 내용"
                        }
                    },
                    "required": ["relative_path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "modify_file",
                "description": "기존 로컬 파일에서 특정 구간(target)을 찾아 새로운 내용(replacement)으로 부분 수정합니다. .docx/.pptx는 원래 서식을 유지한 채 한 문단 안의 문구를 교체합니다(Markdown 기호 없이 문서의 실제 문구를 지정).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "relative_path": {
                            "type": "string",
                            "description": "수정할 파일 상대 경로"
                        },
                        "target": {
                            "type": "string",
                            "description": "교체할 기존 원문 문자열 (파일 내용과 정확히 일치해야 함)"
                        },
                        "replacement": {
                            "type": "string",
                            "description": "새로 대체할 문자열"
                        }
                    },
                    "required": ["relative_path", "target", "replacement"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search_files",
                "description": "프로젝트 폴더 내 모든 파일에서 특정 키워드를 검색합니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "검색할 키워드"
                        }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "delete_path",
                "description": "프로젝트 내 파일 또는 폴더를 삭제합니다 (사용자 승인 후 실행됨).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "relative_path": {
                            "type": "string",
                            "description": "삭제할 파일 또는 폴더의 상대 경로"
                        }
                    },
                    "required": ["relative_path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "프로젝트 루트 경로에서 PowerShell 명령어를 실행합니다 (사용자 승인 후 실행됨, 최대 120초).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "실행할 명령어"
                        }
                    },
                    "required": ["command"]
                }
            }
        }
    ])
}

// ---------------------------------------------------------------------------
// SSE 스트림 파싱
// ---------------------------------------------------------------------------

/// 바이트 스트림을 줄 단위로 끊어 준다. 줄이 완성된 뒤에만 UTF-8로 디코딩하므로
/// 청크 경계에서 한글(멀티바이트) 문자가 잘려도 깨지지 않는다.
#[derive(Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line[..line.len() - 1]);
            lines.push(text.trim_end_matches('\r').to_string());
        }
        lines
    }

    /// 스트림이 개행 없이 끝났을 때 남은 마지막 줄을 꺼낸다.
    pub fn finish(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let rest = std::mem::take(&mut self.buf);
        Some(String::from_utf8_lossy(&rest).trim_end_matches('\r').to_string())
    }
}

#[derive(Debug, PartialEq)]
pub enum SseEvent {
    Data(Value),
    Done,
    Skip,
}

pub fn parse_sse_line(line: &str) -> SseEvent {
    let Some(rest) = line.trim().strip_prefix("data:") else {
        return SseEvent::Skip;
    };
    let data = rest.trim();
    if data.is_empty() {
        SseEvent::Skip
    } else if data == "[DONE]" {
        SseEvent::Done
    } else {
        serde_json::from_str(data)
            .map(SseEvent::Data)
            .unwrap_or(SseEvent::Skip)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct DeltaOut {
    pub reasoning: Option<String>,
    pub content: Option<String>,
}

/// 한 라운드(한 번의 API 호출) 동안 받은 텍스트, 추론, 도구 호출을 모은다.
#[derive(Debug, Default)]
pub struct RoundAccumulator {
    pub content: String,
    pub reasoning: String,
    pub tools: Vec<PendingToolCall>,
}

impl RoundAccumulator {
    pub fn apply_chunk(&mut self, val: &Value) -> Result<DeltaOut, String> {
        if let Some(err) = val.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| err.to_string());
            return Err(format!("EXAONE API 스트림 오류: {}", msg));
        }

        let mut out = DeltaOut::default();
        let Some(delta) = val
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"))
        else {
            return Ok(out);
        };

        let reasoning = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if let Some(r) = reasoning {
            self.reasoning.push_str(r);
            out.reasoning = Some(r.to_string());
        }

        if let Some(c) = delta
            .get("content")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            self.content.push_str(c);
            out.content = Some(c.to_string());
        }

        if let Some(tc_arr) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tc_arr {
                let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                while self.tools.len() <= idx {
                    self.tools.push(PendingToolCall::default());
                }
                let slot = &mut self.tools[idx];
                if let Some(id) = tc.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    slot.id = id.to_string();
                }
                if let Some(func) = tc.get("function") {
                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                        // 일부 서버는 이름을 매 청크 반복해 보내므로 같은 이름은 다시 붙이지 않는다.
                        if slot.name.is_empty() {
                            slot.name = name.to_string();
                        } else if slot.name != name {
                            slot.name.push_str(name);
                        }
                    }
                    if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                        slot.arguments.push_str(args);
                    }
                }
            }
        }

        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// 대화 기록 → API 메시지 변환
// ---------------------------------------------------------------------------

/// 어시스턴트 메시지를 라운드별 (텍스트, 도구 기록) 묶음으로 나눈다.
pub fn message_rounds(msg: &ChatMessage) -> Vec<(String, Vec<&ToolExecutionRecord>)> {
    if msg.round_contents.is_empty() {
        // 라운드 정보가 없는 이전 버전 메시지: 도구 호출을 먼저, 최종 답변을 나중에 둔다.
        let mut rounds = Vec::new();
        if !msg.tool_records.is_empty() {
            rounds.push((String::new(), msg.tool_records.iter().collect()));
        }
        if !msg.content.is_empty() {
            rounds.push((msg.content.clone(), Vec::new()));
        }
        return rounds;
    }

    let tool_rounds = msg
        .tool_records
        .iter()
        .map(|r| r.round as usize + 1)
        .max()
        .unwrap_or(0);
    let n = msg.round_contents.len().max(tool_rounds);
    (0..n)
        .map(|r| {
            (
                msg.round_contents.get(r).cloned().unwrap_or_default(),
                msg.tool_records
                    .iter()
                    .filter(|t| t.round as usize == r)
                    .collect(),
            )
        })
        .collect()
}

pub fn tool_result_for_model(rec: &ToolExecutionRecord) -> String {
    let text = match rec.status.as_str() {
        "pending_approval" => {
            "사용자가 이 작업을 승인하지 않아 실행되지 않았습니다.".to_string()
        }
        "rejected" => format!(
            "{} 이 작업은 실행되지 않았습니다. 필요하면 다른 방법을 제안하세요.",
            rec.output
        ),
        "error" => format!("오류: {}", rec.output),
        _ => rec.output.clone(),
    };
    fs_ops::truncate_for_model(&text, MAX_TOOL_OUTPUT_CHARS)
}

fn tool_call_json(rec: &ToolExecutionRecord) -> Value {
    // 잘못된 JSON 인자를 그대로 돌려보내면 서버가 요청 전체를 거부할 수 있다.
    let arguments = if serde_json::from_str::<Value>(&rec.arguments_json).is_ok() {
        rec.arguments_json.clone()
    } else {
        "{}".to_string()
    };
    json!({
        "id": rec.id,
        "type": "function",
        "function": { "name": rec.tool_name, "arguments": arguments }
    })
}

fn push_tool_round(out: &mut Vec<Value>, content: &str, records: &[&ToolExecutionRecord]) {
    out.push(json!({
        "role": "assistant",
        "content": if content.is_empty() { Value::Null } else { json!(content) },
        "tool_calls": records.iter().map(|r| tool_call_json(r)).collect::<Vec<_>>()
    }));
    for r in records {
        out.push(json!({
            "role": "tool",
            "tool_call_id": r.id,
            "content": tool_result_for_model(r)
        }));
    }
}

/// 저장된 대화 기록을 OpenAI 호환 메시지 배열로 바꾼다.
/// `include_tools`가 참이면 도구 호출과 결과까지 그대로 재현해, 모델이 이전 작업 맥락을 이어받게 한다.
pub fn build_api_messages(sys_prompt: &str, history: &[ChatMessage], include_tools: bool) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": sys_prompt })];
    for msg in history {
        match msg.role.as_str() {
            "user" => out.push(json!({ "role": "user", "content": msg.content })),
            "assistant" => {
                if !include_tools || msg.tool_records.is_empty() {
                    let content = if msg.content.is_empty() {
                        "(응답 없음)"
                    } else {
                        msg.content.as_str()
                    };
                    out.push(json!({ "role": "assistant", "content": content }));
                    continue;
                }
                for (content, records) in message_rounds(msg) {
                    if records.is_empty() {
                        if !content.is_empty() {
                            out.push(json!({ "role": "assistant", "content": content }));
                        }
                    } else {
                        push_tool_round(&mut out, &content, &records);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn build_system_prompt(project: Option<&Project>) -> String {
    match project {
        Some(p) => format!(
            "당신은 LG AI연구원의 K-EXAONE 2.5 기반 데스크톱 어시스턴트입니다.\n\
현재 프로젝트: {} (로컬 경로: {})\n\
프로젝트 지침: {}\n\n\
다음 도구로 프로젝트 폴더를 직접 다룰 수 있습니다: 읽기(read_file, list_directory, search_files), 쓰기(write_file), 수정(modify_file), 삭제(delete_path), 명령어 실행(run_command).\n\
- 모든 경로는 프로젝트 루트 기준 상대 경로로 지정하세요.\n\
- modify_file의 target은 파일 내용과 정확히 일치해야 합니다. 확실하지 않으면 먼저 read_file로 확인하세요.\n\
- delete_path와 run_command는 사용자가 승인해야 실행됩니다. 거부되면 다른 방법을 제안하세요.\n\
- 명령은 Windows PowerShell에서 실행됩니다.\n\
- 보고서·문서를 만들 때 사용자가 Word나 PowerPoint를 원하면 write_file 경로를 .docx 또는 .pptx로 지정하고 content는 Markdown으로 작성하세요. 결과는 사용자의 문서 캔버스에서 바로 열립니다.\n\
- 기존 .docx/.pptx의 일부 문구만 고칠 때는 modify_file을 쓰면 원래 서식이 유지됩니다. 전체를 다시 쓰면 서식이 단순해집니다.",
            p.name, p.root_path, p.instructions
        ),
        None => "당신은 LG AI연구원의 K-EXAONE 2.5 기반 데스크톱 어시스턴트입니다. 한국어로 명확하고 도움이 되는 답변을 작성하세요.".to_string(),
    }
}

// ---------------------------------------------------------------------------
// 에이전트 루프
// ---------------------------------------------------------------------------

pub struct TurnRequest<'a> {
    pub settings: &'a AppSettings,
    /// `Some`이면 프로젝트 도구가 켜진다. 일반 채팅은 `None`.
    pub project: Option<&'a Project>,
    pub conversation_id: &'a str,
    /// 전체 대화 기록. `continue_from`이 있으면 그 메시지도 마지막에 포함되어 있어야 한다.
    pub history: &'a [ChatMessage],
    /// 승인 대기로 멈췄던 어시스턴트 메시지. 주어지면 새 메시지를 만들지 않고 이어서 진행한다.
    pub continue_from: Option<ChatMessage>,
}

struct StreamSink<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    conversation_id: String,
    message_id: String,
}

impl<R: Runtime> StreamSink<'_, R> {
    fn emit(&self, fill: impl FnOnce(&mut StreamChunkPayload)) {
        let mut p = StreamChunkPayload {
            conversation_id: self.conversation_id.clone(),
            message_id: self.message_id.clone(),
            ..Default::default()
        };
        fill(&mut p);
        let _ = self.app.emit(STREAM_EVENT, p);
    }
    fn reasoning(&self, s: &str) {
        self.emit(|p| p.delta_reasoning = Some(s.to_string()));
    }
    fn content(&self, s: &str) {
        self.emit(|p| p.delta_content = Some(s.to_string()));
    }
    fn notice(&self, s: String) {
        self.emit(|p| p.status_notice = Some(s));
    }
    fn tool(&self, rec: ToolExecutionRecord, snap: Option<FileSnapshot>) {
        self.emit(|p| {
            p.tool_record = Some(rec);
            p.snapshot = snap;
        });
    }
    fn done(&self) {
        self.emit(|p| p.done = true);
    }
}

enum Raced<T> {
    Done(T),
    Cancelled,
    TimedOut,
}

/// `fut`을 기다리되, 취소 플래그가 켜지거나 `limit`이 지나면 즉시 빠져나온다.
async fn race_cancel<F: Future>(fut: F, flag: &AtomicBool, limit: Duration) -> Raced<F::Output> {
    tokio::pin!(fut);
    let start = Instant::now();
    loop {
        tokio::select! {
            out = &mut fut => return Raced::Done(out),
            _ = tokio::time::sleep(CANCEL_POLL) => {
                if flag.load(Ordering::Relaxed) {
                    return Raced::Cancelled;
                }
                if start.elapsed() >= limit {
                    return Raced::TimedOut;
                }
            }
        }
    }
}

/// 요청을 보내고 429/503이면 잠시 기다렸다 재시도한다. 취소되면 `Ok(None)`.
async fn send_with_retry<R: Runtime>(
    client: &reqwest::Client,
    settings: &AppSettings,
    body: &Value,
    sink: &StreamSink<'_, R>,
    cancel: &AtomicBool,
) -> Result<Option<reqwest::Response>, String> {
    for attempt in 0..=RETRY_DELAYS_SEC.len() {
        let req = client
            .post(&settings.base_url)
            .bearer_auth(settings.api_key.trim())
            .json(body)
            .send();

        let resp = match race_cancel(req, cancel, RESPONSE_HEADER_TIMEOUT).await {
            Raced::Cancelled => return Ok(None),
            Raced::TimedOut => {
                return Err(format!(
                    "EXAONE API 서버가 {}초 동안 응답하지 않습니다. 잠시 후 다시 시도해 주세요.",
                    RESPONSE_HEADER_TIMEOUT.as_secs()
                ))
            }
            Raced::Done(Err(e)) => return Err(format!("EXAONE API 네트워크 연결 오류: {}", e)),
            Raced::Done(Ok(r)) => r,
        };

        let status = resp.status();
        if status.is_success() {
            sink.notice(String::new());
            return Ok(Some(resp));
        }

        let code = status.as_u16();
        let err_text: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(800)
            .collect();

        if (code == 429 || code == 503) && attempt < RETRY_DELAYS_SEC.len() {
            let wait = RETRY_DELAYS_SEC[attempt];
            sink.notice(format!(
                "현재 EXAONE API 서버 동시 접속자가 많아 자동 재시도 중입니다... ({}/{}회, {}초 대기)",
                attempt + 1,
                RETRY_DELAYS_SEC.len(),
                wait
            ));
            let sleep = tokio::time::sleep(Duration::from_secs(wait));
            if let Raced::Cancelled = race_cancel(sleep, cancel, Duration::from_secs(wait + 5)).await {
                return Ok(None);
            }
            continue;
        }

        if code == 401 || code == 403 {
            return Err(format!(
                "EXAONE API 키가 올바르지 않거나 권한이 없습니다 ({}). [API 설정]에서 API 키를 확인해 주세요. {}",
                status, err_text
            ));
        }
        return Err(format!("EXAONE API 응답 오류 ({}): {}", status, err_text));
    }
    Err("EXAONE API 서버가 계속 혼잡합니다. 잠시 후 다시 시도해 주세요.".to_string())
}

/// 한 라운드의 스트림 줄을 처리하고 화면으로 델타를 내보낸다.
struct RoundStreamer {
    acc: RoundAccumulator,
    /// 앞 라운드에 이미 텍스트가 있으면 화면에 문단 구분을 한 번 넣는다.
    needs_separator: bool,
}

impl RoundStreamer {
    /// `Ok(true)`면 `[DONE]`을 받은 것.
    fn handle_line<R: Runtime>(&mut self, line: &str, sink: &StreamSink<'_, R>) -> Result<bool, String> {
        match parse_sse_line(line) {
            SseEvent::Done => Ok(true),
            SseEvent::Skip => Ok(false),
            SseEvent::Data(val) => {
                let out = self.acc.apply_chunk(&val)?;
                if let Some(r) = out.reasoning {
                    sink.reasoning(&r);
                }
                if let Some(c) = out.content {
                    if self.needs_separator {
                        sink.content("\n\n");
                        self.needs_separator = false;
                    }
                    sink.content(&c);
                }
                Ok(false)
            }
        }
    }
}

pub async fn run_exaone_turn<R: Runtime>(
    app: &AppHandle<R>,
    req: TurnRequest<'_>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<(ChatMessage, Vec<FileSnapshot>), String> {
    let settings = req.settings;
    if settings.api_key.trim().is_empty() {
        return Err("EXAONE API 키가 설정되지 않았습니다. 좌측 하단 [API 설정] 버튼을 눌러 API Key를 입력해 주세요.".to_string());
    }

    let tools_enabled = req.project.is_some();
    let sys_prompt = build_system_prompt(req.project);
    let mut api_messages = build_api_messages(&sys_prompt, req.history, tools_enabled);

    let mut msg = req.continue_from.unwrap_or_else(|| ChatMessage {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: String::new(),
        reasoning_content: None,
        tool_records: Vec::new(),
        created_at: chrono::Local::now().to_rfc3339(),
        round_contents: Vec::new(),
    });
    if msg.round_contents.is_empty() && !msg.content.is_empty() {
        msg.round_contents.push(msg.content.clone());
    }
    let start_round = msg
        .tool_records
        .iter()
        .map(|r| r.round as usize + 1)
        .max()
        .unwrap_or(0)
        .max(msg.round_contents.len());
    msg.round_contents.resize(start_round, String::new());

    let sink = StreamSink {
        app,
        conversation_id: req.conversation_id.to_string(),
        message_id: msg.id.clone(),
    };

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| format!("HTTP 클라이언트 생성 실패: {}", e))?;

    let mut new_snapshots: Vec<FileSnapshot> = Vec::new();
    let mut cancelled = false;
    let mut hit_round_limit = false;

    for local_round in 0..MAX_TOOL_ROUNDS {
        let round = start_round + local_round;
        if cancel_flag.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let mut body = json!({
            "model": settings.model,
            "messages": api_messages,
            "stream": true
        });
        if tools_enabled {
            body["tools"] = build_tools_schema();
            body["tool_choice"] = json!("auto");
        }
        if !settings.reasoning_effort.is_empty() {
            body["reasoning_effort"] = json!(settings.reasoning_effort);
        }

        let Some(response) = send_with_retry(&client, settings, &body, &sink, &cancel_flag).await? else {
            cancelled = true;
            break;
        };

        let mut streamer = RoundStreamer {
            acc: RoundAccumulator::default(),
            needs_separator: msg.round_contents.iter().any(|c| !c.is_empty()),
        };
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut finished = false;

        while !finished {
            match race_cancel(stream.next(), &cancel_flag, STREAM_IDLE_TIMEOUT).await {
                Raced::Cancelled => {
                    cancelled = true;
                    break;
                }
                Raced::TimedOut => {
                    return Err(format!(
                        "EXAONE API 응답이 {}초 동안 멈춰 있어 중단했습니다.",
                        STREAM_IDLE_TIMEOUT.as_secs()
                    ))
                }
                Raced::Done(None) => break,
                Raced::Done(Some(Err(e))) => return Err(format!("스트림 수신 오류: {}", e)),
                Raced::Done(Some(Ok(bytes))) => {
                    for line in decoder.push(&bytes) {
                        if streamer.handle_line(&line, &sink)? {
                            finished = true;
                            break;
                        }
                    }
                }
            }
        }
        if !cancelled && !finished {
            if let Some(line) = decoder.finish() {
                streamer.handle_line(&line, &sink)?;
            }
        }

        let acc = streamer.acc;
        if !acc.reasoning.is_empty() {
            let merged = match msg.reasoning_content.take() {
                Some(prev) if !prev.is_empty() => format!("{}\n\n{}", prev, acc.reasoning),
                _ => acc.reasoning.clone(),
            };
            msg.reasoning_content = Some(merged);
        }
        msg.round_contents.push(acc.content.clone());

        // 취소되면 끝까지 받지 못한 도구 호출은 실행하지 않는다.
        if cancelled {
            break;
        }

        let calls: Vec<PendingToolCall> =
            acc.tools.into_iter().filter(|t| !t.name.is_empty()).collect();
        if calls.is_empty() {
            break;
        }
        let Some(project) = req.project else {
            // 도구를 켜지 않았는데 도구 호출이 온 경우: 텍스트 답변만 남긴다.
            break;
        };
        let root = Path::new(&project.root_path);

        let mut round_records: Vec<ToolExecutionRecord> = Vec::new();
        let mut paused_for_approval = false;
        for call in calls {
            let mut call_id = if call.id.is_empty() {
                format!("call_{}", Uuid::new_v4().simple())
            } else {
                call.id.clone()
            };
            // 라운드마다 같은 id를 재사용하는 서버가 있어, 메시지 안에서 id가 겹치지 않게 한다.
            if msg.tool_records.iter().chain(round_records.iter()).any(|r| r.id == call_id) {
                call_id = format!("{}_{}", call_id, &Uuid::new_v4().simple().to_string()[..8]);
            }

            let (mut record, snap, needs_approval) =
                match serde_json::from_str::<Value>(&call.arguments) {
                    Ok(args) => {
                        execute_or_queue_tool(app, root, &project.id, &call_id, &call.name, &args).await
                    }
                    Err(e) => (
                        tool_record(
                            &call_id,
                            &call.name,
                            call.arguments.clone(),
                            "error",
                            format!("도구 인자가 올바른 JSON이 아닙니다 ({}). 인자를 다시 확인해 호출하세요.", e),
                            None,
                            None,
                        ),
                        None,
                        false,
                    ),
                };
            record.round = round as u32;
            if let Some(s) = &snap {
                new_snapshots.push(s.clone());
            }
            sink.tool(record.clone(), snap);
            paused_for_approval |= needs_approval;
            round_records.push(record);
        }

        let refs: Vec<&ToolExecutionRecord> = round_records.iter().collect();
        push_tool_round(&mut api_messages, &acc.content, &refs);
        msg.tool_records.extend(round_records);

        if paused_for_approval {
            break;
        }
        if local_round + 1 == MAX_TOOL_ROUNDS {
            hit_round_limit = true;
        }
    }

    if hit_round_limit {
        let note = format!(
            "_(도구 호출 한도({}회)에 도달해 멈췄습니다. 이어서 진행하려면 \"계속\"이라고 입력하세요.)_",
            MAX_TOOL_ROUNDS
        );
        sink.content(&format!("\n\n{}", note));
        msg.round_contents.push(note);
    }
    if cancelled {
        let note = "_(사용자가 생성을 중지했습니다.)_";
        sink.content(&format!("\n\n{}", note));
        if let Some(last) = msg.round_contents.last_mut() {
            if last.is_empty() {
                last.push_str(note);
            } else {
                last.push_str(&format!("\n\n{}", note));
            }
        } else {
            msg.round_contents.push(note.to_string());
        }
    }

    msg.content = msg
        .round_contents
        .iter()
        .filter(|c| !c.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");

    sink.done();
    Ok((msg, new_snapshots))
}

// ---------------------------------------------------------------------------
// 도구 실행
// ---------------------------------------------------------------------------

fn tool_record(
    call_id: &str,
    name: &str,
    arguments_json: String,
    status: &str,
    output: String,
    file_path: Option<String>,
    snapshot_id: Option<String>,
) -> ToolExecutionRecord {
    ToolExecutionRecord {
        id: call_id.to_string(),
        tool_name: name.to_string(),
        arguments_json,
        status: status.to_string(),
        output,
        file_path,
        snapshot_id,
        round: 0,
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// 파일 쓰기/수정 결과를 도구 기록으로 만든다.
fn snapshot_result(
    call_id: &str,
    name: &str,
    args_json: &str,
    rel: &str,
    res: Result<FileSnapshot, String>,
    success_msg: String,
) -> (ToolExecutionRecord, Option<FileSnapshot>, bool) {
    let file_path = Some(rel.replace('\\', "/"));
    match res {
        Ok(snap) => (
            tool_record(
                call_id,
                name,
                args_json.to_string(),
                "completed",
                success_msg,
                file_path,
                Some(snap.id.clone()),
            ),
            Some(snap),
            false,
        ),
        Err(err) => (
            tool_record(call_id, name, args_json.to_string(), "error", err, file_path, None),
            None,
            false,
        ),
    }
}

fn file_title(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "문서".to_string())
}

/// 읽기·쓰기·수정 도구는 즉시 실행하고, 삭제·명령 실행은 승인 대기로 돌려준다.
/// `.docx`/`.pptx`/`.pdf`는 웹뷰의 문서 변환기를 거쳐 Markdown으로 읽고 쓴다.
/// 반환값: (기록, 스냅샷, 승인 필요 여부)
pub async fn execute_or_queue_tool<R: Runtime>(
    app: &AppHandle<R>,
    root: &Path,
    project_id: &str,
    call_id: &str,
    name: &str,
    args: &Value,
) -> (ToolExecutionRecord, Option<FileSnapshot>, bool) {
    let args_json = serde_json::to_string(args).unwrap_or_default();

    if matches!(name, "read_file" | "write_file" | "modify_file") {
        let rel = str_arg(args, "relative_path");
        if let Some(kind) = DocKind::from_path(rel) {
            return document_tool(app, root, project_id, call_id, name, args, &args_json, rel, kind).await;
        }
    }

    let done = |output: String, file_path: Option<String>| {
        (
            tool_record(call_id, name, args_json.clone(), "completed", output, file_path, None),
            None,
            false,
        )
    };
    let fail = |output: String, file_path: Option<String>| {
        (
            tool_record(call_id, name, args_json.clone(), "error", output, file_path, None),
            None,
            false,
        )
    };

    match name {
        "list_directory" => {
            let rel = str_arg(args, "relative_path");
            let res = fs_ops::resolve_safe_path(root, rel)
                .and_then(|target| fs_ops::list_project_subtree(root, &target, 2))
                .map(|nodes| serde_json::to_string_pretty(&nodes).unwrap_or_default());
            match res {
                Ok(out) => done(out, None),
                Err(err) => fail(err, None),
            }
        }
        "read_file" => {
            let rel = str_arg(args, "relative_path");
            match fs_ops::read_file_content(root, rel) {
                Ok(content) => done(content, Some(rel.replace('\\', "/"))),
                Err(err) => fail(err, Some(rel.to_string())),
            }
        }
        "write_file" => {
            let rel = str_arg(args, "relative_path");
            if rel.trim().is_empty() {
                return fail("relative_path가 비어 있습니다.".to_string(), None);
            }
            let content = str_arg(args, "content");
            match fs_ops::write_file_with_snapshot(root, project_id, rel, content) {
                Ok(snap) => {
                    let rec = tool_record(
                        call_id,
                        name,
                        args_json.clone(),
                        "completed",
                        format!("파일 작성 완료: {} ({} bytes)", rel, content.len()),
                        Some(rel.replace('\\', "/")),
                        Some(snap.id.clone()),
                    );
                    (rec, Some(snap), false)
                }
                Err(err) => fail(err, Some(rel.to_string())),
            }
        }
        "modify_file" => {
            let rel = str_arg(args, "relative_path");
            let target = str_arg(args, "target");
            if target.is_empty() {
                return fail(
                    "target이 비어 있습니다. 교체할 원문을 정확히 지정하세요.".to_string(),
                    Some(rel.to_string()),
                );
            }
            let replacement = str_arg(args, "replacement");
            match fs_ops::modify_file_with_snapshot(root, project_id, rel, target, replacement) {
                Ok(snap) => {
                    let rec = tool_record(
                        call_id,
                        name,
                        args_json.clone(),
                        "completed",
                        format!("파일 부분 수정 완료: {}", rel),
                        Some(rel.replace('\\', "/")),
                        Some(snap.id.clone()),
                    );
                    (rec, Some(snap), false)
                }
                Err(err) => fail(err, Some(rel.to_string())),
            }
        }
        "search_files" => match fs_ops::search_in_files(root, str_arg(args, "query")) {
            Ok(out) => done(out, None),
            Err(err) => fail(err, None),
        },
        "delete_path" | "run_command" => {
            let file_path = if name == "delete_path" {
                Some(str_arg(args, "relative_path").replace('\\', "/"))
            } else {
                None
            };
            (
                tool_record(
                    call_id,
                    name,
                    args_json.clone(),
                    "pending_approval",
                    "사용자 승인을 기다리고 있습니다.".to_string(),
                    file_path,
                    None,
                ),
                None,
                true,
            )
        }
        _ => fail(format!("지원하지 않는 도구입니다: {}", name), None),
    }
}

/// DOCX/PPTX/PDF 파일에 대한 read_file / write_file / modify_file
#[allow(clippy::too_many_arguments)]
async fn document_tool<R: Runtime>(
    app: &AppHandle<R>,
    root: &Path,
    project_id: &str,
    call_id: &str,
    name: &str,
    args: &Value,
    args_json: &str,
    rel: &str,
    kind: DocKind,
) -> (ToolExecutionRecord, Option<FileSnapshot>, bool) {
    let fail = |msg: String| {
        (
            tool_record(call_id, name, args_json.to_string(), "error", msg, Some(rel.replace('\\', "/")), None),
            None,
            false,
        )
    };

    match name {
        "read_file" => {
            let res = match fs_ops::read_file_bytes(root, rel) {
                Ok(bytes) => doc_bridge::extract_text(app, kind, &bytes).await,
                Err(e) => Err(e),
            };
            match res {
                Ok(text) => (
                    tool_record(
                        call_id,
                        name,
                        args_json.to_string(),
                        "completed",
                        format!("[{} 문서를 Markdown으로 변환한 내용]\n\n{}", kind.label(), text),
                        Some(rel.replace('\\', "/")),
                        None,
                    ),
                    None,
                    false,
                ),
                Err(e) => fail(e),
            }
        }
        "write_file" => {
            if kind == DocKind::Pdf {
                return fail(
                    "PDF 파일은 만들 수 없습니다. 같은 내용을 .docx(Word) 또는 .md로 저장하세요.".to_string(),
                );
            }
            let content = str_arg(args, "content");
            let res = match doc_bridge::generate(app, kind, content, &file_title(rel)).await {
                Ok(bytes) => fs_ops::write_bytes_with_snapshot(root, project_id, rel, &bytes, "write"),
                Err(e) => Err(e),
            };
            let msg = format!("{} 문서 작성 완료: {}", kind.label(), rel);
            snapshot_result(call_id, name, args_json, rel, res, msg)
        }
        "modify_file" => {
            if kind == DocKind::Pdf {
                return fail("PDF는 수정할 수 없습니다. 내용을 .docx나 .md로 새로 작성하세요.".to_string());
            }
            let target = str_arg(args, "target");
            if target.is_empty() {
                return fail("target이 비어 있습니다. 교체할 원문을 정확히 지정하세요.".to_string());
            }
            let replacement = str_arg(args, "replacement");
            let res = match fs_ops::read_file_bytes(root, rel) {
                Ok(bytes) => match doc_bridge::replace_text(app, kind, &bytes, target, replacement).await {
                    Ok(new_bytes) => fs_ops::write_bytes_with_snapshot(root, project_id, rel, &new_bytes, "modify"),
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            let msg = format!("{} 문서 부분 수정 완료 (서식 유지): {}", kind.label(), rel);
            snapshot_result(call_id, name, args_json, rel, res, msg)
        }
        _ => fail(format!("지원하지 않는 도구입니다: {}", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, name: &str, status: &str, output: &str, round: u32) -> ToolExecutionRecord {
        ToolExecutionRecord {
            id: id.into(),
            tool_name: name.into(),
            arguments_json: r#"{"relative_path":"a.md"}"#.into(),
            status: status.into(),
            output: output.into(),
            file_path: None,
            snapshot_id: None,
            round,
        }
    }

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: role.into(),
            content: content.into(),
            reasoning_content: None,
            tool_records: vec![],
            created_at: String::new(),
            round_contents: vec![],
        }
    }

    #[test]
    fn decoder_keeps_korean_split_across_chunks() {
        let line = "data: {\"t\":\"안녕하세요\"}\n";
        let bytes = line.as_bytes();
        // '안'(3바이트)의 중간에서 자른다.
        let cut = line.find('안').unwrap() + 1;
        let mut d = SseDecoder::default();
        assert!(d.push(&bytes[..cut]).is_empty());
        let lines = d.push(&bytes[cut..]);
        assert_eq!(lines, vec!["data: {\"t\":\"안녕하세요\"}".to_string()]);
    }

    #[test]
    fn decoder_handles_crlf_and_trailing_line() {
        let mut d = SseDecoder::default();
        let lines = d.push(b"data: a\r\n\r\ndata: b");
        assert_eq!(lines, vec!["data: a".to_string(), String::new()]);
        assert_eq!(d.finish(), Some("data: b".to_string()));
        assert_eq!(d.finish(), None);
    }

    #[test]
    fn parse_sse_line_variants() {
        assert_eq!(parse_sse_line("data: [DONE]"), SseEvent::Done);
        assert_eq!(parse_sse_line(": keep-alive"), SseEvent::Skip);
        assert_eq!(parse_sse_line("data: not json"), SseEvent::Skip);
        assert_eq!(parse_sse_line("data:{\"a\":1}"), SseEvent::Data(json!({"a":1})));
    }

    #[test]
    fn accumulator_merges_tool_call_fragments() {
        let mut acc = RoundAccumulator::default();
        let chunks = [
            json!({"choices":[{"delta":{"reasoning_content":"생각"}}]}),
            json!({"choices":[{"delta":{"content":"파일을 읽겠습니다."}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"relative_"}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"read_file","arguments":"path\":\"a.md\"}"}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_2","function":{"name":"list_directory","arguments":"{}"}}]}}]}),
        ];
        for c in &chunks {
            acc.apply_chunk(c).unwrap();
        }
        assert_eq!(acc.reasoning, "생각");
        assert_eq!(acc.content, "파일을 읽겠습니다.");
        assert_eq!(acc.tools.len(), 2);
        assert_eq!(acc.tools[0].name, "read_file");
        assert_eq!(acc.tools[0].arguments, "{\"relative_path\":\"a.md\"}");
        assert_eq!(acc.tools[1].id, "call_2");
    }

    #[test]
    fn accumulator_reports_stream_error() {
        let mut acc = RoundAccumulator::default();
        let err = acc
            .apply_chunk(&json!({"error":{"message":"quota exceeded"}}))
            .unwrap_err();
        assert!(err.contains("quota exceeded"));
    }

    #[test]
    fn api_messages_replay_tool_rounds_in_order() {
        let mut a = msg("assistant", "");
        a.round_contents = vec!["먼저 읽겠습니다.".into(), "".into(), "완료했습니다.".into()];
        a.tool_records = vec![
            rec("c1", "read_file", "completed", "본문", 0),
            rec("c2", "run_command", "rejected", "사용자가 실행을 거부했습니다.", 1),
        ];
        let history = vec![msg("user", "요약해줘"), a];
        let out = build_api_messages("sys", &history, true);
        let roles: Vec<&str> = out.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(
            roles,
            vec!["system", "user", "assistant", "tool", "assistant", "tool", "assistant"]
        );
        assert_eq!(out[2]["content"], "먼저 읽겠습니다.");
        assert_eq!(out[2]["tool_calls"][0]["id"], "c1");
        assert_eq!(out[3]["tool_call_id"], "c1");
        assert_eq!(out[3]["content"], "본문");
        assert!(out[4]["content"].is_null());
        assert!(out[5]["content"].as_str().unwrap().contains("거부"));
        assert_eq!(out[6]["content"], "완료했습니다.");
    }

    #[test]
    fn api_messages_without_tools_send_plain_text() {
        let mut a = msg("assistant", "답변");
        a.tool_records = vec![rec("c1", "read_file", "completed", "본문", 0)];
        let out = build_api_messages("sys", &[msg("user", "q"), a, msg("assistant", "")], false);
        assert_eq!(out.len(), 4);
        assert_eq!(out[2]["content"], "답변");
        assert!(out[2].get("tool_calls").is_none());
        assert_eq!(out[3]["content"], "(응답 없음)");
    }

    #[test]
    fn legacy_message_puts_tools_before_answer() {
        let mut a = msg("assistant", "최종 답변");
        a.tool_records = vec![rec("c1", "read_file", "completed", "본문", 0)];
        let rounds = message_rounds(&a);
        assert_eq!(rounds.len(), 2);
        assert!(rounds[0].0.is_empty());
        assert_eq!(rounds[0].1.len(), 1);
        assert_eq!(rounds[1].0, "최종 답변");
    }

    #[test]
    fn invalid_tool_arguments_are_not_replayed() {
        let mut r = rec("c1", "read_file", "error", "bad", 0);
        r.arguments_json = "{\"relative_path\":".into();
        assert_eq!(tool_call_json(&r)["function"]["arguments"], "{}");
    }

    #[tokio::test]
    async fn pending_tools_wait_for_approval() {
        let app = tauri::test::mock_app();
        let dir = tempfile::tempdir().unwrap();
        let (r, snap, needs) = execute_or_queue_tool(
            app.handle(), dir.path(), "p", "c1", "run_command", &json!({"command":"dir"}),
        )
        .await;
        assert!(needs);
        assert!(snap.is_none());
        assert_eq!(r.status, "pending_approval");
        assert_eq!(r.file_path, None);

        let (r, _, needs) = execute_or_queue_tool(
            app.handle(), dir.path(), "p", "c2", "list_directory", &json!({"relative_path":"../x"}),
        )
        .await;
        assert!(!needs);
        assert_eq!(r.status, "error");
    }

    /// 프론트엔드 문서 변환기를 흉내 내는 응답기를 붙인 모의 앱
    fn app_with_fake_converter() -> tauri::App<tauri::test::MockRuntime> {
        use base64::Engine;
        use tauri::{Listener, Manager};
        let app = tauri::test::mock_app();
        app.manage(doc_bridge::DocBridge::default());
        let handle = app.handle().clone();
        let responder = handle.clone();
        handle.listen(doc_bridge::REQUEST_EVENT, move |event| {
            let v: Value = serde_json::from_str(event.payload()).unwrap();
            let id = v["requestId"].as_str().unwrap().to_string();
            let b64 = base64::engine::general_purpose::STANDARD;
            let reply = match v["op"].as_str().unwrap() {
                "generate" => doc_bridge::BridgeReply {
                    ok: true,
                    data_base64: Some(b64.encode(format!("FAKE-{}:{}", v["kind"].as_str().unwrap(), v["markdown"].as_str().unwrap()))),
                    ..Default::default()
                },
                "extract" => {
                    let raw = b64.decode(v["dataBase64"].as_str().unwrap()).unwrap();
                    doc_bridge::BridgeReply { ok: true, text: Some(format!("추출:{}", String::from_utf8_lossy(&raw))), ..Default::default() }
                }
                _ => doc_bridge::BridgeReply { ok: false, error: Some("교체 대상 없음".into()), ..Default::default() },
            };
            responder.state::<doc_bridge::DocBridge>().respond(&id, reply).unwrap();
        });
        app
    }

    #[tokio::test]
    async fn document_tools_go_through_converter_and_snapshot() {
        let app = app_with_fake_converter();
        let dir = tempfile::tempdir().unwrap();

        let (w, snap, _) = execute_or_queue_tool(
            app.handle(), dir.path(), "p", "c1", "write_file",
            &json!({"relative_path": "docs/보고서.docx", "content": "# 제목"}),
        )
        .await;
        assert_eq!(w.status, "completed", "{}", w.output);
        assert!(snap.is_some());
        assert_eq!(std::fs::read(dir.path().join("docs/보고서.docx")).unwrap(), b"FAKE-docx:# \xEC\xA0\x9C\xEB\xAA\xA9");

        let (r, _, _) = execute_or_queue_tool(
            app.handle(), dir.path(), "p", "c2", "read_file", &json!({"relative_path": "docs/보고서.docx"}),
        )
        .await;
        assert_eq!(r.status, "completed");
        assert!(r.output.contains("추출:FAKE-docx:# 제목"), "{}", r.output);

        let (m, _, _) = execute_or_queue_tool(
            app.handle(), dir.path(), "p", "c3", "modify_file",
            &json!({"relative_path": "docs/보고서.docx", "target": "없음", "replacement": "x"}),
        )
        .await;
        assert_eq!(m.status, "error");
        assert!(m.output.contains("교체 대상 없음"));

        let (pdf, _, _) = execute_or_queue_tool(
            app.handle(), dir.path(), "p", "c4", "write_file", &json!({"relative_path": "a.pdf", "content": "x"}),
        )
        .await;
        assert_eq!(pdf.status, "error");
        assert!(pdf.output.contains("PDF 파일은 만들 수 없습니다"));
    }

    // -----------------------------------------------------------------------
    // 모의 EXAONE 서버를 이용한 에이전트 루프 통합 테스트
    // -----------------------------------------------------------------------

    use std::sync::Mutex as StdMutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sse(v: Value) -> Vec<u8> {
        format!("data: {}\n\n", v).into_bytes()
    }

    fn sse_done() -> Vec<u8> {
        b"data: [DONE]\n\n".to_vec()
    }

    fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    /// 요청마다 미리 정한 SSE 청크들을 순서대로 돌려주고, 받은 요청 본문을 기록한다.
    async fn mock_server(
        responses: Vec<Vec<Vec<u8>>>,
        chunk_delay_ms: u64,
    ) -> (String, Arc<StdMutex<Vec<Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/chat/completions", listener.local_addr().unwrap());
        let bodies = Arc::new(StdMutex::new(Vec::new()));
        let bodies_c = bodies.clone();
        tokio::spawn(async move {
            for chunks in responses {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let body = loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break Vec::new();
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = find_bytes(&buf, b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .map(|v| v.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if buf.len() >= pos + 4 + len {
                            break buf[pos + 4..pos + 4 + len].to_vec();
                        }
                    }
                };
                bodies_c
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n")
                    .await;
                for c in chunks {
                    if sock.write_all(&c).await.is_err() {
                        break;
                    }
                    let _ = sock.flush().await;
                    tokio::time::sleep(Duration::from_millis(chunk_delay_ms)).await;
                }
                let _ = sock.shutdown().await;
            }
        });
        (url, bodies)
    }

    fn test_settings(url: String) -> AppSettings {
        AppSettings {
            api_key: "test-key".into(),
            base_url: url,
            model: "k-exaone_v2.5".into(),
            reasoning_effort: "high".into(),
            writing_style: "default".into(),
        }
    }

    fn test_project(root: &Path) -> Project {
        Project {
            id: "proj-test".into(),
            name: "테스트".into(),
            root_path: root.to_string_lossy().to_string(),
            instructions: String::new(),
            created_at: String::new(),
        }
    }

    #[tokio::test]
    async fn agent_executes_tool_then_answers() {
        let dir = tempfile::tempdir().unwrap();
        let first: Vec<u8> = [
            sse(json!({"choices":[{"delta":{"reasoning_content":"보고서를 써야 한다"}}]})),
            sse(json!({"choices":[{"delta":{"content":"파일을 작성하겠습니다."}}]})),
            sse(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","function":{"name":"write_file","arguments":"{\"relative_path\":\"docs/r.md\","}}]}}]})),
            sse(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"content\":\"# 보고서\"}"}}]}}]})),
            sse_done(),
        ]
        .concat();
        // 한글 글자 바이트 중간에서 TCP 청크를 나눠 보낸다.
        let cut = find_bytes(&first, "보고서를".as_bytes()).unwrap() + 1;
        let first_chunks = vec![first[..cut].to_vec(), first[cut..].to_vec()];
        let second = vec![
            sse(json!({"choices":[{"delta":{"content":"docs/r.md에 저장했습니다."}}]})),
            sse_done(),
        ];
        let (url, bodies) = mock_server(vec![first_chunks, second], 5).await;

        let app = tauri::test::mock_app();
        let settings = test_settings(url);
        let project = test_project(dir.path());
        let history = vec![msg("user", "보고서 써줘")];
        let (out, snaps) = run_exaone_turn(
            app.handle(),
            TurnRequest {
                settings: &settings,
                project: Some(&project),
                conversation_id: "conv-1",
                history: &history,
                continue_from: None,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("docs/r.md")).unwrap(),
            "# 보고서"
        );
        assert_eq!(snaps.len(), 1);
        assert_eq!(out.reasoning_content.as_deref(), Some("보고서를 써야 한다"));
        assert_eq!(
            out.round_contents,
            vec!["파일을 작성하겠습니다.".to_string(), "docs/r.md에 저장했습니다.".to_string()]
        );
        assert_eq!(out.content, "파일을 작성하겠습니다.\n\ndocs/r.md에 저장했습니다.");
        assert_eq!(out.tool_records.len(), 1);
        assert_eq!(out.tool_records[0].status, "completed");
        assert_eq!(out.tool_records[0].round, 0);

        let bodies = bodies.lock().unwrap();
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0]["tools"].as_array().unwrap().len(), 7);
        assert_eq!(bodies[0]["reasoning_effort"], "high");
        let msgs = bodies[1]["messages"].as_array().unwrap();
        let roles: Vec<&str> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, vec!["system", "user", "assistant", "tool"]);
        assert_eq!(msgs[2]["tool_calls"][0]["id"], "call_a");
        assert_eq!(msgs[3]["tool_call_id"], "call_a");
    }

    #[tokio::test]
    async fn agent_pauses_for_approval_then_continues_same_message() {
        let dir = tempfile::tempdir().unwrap();
        let first = vec![
            sse(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_cmd","function":{"name":"run_command","arguments":"{\"command\":\"echo hi\"}"}}]}}]})),
            sse_done(),
        ];
        let second = vec![
            sse(json!({"choices":[{"delta":{"content":"명령 결과를 확인했습니다."}}]})),
            sse_done(),
        ];
        let (url, bodies) = mock_server(vec![first, second], 5).await;
        let app = tauri::test::mock_app();
        let settings = test_settings(url);
        let project = test_project(dir.path());
        let user = msg("user", "빌드해줘");

        // 1) 승인이 필요한 도구에서 멈춘다.
        let (mut paused, _) = run_exaone_turn(
            app.handle(),
            TurnRequest {
                settings: &settings,
                project: Some(&project),
                conversation_id: "conv-2",
                history: &[user.clone()],
                continue_from: None,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        assert_eq!(paused.tool_records[0].status, "pending_approval");
        assert_eq!(bodies.lock().unwrap().len(), 1);

        // 2) 사용자가 승인해 명령이 실행된 상태를 흉내 낸 뒤 이어서 진행한다.
        paused.tool_records[0].status = "completed".into();
        paused.tool_records[0].output = "[Exit Code: 0]\nhi".into();
        let history = vec![user, paused.clone()];
        let (resumed, _) = run_exaone_turn(
            app.handle(),
            TurnRequest {
                settings: &settings,
                project: Some(&project),
                conversation_id: "conv-2",
                history: &history,
                continue_from: Some(paused.clone()),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();

        assert_eq!(resumed.id, paused.id);
        assert_eq!(resumed.content, "명령 결과를 확인했습니다.");
        assert_eq!(resumed.round_contents.len(), 2);
        assert_eq!(resumed.tool_records.len(), 1);

        let bodies = bodies.lock().unwrap();
        let msgs = bodies[1]["messages"].as_array().unwrap();
        let last = msgs.last().unwrap();
        assert_eq!(last["role"], "tool");
        assert_eq!(last["tool_call_id"], "call_cmd");
        assert!(last["content"].as_str().unwrap().contains("hi"));
    }

    #[tokio::test]
    async fn cancel_stops_stream_quickly_and_keeps_partial_text() {
        let mut chunks = vec![sse(json!({"choices":[{"delta":{"content":"부분 답변"}}]}))];
        for _ in 0..30 {
            chunks.push(sse(json!({"choices":[{"delta":{"content":"."}}]})));
        }
        let (url, bodies) = mock_server(vec![chunks], 300).await;
        let app = tauri::test::mock_app();
        let settings = test_settings(url);
        let flag = Arc::new(AtomicBool::new(false));
        let flag_c = flag.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            flag_c.store(true, Ordering::Relaxed);
        });

        let started = Instant::now();
        let (out, _) = run_exaone_turn(
            app.handle(),
            TurnRequest {
                settings: &settings,
                project: None,
                conversation_id: "conv-3",
                history: &[msg("user", "길게 말해줘")],
                continue_from: None,
            },
            flag,
        )
        .await
        .unwrap();

        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(out.content.starts_with("부분 답변"), "{}", out.content);
        assert!(out.content.contains("중지"), "{}", out.content);
        // 일반 채팅에는 도구 정의를 보내지 않는다.
        assert!(bodies.lock().unwrap()[0].get("tools").is_none());
    }
}
