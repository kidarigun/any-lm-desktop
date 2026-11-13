pub mod doc_bridge;
pub mod exaone;
pub mod fs_ops;
pub mod store;

use exaone::TurnRequest;
use fs_ops::FileNode;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::{
    AppDatabase, AppSettings, ChatMessage, Conversation, FileSnapshot, Project, StoreManager,
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

/// 일반 채팅(프로젝트 폴더와 연결되지 않은 대화)의 project_id.
pub const GENERAL_CHAT_ID: &str = "general-chat";
/// 셸 명령 최대 실행 시간.
const SHELL_TIMEOUT: Duration = Duration::from_secs(120);

pub struct AppState {
    pub store: StoreManager,
    pub db: Mutex<AppDatabase>,
    pub cancel_flag: Arc<AtomicBool>,
}

/// 대화가 속한 프로젝트를 찾는다. 일반 채팅이면 `None`(도구 비활성).
fn conversation_project(db: &AppDatabase, conv: &Conversation) -> Result<Option<Project>, String> {
    if conv.project_id == GENERAL_CHAT_ID {
        return Ok(None);
    }
    db.projects
        .iter()
        .find(|p| p.id == conv.project_id)
        .cloned()
        .map(Some)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())
}

/// 사용자가 승인/거부하지 않고 다음 메시지를 보내면, 남은 승인 대기 작업은 실행하지 않은 것으로 처리한다.
fn expire_pending_approvals(conv: &mut Conversation) {
    for m in conv.messages.iter_mut() {
        for r in m.tool_records.iter_mut() {
            if r.status == "pending_approval" {
                r.status = "rejected".to_string();
                r.output =
                    "사용자가 승인하지 않고 다음 메시지를 보내 실행되지 않았습니다.".to_string();
            }
        }
    }
}

fn find_conv_mut<'a>(db: &'a mut AppDatabase, conversation_id: &str) -> Result<&'a mut Conversation, String> {
    db.conversations
        .iter_mut()
        .find(|c| c.id == conversation_id)
        .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())
}

/// 첨부 파일들을 읽어 메시지 끝에 붙일 블록을 만든다. 읽지 못한 파일은 이유를 남긴다.
async fn read_attachments<R: tauri::Runtime>(
    app: &AppHandle<R>,
    project: Option<&Project>,
    files: &[String],
) -> String {
    if files.is_empty() {
        return String::new();
    }
    let mut block = String::from("\n\n---\n[첨부 파일 내용]\n");
    for fpath in files {
        let display_name = Path::new(fpath)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| fpath.clone());
        let bytes = if Path::new(fpath).is_absolute() {
            std::fs::read(fpath).map_err(|e| e.to_string())
        } else if let Some(p) = project {
            fs_ops::read_file_bytes(Path::new(&p.root_path), fpath)
        } else {
            Err("일반 채팅에서는 프로젝트 상대 경로 첨부를 지원하지 않습니다.".to_string())
        };
        let text = match (bytes, doc_bridge::DocKind::from_path(fpath)) {
            (Ok(b), Some(kind)) => doc_bridge::extract_text(app, kind, &b).await,
            (Ok(b), None) => String::from_utf8(b)
                .map_err(|_| "텍스트 파일이 아니어서 내용을 읽을 수 없습니다.".to_string()),
            (Err(e), _) => Err(e),
        };
        match text {
            Ok(t) => block.push_str(&format!(
                "\n### 파일: `{}`\n```\n{}\n```\n",
                display_name,
                fs_ops::truncate_for_model(&t, exaone::MAX_TOOL_OUTPUT_CHARS * 2)
            )),
            Err(e) => block.push_str(&format!("\n### 파일: `{}`\n(읽기 실패: {})\n", display_name, e)),
        }
    }
    block
}

/// 에이전트 턴 결과를 대화에 반영한다. 같은 id의 메시지가 있으면 교체(승인 후 이어가기), 없으면 추가.
fn commit_assistant_message(
    state: &AppState,
    conversation_id: &str,
    assistant_msg: ChatMessage,
    new_snapshots: Vec<FileSnapshot>,
) -> Result<Conversation, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.snapshots.extend(new_snapshots);
    let conv = find_conv_mut(&mut db, conversation_id)?;
    if let Some(existing) = conv.messages.iter_mut().find(|m| m.id == assistant_msg.id) {
        *existing = assistant_msg;
    } else {
        conv.messages.push(assistant_msg);
    }
    conv.updated_at = chrono::Local::now().to_rfc3339();
    let updated = conv.clone();
    state.store.save(&db)?;
    Ok(updated)
}

#[tauri::command]
fn get_initial_state(state: State<'_, AppState>) -> Result<AppDatabase, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db.clone())
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.settings = settings.clone();
    state.store.save(&db)?;
    Ok(settings)
}

#[tauri::command]
async fn pick_folder_dialog(initial_dir: Option<String>) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let init = initial_dir.unwrap_or_default().replace('\'', "''");
            let script = format!(
                r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.FolderBrowserDialog
                $dlg.Description = '프로젝트 폴더를 선택하세요'
                $dlg.UseDescriptionForTitle = $true
                $dlg.AutoUpgradeEnabled = $true
                if ('{init}' -ne '' -and (Test-Path '{init}')) {{
                    $dlg.SelectedPath = '{init}'
                }}
                $res = $dlg.ShowDialog()
                if ($res -eq [System.Windows.Forms.DialogResult]::OK) {{
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    Write-Output $dlg.SelectedPath
                }}
                "#
            );

            let out = Command::new("powershell")
                .args(["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command", &script])
                .output()
                .map_err(|e| format!("폴더 선택 창 실행 실패: {}", e))?;

            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = initial_dir;
            Ok(None)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn pick_files_dialog(initial_dir: Option<String>) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let init = initial_dir.unwrap_or_default().replace('\'', "''");
            let script = format!(
                r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.OpenFileDialog
                $dlg.Title = '첨부할 파일을 선택하세요'
                $dlg.Multiselect = $true
                if ('{init}' -ne '' -and (Test-Path '{init}')) {{
                    $dlg.InitialDirectory = '{init}'
                }}
                $res = $dlg.ShowDialog()
                if ($res -eq [System.Windows.Forms.DialogResult]::OK) {{
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    foreach ($f in $dlg.FileNames) {{
                        Write-Output $f
                    }}
                }}
                "#
            );

            let out = Command::new("powershell")
                .args(["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command", &script])
                .output()
                .map_err(|e| format!("파일 선택 창 실행 실패: {}", e))?;

            let files: Vec<String> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Ok(files)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = initial_dir;
            Ok(Vec::new())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_generation(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn create_or_update_project(
    state: State<'_, AppState>,
    id: Option<String>,
    name: String,
    root_path: String,
    instructions: String,
) -> Result<Project, String> {
    let path_obj = Path::new(&root_path);
    if !path_obj.exists() {
        std::fs::create_dir_all(path_obj)
            .map_err(|e| format!("로컬 폴더를 생성할 수 없습니다 ({}): {}", root_path, e))?;
    }

    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    if let Some(existing_id) = id {
        if let Some(proj) = db.projects.iter_mut().find(|p| p.id == existing_id) {
            proj.name = name;
            proj.root_path = root_path;
            proj.instructions = instructions;
            let updated = proj.clone();
            state.store.save(&db)?;
            return Ok(updated);
        }
    }

    let proj = Project {
        id: format!("proj-{}", Uuid::new_v4().simple()),
        name,
        root_path,
        instructions,
        created_at: chrono::Local::now().to_rfc3339(),
    };
    db.projects.push(proj.clone());
    state.store.save(&db)?;
    Ok(proj)
}

#[tauri::command]
fn delete_project(state: State<'_, AppState>, project_id: String) -> Result<(), String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    if db.projects.len() <= 1 {
        return Err("최소 1개의 프로젝트는 유지되어야 합니다.".to_string());
    }
    db.projects.retain(|p| p.id != project_id);
    db.conversations.retain(|c| c.project_id != project_id);
    state.store.save(&db)?;
    Ok(())
}

#[tauri::command]
fn list_project_files(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<FileNode>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;

    fs_ops::list_project_tree(Path::new(&proj.root_path), 4)
}

#[tauri::command]
fn read_project_file(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;

    fs_ops::read_file_content(Path::new(&proj.root_path), &relative_path)
}

#[tauri::command]
fn write_project_file(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
    content: String,
) -> Result<FileSnapshot, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?
        .clone();

    let snap = fs_ops::write_file_with_snapshot(
        Path::new(&proj.root_path),
        &proj.id,
        &relative_path,
        &content,
    )?;
    db.snapshots.push(snap.clone());
    state.store.save(&db)?;
    Ok(snap)
}

#[tauri::command]
fn modify_project_file(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
    target: String,
    replacement: String,
) -> Result<FileSnapshot, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?
        .clone();

    let snap = fs_ops::modify_file_with_snapshot(
        Path::new(&proj.root_path),
        &proj.id,
        &relative_path,
        &target,
        &replacement,
    )?;
    db.snapshots.push(snap.clone());
    state.store.save(&db)?;
    Ok(snap)
}

#[tauri::command]
fn create_project_folder(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;

    fs_ops::create_directory(Path::new(&proj.root_path), &relative_path)
}

#[tauri::command]
fn delete_project_path(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<FileSnapshot, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?
        .clone();

    let snap = fs_ops::delete_path_with_snapshot(
        Path::new(&proj.root_path),
        &proj.id,
        &relative_path,
    )?;
    db.snapshots.push(snap.clone());
    state.store.save(&db)?;
    Ok(snap)
}

#[tauri::command]
fn undo_file_snapshot(
    state: State<'_, AppState>,
    snapshot_id: String,
) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let snap = db
        .snapshots
        .iter()
        .find(|s| s.id == snapshot_id)
        .ok_or_else(|| "스냅샷을 찾을 수 없습니다.".to_string())?
        .clone();

    let proj = db
        .projects
        .iter()
        .find(|p| p.id == snap.project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;

    fs_ops::restore_snapshot(Path::new(&proj.root_path), &snap)
}

#[tauri::command]
fn create_conversation(
    state: State<'_, AppState>,
    project_id: String,
    title: String,
) -> Result<Conversation, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let conv = Conversation {
        id: format!("conv-{}", Uuid::new_v4().simple()),
        project_id,
        title,
        messages: Vec::new(),
        updated_at: chrono::Local::now().to_rfc3339(),
    };
    db.conversations.insert(0, conv.clone());
    state.store.save(&db)?;
    Ok(conv)
}

#[tauri::command]
fn delete_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<(), String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.conversations.retain(|c| c.id != conversation_id);
    state.store.save(&db)?;
    Ok(())
}

#[tauri::command]
async fn send_chat_message(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    user_content: String,
    attached_files: Vec<String>,
) -> Result<Conversation, String> {
    state.cancel_flag.store(false, Ordering::Relaxed);
    let cancel_flag = state.cancel_flag.clone();

    // 첨부 파일은 잠금 밖에서 읽는다 (PDF/DOCX/PPTX는 웹뷰 변환기를 거친다).
    let attach_project = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let conv = db
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;
        conversation_project(&db, conv)?
    };
    let attachments_block = read_attachments(&app, attach_project.as_ref(), &attached_files).await;

    let (settings, project, history_for_api) = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db.settings.clone();
        let conv_idx = db
            .conversations
            .iter()
            .position(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;

        let project = conversation_project(&db, &db.conversations[conv_idx])?;
        let full_content = format!("{}{}", user_content, attachments_block);

        let user_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: full_content,
            reasoning_content: None,
            tool_records: Vec::new(),
            created_at: chrono::Local::now().to_rfc3339(),
            round_contents: Vec::new(),
        };

        let conv = &mut db.conversations[conv_idx];
        expire_pending_approvals(conv);
        if conv.messages.is_empty() {
            let short_title: String = user_content.chars().take(28).collect();
            conv.title = if short_title.is_empty() {
                "새 대화".to_string()
            } else {
                short_title
            };
        }
        conv.messages.push(user_msg);
        conv.updated_at = chrono::Local::now().to_rfc3339();
        let history = conv.messages.clone();
        state.store.save(&db)?;
        (settings, project, history)
    };

    let turn_res = exaone::run_exaone_turn(
        &app,
        TurnRequest {
            settings: &settings,
            project: project.as_ref(),
            conversation_id: &conversation_id,
            history: &history_for_api,
            continue_from: None,
        },
        cancel_flag,
    )
    .await;

    let (assistant_msg, new_snapshots) = match turn_res {
        Ok(res) => res,
        Err(err) => {
            if let Ok(mut db) = state.db.lock() {
                if let Some(conv) = db
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    if conv
                        .messages
                        .last()
                        .map(|m| m.role == "user")
                        .unwrap_or(false)
                    {
                        conv.messages.pop();
                    }
                    let _ = state.store.save(&db);
                }
            }
            return Err(err);
        }
    };

    commit_assistant_message(&state, &conversation_id, assistant_msg, new_snapshots)
}

#[tauri::command]
async fn edit_and_resend_message(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    message_id: String,
    new_content: String,
) -> Result<Conversation, String> {
    state.cancel_flag.store(false, Ordering::Relaxed);
    let cancel_flag = state.cancel_flag.clone();

    let (settings, project, history_for_api, backup_messages) = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db.settings.clone();
        let conv_idx = db
            .conversations
            .iter()
            .position(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;

        let project = conversation_project(&db, &db.conversations[conv_idx])?;

        let conv = &mut db.conversations[conv_idx];
        let msg_pos = conv
            .messages
            .iter()
            .position(|m| m.id == message_id)
            .ok_or_else(|| "수정할 메시지를 찾을 수 없습니다.".to_string())?;

        let backup = conv.messages.clone();
        conv.messages.truncate(msg_pos);
        expire_pending_approvals(conv);

        let updated_user_msg = ChatMessage {
            id: message_id,
            role: "user".to_string(),
            content: new_content.clone(),
            reasoning_content: None,
            tool_records: Vec::new(),
            created_at: chrono::Local::now().to_rfc3339(),
            round_contents: Vec::new(),
        };

        if conv.messages.is_empty() {
            let short_title: String = new_content.chars().take(28).collect();
            if !short_title.is_empty() {
                conv.title = short_title;
            }
        }

        conv.messages.push(updated_user_msg);
        conv.updated_at = chrono::Local::now().to_rfc3339();
        let history = conv.messages.clone();
        state.store.save(&db)?;
        (settings, project, history, backup)
    };

    let turn_res = exaone::run_exaone_turn(
        &app,
        TurnRequest {
            settings: &settings,
            project: project.as_ref(),
            conversation_id: &conversation_id,
            history: &history_for_api,
            continue_from: None,
        },
        cancel_flag,
    )
    .await;

    let (assistant_msg, new_snapshots) = match turn_res {
        Ok(res) => res,
        Err(err) => {
            if let Ok(mut db) = state.db.lock() {
                if let Some(conv) = db
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    conv.messages = backup_messages;
                    let _ = state.store.save(&db);
                }
            }
            return Err(err);
        }
    };

    commit_assistant_message(&state, &conversation_id, assistant_msg, new_snapshots)
}

/// 승인 대기 중인 삭제/명령 실행을 승인하거나 거부한다.
/// 같은 메시지에 남은 승인 대기가 없으면, 실행 결과를 모델에 전달해 에이전트 루프를 이어서 진행한다.
#[tauri::command]
async fn resolve_tool_approval(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    message_id: String,
    tool_id: String,
    approve: bool,
) -> Result<Conversation, String> {
    // 1) 대상 기록 확인 (잠금은 짧게만 잡는다)
    let (project, tool_name, args) = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let conv = db
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| "대화를 찾을 수 없습니다.".to_string())?;
        let project = conversation_project(&db, conv)?
            .ok_or_else(|| "일반 채팅에서는 도구 승인을 처리할 수 없습니다.".to_string())?;
        let conv = find_conv_mut(&mut db, &conversation_id)?;
        let rec = conv
            .messages
            .iter_mut()
            .find(|m| m.id == message_id)
            .ok_or_else(|| "메시지를 찾을 수 없습니다.".to_string())?
            .tool_records
            .iter_mut()
            .find(|r| r.id == tool_id)
            .ok_or_else(|| "도구 실행 기록을 찾을 수 없습니다.".to_string())?;
        if rec.status != "pending_approval" {
            return Err("이미 처리된 작업입니다.".to_string());
        }
        // 실행 중 중복 클릭을 막기 위해 바로 상태를 바꿔 둔다.
        rec.status = "running".to_string();
        let args: Value = serde_json::from_str(&rec.arguments_json).unwrap_or_default();
        (project, rec.tool_name.clone(), args)
    };

    // 2) 잠금 밖에서 실제 작업 실행
    let root = PathBuf::from(&project.root_path);
    let mut new_snap: Option<FileSnapshot> = None;
    let (status, output) = if !approve {
        ("rejected", "사용자가 실행을 거부했습니다.".to_string())
    } else {
        match tool_name.as_str() {
            "delete_path" => {
                let rel = args.get("relative_path").and_then(|v| v.as_str()).unwrap_or("");
                match fs_ops::delete_path_with_snapshot(&root, &project.id, rel) {
                    Ok(snap) => {
                        new_snap = Some(snap);
                        ("completed", format!("승인 완료: '{}' 삭제됨", rel))
                    }
                    Err(e) => ("error", e),
                }
            }
            "run_command" => {
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                match fs_ops::execute_shell_command(&root, cmd, SHELL_TIMEOUT).await {
                    Ok(out) => ("completed", out),
                    Err(e) => ("error", e),
                }
            }
            other => ("error", format!("승인이 필요 없는 도구입니다: {}", other)),
        }
    };

    // 3) 결과 저장 후, 남은 승인 대기가 없으면 이어서 진행할 준비
    let continuation = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        if let Some(s) = &new_snap {
            db.snapshots.push(s.clone());
        }
        let settings = db.settings.clone();
        let conv = find_conv_mut(&mut db, &conversation_id)?;
        let msg_idx = conv
            .messages
            .iter()
            .position(|m| m.id == message_id)
            .ok_or_else(|| "메시지를 찾을 수 없습니다.".to_string())?;
        {
            let msg = &mut conv.messages[msg_idx];
            if let Some(rec) = msg.tool_records.iter_mut().find(|r| r.id == tool_id) {
                rec.status = status.to_string();
                rec.output = output;
                rec.snapshot_id = new_snap.as_ref().map(|s| s.id.clone());
            }
        }
        let msg = &conv.messages[msg_idx];
        let all_resolved = !msg
            .tool_records
            .iter()
            .any(|r| r.status == "pending_approval" || r.status == "running");
        // 사용자가 그사이 새 메시지를 보냈다면 이 메시지는 더 이상 마지막이 아니므로 이어가지 않는다.
        let is_last = msg_idx + 1 == conv.messages.len();
        let continuation = if all_resolved && is_last {
            Some((settings, conv.messages.clone(), msg.clone()))
        } else {
            None
        };
        let snapshot = conv.clone();
        state.store.save(&db)?;
        match continuation {
            Some(c) => c,
            None => return Ok(snapshot),
        }
    };

    // 4) 도구 결과를 모델에 전달해 같은 메시지에 이어서 답변을 받는다.
    let (settings, history, assistant_msg) = continuation;
    state.cancel_flag.store(false, Ordering::Relaxed);
    let (updated_msg, new_snapshots) = exaone::run_exaone_turn(
        &app,
        TurnRequest {
            settings: &settings,
            project: Some(&project),
            conversation_id: &conversation_id,
            history: &history,
            continue_from: Some(assistant_msg),
        },
        state.cancel_flag.clone(),
    )
    .await?;

    commit_assistant_message(&state, &conversation_id, updated_msg, new_snapshots)
}

#[tauri::command]
async fn run_manual_terminal_command(
    state: State<'_, AppState>,
    project_id: String,
    command: String,
) -> Result<String, String> {
    let root = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let proj = db
            .projects
            .iter()
            .find(|p| p.id == project_id)
            .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;
        PathBuf::from(&proj.root_path)
    };
    fs_ops::execute_shell_command(&root, &command, SHELL_TIMEOUT).await
}

// ---------------------------------------------------------------------------
// 문서 캔버스용 파일 입출력
// ---------------------------------------------------------------------------

fn project_root(state: &AppState, project_id: &str) -> Result<(PathBuf, String), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;
    Ok((PathBuf::from(&proj.root_path), proj.id.clone()))
}

fn push_snapshot(state: &AppState, snap: &FileSnapshot) -> Result<(), String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.snapshots.push(snap.clone());
    state.store.save(&db)
}

/// 바이너리 문서(docx, pptx, pdf 등)를 base64로 읽는다.
#[tauri::command]
fn read_project_file_base64(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<String, String> {
    let (root, _) = project_root(&state, &project_id)?;
    Ok(doc_bridge::encode_base64(&fs_ops::read_file_bytes(&root, &relative_path)?))
}

/// 사용자가 파일 열기 창에서 고른 파일(프로젝트 밖일 수 있음)을 base64로 읽는다.
#[tauri::command]
fn read_absolute_file_base64(path: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 200 * 1024 * 1024;
    let p = PathBuf::from(&path);
    if !p.is_absolute() || !p.is_file() {
        return Err(format!("파일을 찾을 수 없습니다: {}", path));
    }
    let size = std::fs::metadata(&p).map_err(|e| e.to_string())?.len();
    if size > MAX_BYTES {
        return Err("200MB보다 큰 파일은 열 수 없습니다.".to_string());
    }
    let bytes = std::fs::read(&p).map_err(|e| format!("파일 읽기 오류: {}", e))?;
    Ok(doc_bridge::encode_base64(&bytes))
}

/// 바이너리 문서를 프로젝트에 저장한다 (되돌리기용 백업 포함).
#[tauri::command]
fn write_project_file_base64(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
    data_base64: String,
) -> Result<FileSnapshot, String> {
    let (root, pid) = project_root(&state, &project_id)?;
    let bytes = doc_bridge::decode_base64(&data_base64)?;
    let snap = fs_ops::write_bytes_with_snapshot(&root, &pid, &relative_path, &bytes, "write")?;
    push_snapshot(&state, &snap)?;
    Ok(snap)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    absolute_path: String,
    /// 프로젝트 폴더 안에 저장했으면 프로젝트 기준 상대 경로
    relative_path: Option<String>,
    snapshot: Option<FileSnapshot>,
}

/// 경로가 프로젝트 폴더 안이면 상대 경로를 돌려준다.
fn relative_to_root(root: &Path, absolute: &Path) -> Option<String> {
    let root = std::fs::canonicalize(root).ok()?;
    let parent = std::fs::canonicalize(absolute.parent()?).ok()?;
    let rel_dir = parent.strip_prefix(&root).ok()?;
    let rel = rel_dir.join(absolute.file_name()?);
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// "다른 이름으로 저장": 사용자가 고른 경로에 저장한다.
/// 프로젝트 폴더 안이면 스냅샷을 남겨 되돌릴 수 있게 한다.
#[tauri::command]
fn save_file_to_path(
    state: State<'_, AppState>,
    project_id: Option<String>,
    absolute_path: String,
    data_base64: String,
) -> Result<SaveResult, String> {
    let target = PathBuf::from(&absolute_path);
    if !target.is_absolute() {
        return Err("저장 경로가 올바르지 않습니다.".to_string());
    }
    let bytes = doc_bridge::decode_base64(&data_base64)?;
    if let Some(pid) = project_id.filter(|p| p != GENERAL_CHAT_ID) {
        let (root, pid) = project_root(&state, &pid)?;
        if let Some(rel) = relative_to_root(&root, &target) {
            let snap = fs_ops::write_bytes_with_snapshot(&root, &pid, &rel, &bytes, "write")?;
            push_snapshot(&state, &snap)?;
            return Ok(SaveResult { absolute_path, relative_path: Some(rel), snapshot: Some(snap) });
        }
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("폴더를 만들 수 없습니다: {}", e))?;
    }
    std::fs::write(&target, bytes).map_err(|e| format!("파일 저장 실패: {}", e))?;
    Ok(SaveResult { absolute_path, relative_path: None, snapshot: None })
}

/// Windows 저장 대화상자. `filter` 예: "Word 문서 (*.docx)|*.docx"
#[tauri::command]
async fn pick_save_path_dialog(
    initial_dir: Option<String>,
    default_name: String,
    filter: String,
) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let esc = |s: &str| s.replace('\'', "''");
            let init = esc(&initial_dir.unwrap_or_default());
            let script = format!(
                r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.SaveFileDialog
                $dlg.Title = '다른 이름으로 저장'
                $dlg.FileName = '{name}'
                $dlg.Filter = '{filter}'
                $dlg.OverwritePrompt = $true
                $dlg.AddExtension = $true
                if ('{init}' -ne '' -and (Test-Path '{init}')) {{
                    $dlg.InitialDirectory = '{init}'
                }}
                $res = $dlg.ShowDialog()
                if ($res -eq [System.Windows.Forms.DialogResult]::OK) {{
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    Write-Output $dlg.FileName
                }}
                "#,
                name = esc(&default_name),
                filter = esc(&filter),
                init = init
            );
            let out = Command::new("powershell")
                .args(["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command", &script])
                .output()
                .map_err(|e| format!("저장 창 실행 실패: {}", e))?;
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Ok(if path.is_empty() { None } else { Some(path) })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (initial_dir, default_name, filter);
            Ok(None)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 웹뷰 문서 변환기가 처리 결과를 돌려준다.
#[tauri::command]
fn doc_bridge_respond(
    bridge: State<'_, doc_bridge::DocBridge>,
    request_id: String,
    reply: doc_bridge::BridgeReply,
) -> Result<(), String> {
    bridge.respond(&request_id, reply)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("c:\\DOCS\\EXAONE\\.exaone_data"));
            fs_ops::set_blob_dir(app_data_dir.join("blobs"));
            let store = StoreManager::new(&app_data_dir);
            let initial_db = store.load();
            app.manage(AppState {
                store,
                db: Mutex::new(initial_db),
                cancel_flag: Arc::new(AtomicBool::new(false)),
            });
            app.manage(doc_bridge::DocBridge::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_initial_state,
            save_settings,
            pick_folder_dialog,
            pick_files_dialog,
            cancel_generation,
            create_or_update_project,
            delete_project,
            list_project_files,
            read_project_file,
            write_project_file,
            modify_project_file,
            create_project_folder,
            delete_project_path,
            undo_file_snapshot,
            create_conversation,
            delete_conversation,
            send_chat_message,
            edit_and_resend_message,
            resolve_tool_approval,
            run_manual_terminal_command,
            read_project_file_base64,
            read_absolute_file_base64,
            write_project_file_base64,
            save_file_to_path,
            pick_save_path_dialog,
            doc_bridge_respond,
        ])
        .run(tauri::generate_context!())
        .expect("error while running EXAONE Desktop application");
}
