pub mod exaone;
pub mod fs_ops;
pub mod store;

use fs_ops::FileNode;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use store::{
    AppDatabase, AppSettings, ChatMessage, Conversation, FileSnapshot, Project, StoreManager,
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

pub struct AppState {
    pub store: StoreManager,
    pub db: Mutex<AppDatabase>,
    pub cancel_flag: Arc<AtomicBool>,
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

    let (settings, project, history_for_api) = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db.settings.clone();
        let conv_idx = db
            .conversations
            .iter()
            .position(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;

        let project_id = db.conversations[conv_idx].project_id.clone();
        let project = if project_id == "general-chat" {
            Project {
                id: "general-chat".to_string(),
                name: "일반 채팅".to_string(),
                root_path: std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("c:\\DOCS\\EXAONE"))
                    .to_string_lossy()
                    .to_string(),
                instructions: String::new(),
                created_at: String::new(),
            }
        } else {
            db.projects
                .iter()
                .find(|p| p.id == project_id)
                .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?
                .clone()
        };

        let mut full_content = user_content.clone();
        if !attached_files.is_empty() {
            let root = Path::new(&project.root_path);
            let mut attachments_block = String::from("\n\n---\n[첨부 파일 내용]\n");
            for fpath in &attached_files {
                let text_res = if Path::new(fpath).is_absolute() {
                    std::fs::read_to_string(fpath).map_err(|e| e.to_string())
                } else {
                    fs_ops::read_file_content(root, fpath)
                };
                if let Ok(file_text) = text_res {
                    let display_name = Path::new(fpath)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| fpath.clone());
                    attachments_block.push_str(&format!(
                        "\n### 파일: `{}`\n```\n{}\n```\n",
                        display_name, file_text
                    ));
                }
            }
            full_content.push_str(&attachments_block);
        }

        let user_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: full_content,
            reasoning_content: None,
            tool_records: Vec::new(),
            created_at: chrono::Local::now().to_rfc3339(),
        };

        let conv = &mut db.conversations[conv_idx];
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
        &settings,
        &project,
        &conversation_id,
        &history_for_api,
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

    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.snapshots.extend(new_snapshots);
    let conv = db
        .conversations
        .iter_mut()
        .find(|c| c.id == conversation_id)
        .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;
    conv.messages.push(assistant_msg);
    conv.updated_at = chrono::Local::now().to_rfc3339();
    let updated_conv = conv.clone();
    state.store.save(&db)?;

    Ok(updated_conv)
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

        let project_id = db.conversations[conv_idx].project_id.clone();
        let project = if project_id == "general-chat" {
            Project {
                id: "general-chat".to_string(),
                name: "일반 채팅".to_string(),
                root_path: std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("c:\\DOCS\\EXAONE"))
                    .to_string_lossy()
                    .to_string(),
                instructions: String::new(),
                created_at: String::new(),
            }
        } else {
            db.projects
                .iter()
                .find(|p| p.id == project_id)
                .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?
                .clone()
        };

        let conv = &mut db.conversations[conv_idx];
        let msg_pos = conv
            .messages
            .iter()
            .position(|m| m.id == message_id)
            .ok_or_else(|| "수정할 메시지를 찾을 수 없습니다.".to_string())?;

        let backup = conv.messages.clone();
        conv.messages.truncate(msg_pos);

        let updated_user_msg = ChatMessage {
            id: message_id,
            role: "user".to_string(),
            content: new_content.clone(),
            reasoning_content: None,
            tool_records: Vec::new(),
            created_at: chrono::Local::now().to_rfc3339(),
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
        &settings,
        &project,
        &conversation_id,
        &history_for_api,
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

    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.snapshots.extend(new_snapshots);
    let conv = db
        .conversations
        .iter_mut()
        .find(|c| c.id == conversation_id)
        .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;
    conv.messages.push(assistant_msg);
    conv.updated_at = chrono::Local::now().to_rfc3339();
    let updated_conv = conv.clone();
    state.store.save(&db)?;

    Ok(updated_conv)
}

#[tauri::command]
fn resolve_tool_approval(
    state: State<'_, AppState>,
    conversation_id: String,
    message_id: String,
    tool_id: String,
    approve: bool,
) -> Result<Conversation, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let conv_idx = db
        .conversations
        .iter()
        .position(|c| c.id == conversation_id)
        .ok_or_else(|| "대화를 찾을 수 없습니다.".to_string())?;

    let project_id = db.conversations[conv_idx].project_id.clone();
    let project = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?
        .clone();

    let root = Path::new(&project.root_path);
    let mut new_snap: Option<FileSnapshot> = None;

    {
        let conv = &mut db.conversations[conv_idx];
        let msg = conv
            .messages
            .iter_mut()
            .find(|m| m.id == message_id)
            .ok_or_else(|| "메시지를 찾을 수 없습니다.".to_string())?;

        let rec = msg
            .tool_records
            .iter_mut()
            .find(|r| r.id == tool_id)
            .ok_or_else(|| "도구 실행 기록을 찾을 수 없습니다.".to_string())?;

        if !approve {
            rec.status = "rejected".to_string();
            rec.output = "사용자가 실행을 거부했습니다.".to_string();
        } else {
            let args: Value = serde_json::from_str(&rec.arguments_json).unwrap_or_default();
            match rec.tool_name.as_str() {
                "delete_path" => {
                    let rel = args
                        .get("relative_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    match fs_ops::delete_path_with_snapshot(root, &project.id, rel) {
                        Ok(snap) => {
                            rec.status = "completed".to_string();
                            rec.output = format!("승인 완료: '{}' 삭제됨", rel);
                            rec.snapshot_id = Some(snap.id.clone());
                            new_snap = Some(snap);
                        }
                        Err(e) => {
                            rec.status = "error".to_string();
                            rec.output = e;
                        }
                    }
                }
                "run_command" => {
                    let cmd = args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    match fs_ops::execute_shell_command(root, cmd) {
                        Ok(out) => {
                            rec.status = "completed".to_string();
                            rec.output = out;
                        }
                        Err(e) => {
                            rec.status = "error".to_string();
                            rec.output = e;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(s) = new_snap {
        db.snapshots.push(s);
    }
    let updated = db.conversations[conv_idx].clone();
    state.store.save(&db)?;
    Ok(updated)
}

#[tauri::command]
fn run_manual_terminal_command(
    state: State<'_, AppState>,
    project_id: String,
    command: String,
) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;

    fs_ops::execute_shell_command(Path::new(&proj.root_path), &command)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("c:\\DOCS\\EXAONE\\.exaone_data"));
            let store = StoreManager::new(&app_data_dir);
            let initial_db = store.load();
            app.manage(AppState {
                store,
                db: Mutex::new(initial_db),
                cancel_flag: Arc::new(AtomicBool::new(false)),
            });
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running EXAONE Desktop application");
}
