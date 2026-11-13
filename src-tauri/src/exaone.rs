use crate::fs_ops;
use crate::store::{
    AppSettings, ChatMessage, FileSnapshot, Project, ToolExecutionRecord,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
                "description": "프로젝트 폴더 내의 로컬 파일 내용을 읽습니다.",
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
                "description": "프로젝트 폴더 내에 새 파일을 작성하거나 전체 내용을 덮어씁니다.",
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
                "description": "기존 로컬 파일에서 특정 구간(target)을 찾아 새로운 내용(replacement)으로 부분 수정합니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "relative_path": {
                            "type": "string",
                            "description": "수정할 파일 상대 경로"
                        },
                        "target": {
                            "type": "string",
                            "description": "교체할 기존 원문 문자열"
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
                "description": "프로젝트 루트 경로에서 터미널/쉘 명령어를 실행합니다 (사용자 승인 후 실행됨).",
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

#[derive(Default, Clone)]
struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

pub async fn run_exaone_turn(
    app: &AppHandle,
    settings: &AppSettings,
    project: &Project,
    conversation_id: &str,
    history: &[ChatMessage],
    cancel_flag: Arc<AtomicBool>,
) -> Result<(ChatMessage, Vec<FileSnapshot>), String> {
    let assistant_msg_id = Uuid::new_v4().to_string();
    let mut accumulated_reasoning = String::new();
    let mut accumulated_content = String::new();
    let mut tool_records: Vec<ToolExecutionRecord> = Vec::new();
    let mut new_snapshots: Vec<FileSnapshot> = Vec::new();

    if settings.api_key.trim().is_empty() {
        return Err("EXAONE API 키가 설정되지 않았습니다. 좌측 하단 [API 설정] 버튼을 눌러 API Key를 입력해 주세요.".to_string());
    }

    let root_path = Path::new(&project.root_path);
    let enable_tools = settings.writing_style == "project" || settings.writing_style == "code_architect";

    let sys_prompt = if enable_tools {
        format!(
            "당신은 LG AI연구원의 K-EXAONE 2.5 기반 데스크톱 어시스턴트입니다.\n\
현재 프로젝트: {} (로컬 경로: {})\n\
프로젝트 지침: {}\n\
로컬 파일 읽기(read_file, list_directory, search_files), 쓰기(write_file), 수정(modify_file), 삭제(delete_path), 명령어 실행(run_command) 도구를 사용할 수 있습니다.",
            project.name, project.root_path, project.instructions
        )
    } else {
        format!(
            "당신은 LG AI연구원의 K-EXAONE 2.5 기반 데스크톱 어시스턴트입니다. 한국어로 명확하고 도움이 되는 답변을 작성하세요.\n{}",
            project.instructions
        )
    };

    let mut api_messages: Vec<Value> = vec![json!({
        "role": "system",
        "content": sys_prompt
    })];

    for msg in history {
        if msg.role == "user" || msg.role == "assistant" {
            api_messages.push(json!({
                "role": msg.role,
                "content": msg.content
            }));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let max_rounds = if enable_tools { 4 } else { 1 };

    for _round in 0..max_rounds {
        let mut body = json!({
            "model": settings.model,
            "messages": api_messages,
            "stream": true
        });

        if enable_tools {
            body["tools"] = build_tools_schema();
            body["tool_choice"] = json!("auto");
        }

        if !settings.reasoning_effort.is_empty() {
            body["reasoning_effort"] = json!(settings.reasoning_effort);
        }

        // Retry loop for 429 Too Many Requests / 503 Service Busy
        let retry_delays_sec = [2u64, 3, 5, 8, 12];
        let mut response_opt = None;
        let mut last_error = String::new();

        for attempt in 0..=retry_delays_sec.len() {
            let send_res = client
                .post(&settings.base_url)
                .header("Authorization", format!("Bearer {}", settings.api_key.trim()))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            match send_res {
                Ok(resp) => {
                    if resp.status().is_success() {
                        // Clear any retry status notice
                        let _ = app.emit(
                            "exaone-stream",
                            StreamChunkPayload {
                                conversation_id: conversation_id.to_string(),
                                message_id: assistant_msg_id.clone(),
                                delta_reasoning: None,
                                delta_content: None,
                                tool_record: None,
                                snapshot: None,
                                status_notice: Some(String::new()),
                                done: false,
                                error: None,
                            },
                        );
                        response_opt = Some(resp);
                        break;
                    }

                    let status = resp.status();
                    let err_text = resp.text().await.unwrap_or_default();

                    if (status.as_u16() == 429 || status.as_u16() == 503)
                        && attempt < retry_delays_sec.len()
                    {
                        let wait_secs = retry_delays_sec[attempt];
                        let notice = format!(
                            "현재 EXAONE API 서버 동시 접속자가 많아 자동 재시도 중입니다... ({}/{}회, {}초 대기)",
                            attempt + 1,
                            retry_delays_sec.len(),
                            wait_secs
                        );
                        let _ = app.emit(
                            "exaone-stream",
                            StreamChunkPayload {
                                conversation_id: conversation_id.to_string(),
                                message_id: assistant_msg_id.clone(),
                                delta_reasoning: None,
                                delta_content: None,
                                tool_record: None,
                                snapshot: None,
                                status_notice: Some(notice),
                                done: false,
                                error: None,
                            },
                        );
                        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                        continue;
                    }

                    last_error = format!("EXAONE API 응답 오류 ({}): {}", status, err_text);
                    break;
                }
                Err(e) => {
                    last_error = format!("EXAONE API 네트워크 연결 오류: {}", e);
                    break;
                }
            }
        }

        let Some(response) = response_opt else {
            return Err(last_error);
        };

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut pending_tools: Vec<PendingToolCall> = Vec::new();

        while let Some(chunk_res) = stream.next().await {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            let chunk = chunk_res.map_err(|e| format!("스트림 수신 오류: {}", e))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if !line.starts_with("data:") {
                    continue;
                }
                let data_str = line.trim_start_matches("data:").trim();
                if data_str == "[DONE]" || data_str.is_empty() {
                    continue;
                }

                let Ok(val) = serde_json::from_str::<Value>(data_str) else {
                    continue;
                };

                let Some(choice) = val.get("choices").and_then(|c| c.get(0)) else {
                    continue;
                };
                let Some(delta) = choice.get("delta") else {
                    continue;
                };

                if let Some(r_text) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                    if !r_text.is_empty() {
                        accumulated_reasoning.push_str(r_text);
                        let _ = app.emit(
                            "exaone-stream",
                            StreamChunkPayload {
                                conversation_id: conversation_id.to_string(),
                                message_id: assistant_msg_id.clone(),
                                delta_reasoning: Some(r_text.to_string()),
                                delta_content: None,
                                tool_record: None,
                                snapshot: None,
                                status_notice: None,
                                done: false,
                                error: None,
                            },
                        );
                    }
                }

                if let Some(c_text) = delta.get("content").and_then(|v| v.as_str()) {
                    if !c_text.is_empty() {
                        accumulated_content.push_str(c_text);
                        let _ = app.emit(
                            "exaone-stream",
                            StreamChunkPayload {
                                conversation_id: conversation_id.to_string(),
                                message_id: assistant_msg_id.clone(),
                                delta_reasoning: None,
                                delta_content: Some(c_text.to_string()),
                                tool_record: None,
                                snapshot: None,
                                status_notice: None,
                                done: false,
                                error: None,
                            },
                        );
                    }
                }

                if let Some(tc_arr) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tc_arr {
                        let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        while pending_tools.len() <= idx {
                            pending_tools.push(PendingToolCall::default());
                        }
                        if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                            pending_tools[idx].id = id.to_string();
                        }
                        if let Some(func) = tc.get("function") {
                            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                pending_tools[idx].name.push_str(name);
                            }
                            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                pending_tools[idx].arguments.push_str(args);
                            }
                        }
                    }
                }
            }
        }

        let valid_tools: Vec<PendingToolCall> = pending_tools
            .into_iter()
            .filter(|t| !t.name.is_empty())
            .collect();

        if valid_tools.is_empty() {
            break;
        }

        let assistant_tool_calls_json: Vec<Value> = valid_tools
            .iter()
            .map(|t| {
                let id = if t.id.is_empty() {
                    format!("call_{}", Uuid::new_v4().simple())
                } else {
                    t.id.clone()
                };
                json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "arguments": t.arguments
                    }
                })
            })
            .collect();

        api_messages.push(json!({
            "role": "assistant",
            "content": accumulated_content,
            "tool_calls": assistant_tool_calls_json
        }));

        let mut paused_for_approval = false;

        for (i, tc) in valid_tools.iter().enumerate() {
            let call_id = assistant_tool_calls_json[i]["id"]
                .as_str()
                .unwrap_or("call_0")
                .to_string();

            let args_val: Value =
                serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));

            let (record, maybe_snap, requires_approval) =
                execute_or_queue_tool(root_path, &project.id, &call_id, &tc.name, &args_val);

            if let Some(snap) = maybe_snap.clone() {
                new_snapshots.push(snap);
            }

            let _ = app.emit(
                "exaone-stream",
                StreamChunkPayload {
                    conversation_id: conversation_id.to_string(),
                    message_id: assistant_msg_id.clone(),
                    delta_reasoning: None,
                    delta_content: None,
                    tool_record: Some(record.clone()),
                    snapshot: maybe_snap,
                    status_notice: None,
                    done: false,
                    error: None,
                },
            );

            api_messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": record.output
            }));

            tool_records.push(record);

            if requires_approval {
                paused_for_approval = true;
            }
        }

        if paused_for_approval {
            break;
        }
    }

    let final_msg = ChatMessage {
        id: assistant_msg_id.clone(),
        role: "assistant".to_string(),
        content: accumulated_content,
        reasoning_content: if accumulated_reasoning.is_empty() {
            None
        } else {
            Some(accumulated_reasoning)
        },
        tool_records,
        created_at: chrono::Local::now().to_rfc3339(),
    };

    let _ = app.emit(
        "exaone-stream",
        StreamChunkPayload {
            conversation_id: conversation_id.to_string(),
            message_id: assistant_msg_id,
            delta_reasoning: None,
            delta_content: None,
            tool_record: None,
            snapshot: None,
            status_notice: None,
            done: true,
            error: None,
        },
    );

    Ok((final_msg, new_snapshots))
}

fn execute_or_queue_tool(
    root: &Path,
    project_id: &str,
    call_id: &str,
    name: &str,
    args: &Value,
) -> (ToolExecutionRecord, Option<FileSnapshot>, bool) {
    let args_pretty = serde_json::to_string(args).unwrap_or_default();

    match name {
        "list_directory" => {
            let rel = args
                .get("relative_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let target = fs_ops::resolve_safe_path(root, rel).unwrap_or_else(|_| root.to_path_buf());
            let res = fs_ops::list_project_tree(&target, 2)
                .map(|nodes| serde_json::to_string_pretty(&nodes).unwrap_or_default());
            match res {
                Ok(out) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "completed".to_string(),
                        output: out,
                        file_path: Some(rel.to_string()),
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
                Err(err) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "error".to_string(),
                        output: err,
                        file_path: Some(rel.to_string()),
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
            }
        }
        "read_file" => {
            let rel = args
                .get("relative_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match fs_ops::read_file_content(root, rel) {
                Ok(content) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "completed".to_string(),
                        output: content,
                        file_path: Some(rel.to_string()),
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
                Err(err) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "error".to_string(),
                        output: err,
                        file_path: Some(rel.to_string()),
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
            }
        }
        "write_file" => {
            let rel = args
                .get("relative_path")
                .and_then(|v| v.as_str())
                .unwrap_or("untitled.md");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            match fs_ops::write_file_with_snapshot(root, project_id, rel, content) {
                Ok(snap) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "completed".to_string(),
                        output: format!("파일 작성 완료: {} ({} bytes)", rel, content.len()),
                        file_path: Some(rel.replace('\\', "/")),
                        snapshot_id: Some(snap.id.clone()),
                    },
                    Some(snap),
                    false,
                ),
                Err(err) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "error".to_string(),
                        output: err,
                        file_path: Some(rel.to_string()),
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
            }
        }
        "modify_file" => {
            let rel = args
                .get("relative_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let replacement = args
                .get("replacement")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match fs_ops::modify_file_with_snapshot(root, project_id, rel, target, replacement) {
                Ok(snap) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "completed".to_string(),
                        output: format!("파일 부분 수정 완료: {}", rel),
                        file_path: Some(rel.replace('\\', "/")),
                        snapshot_id: Some(snap.id.clone()),
                    },
                    Some(snap),
                    false,
                ),
                Err(err) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "error".to_string(),
                        output: err,
                        file_path: Some(rel.to_string()),
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
            }
        }
        "search_files" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            match fs_ops::search_in_files(root, query) {
                Ok(out) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "completed".to_string(),
                        output: out,
                        file_path: None,
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
                Err(err) => (
                    ToolExecutionRecord {
                        id: call_id.to_string(),
                        tool_name: name.to_string(),
                        arguments_json: args_pretty,
                        status: "error".to_string(),
                        output: err,
                        file_path: None,
                        snapshot_id: None,
                    },
                    None,
                    false,
                ),
            }
        }
        "delete_path" | "run_command" => {
            let file_path = args
                .get("relative_path")
                .or_else(|| args.get("command"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            (
                ToolExecutionRecord {
                    id: call_id.to_string(),
                    tool_name: name.to_string(),
                    arguments_json: args_pretty,
                    status: "pending_approval".to_string(),
                    output: "사용자 승인을 기다리고 있습니다.".to_string(),
                    file_path,
                    snapshot_id: None,
                },
                None,
                true,
            )
        }
        _ => (
            ToolExecutionRecord {
                id: call_id.to_string(),
                tool_name: name.to_string(),
                arguments_json: args_pretty,
                status: "error".to_string(),
                output: format!("지원하지 않는 도구입니다: {}", name),
                file_path: None,
                snapshot_id: None,
            },
            None,
            false,
        ),
    }
}
