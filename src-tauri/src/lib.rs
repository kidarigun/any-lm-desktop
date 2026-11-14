pub mod catalog;
pub mod doc_bridge;
pub mod exaone;
pub mod fs_ops;
pub mod mcp;
pub mod mcp_oauth;
pub mod python_runner;
pub mod runs;
pub mod runtime;
pub mod skills;
pub mod secrets;
pub mod single_instance;
pub mod snapshot_diff;
pub mod store;

use exaone::TurnRequest;
use fs_ops::FileNode;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::{
    AppDatabase, AppSettings, ChatMessage, Conversation, FileSnapshot, Project, StoreManager,
};
use tauri::{AppHandle, Manager, Runtime, State};
use uuid::Uuid;

/// 일반 채팅(프로젝트 폴더와 연결되지 않은 대화)의 project_id.
pub const GENERAL_CHAT_ID: &str = "general-chat";
/// 셸 명령 최대 실행 시간.
const SHELL_TIMEOUT: Duration = Duration::from_secs(120);

pub struct AppState {
    pub store: StoreManager,
    pub db: Mutex<AppDatabase>,
    /// 대화별 실행 (서로 다른 대화는 동시에 진행된다)
    pub runs: runs::RunRegistry,
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
            let granted = app
                .try_state::<PathGrants>()
                .map(|g| g.is_granted(Path::new(fpath)))
                .unwrap_or(false);
            if granted {
                std::fs::read(fpath).map_err(|e| e.to_string())
            } else {
                Err(not_granted(fpath))
            }
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

/// 설정 저장. `api_key`가 비어 있으면 기존 키를 그대로 둔다 (키 삭제는 `clear_api_key`).
#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    exaone::validate_base_url(&settings.base_url)?;
    let new_key = settings.api_key.trim().to_string();
    if !new_key.is_empty() {
        secrets::store_api_key(&new_key)?;
    }
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let api_key = if new_key.is_empty() { db.settings.api_key.clone() } else { new_key };
    db.settings = AppSettings { api_key_set: !api_key.is_empty(), api_key, ..settings };
    state.store.save(&db)?;
    // api_key는 직렬화되지 않으므로 화면에는 키가 전달되지 않는다.
    Ok(db.settings.clone())
}

#[tauri::command]
fn clear_api_key(state: State<'_, AppState>) -> Result<AppSettings, String> {
    secrets::delete_api_key()?;
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.settings.api_key.clear();
    db.settings.api_key_set = false;
    state.store.save(&db)?;
    Ok(db.settings.clone())
}

/// 자격 증명 관리자에서 키를 불러온다. 예전 버전이 설정 파일에 평문으로 남긴 키는 옮기고 파일에서 지운다.
fn load_api_key_into(store: &StoreManager, db: &mut AppDatabase) {
    let legacy = std::mem::take(&mut db.settings.api_key).trim().to_string();
    if !legacy.is_empty() {
        match secrets::store_api_key(&legacy) {
            // api_key는 직렬화되지 않으므로 다시 저장하면 파일에서 평문 키가 사라진다.
            Ok(()) => {
                if let Err(e) = store.save(db) {
                    eprintln!("설정 파일 정리 실패: {}", e);
                }
            }
            Err(e) => eprintln!("{}", e),
        }
        db.settings.api_key = legacy;
    } else {
        match secrets::load_api_key() {
            Ok(k) => db.settings.api_key = k.unwrap_or_default(),
            Err(e) => eprintln!("{}", e),
        }
    }
    db.settings.api_key_set = !db.settings.api_key.is_empty();
}

// ---------------------------------------------------------------------------
// 사용자가 대화상자에서 직접 고른 경로만 프로젝트 밖에서 읽고 쓸 수 있다.
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct PathGrants(Mutex<HashSet<String>>);

impl PathGrants {
    fn key(path: &Path) -> Option<String> {
        if !path.is_absolute() {
            return None;
        }
        let parent = std::fs::canonicalize(path.parent()?).ok()?;
        Some(parent.join(path.file_name()?).to_string_lossy().to_lowercase())
    }

    pub fn grant(&self, path: &str) {
        if let (Some(k), Ok(mut set)) = (Self::key(Path::new(path)), self.0.lock()) {
            set.insert(k);
        }
    }

    pub fn is_granted(&self, path: &Path) -> bool {
        match (Self::key(path), self.0.lock()) {
            (Some(k), Ok(set)) => set.contains(&k),
            _ => false,
        }
    }
}

fn not_granted(path: &str) -> String {
    format!(
        "보안 정책: 프로젝트 밖의 파일은 파일 선택 창에서 직접 고른 경우에만 열거나 저장할 수 있습니다. ({})",
        path
    )
}

/// Windows Forms 대화상자를 PowerShell로 띄운다.
/// 사용자 입력(경로·파일 이름)은 스크립트에 끼워 넣지 않고 환경 변수로 넘겨 명령 주입을 막는다.
#[cfg(target_os = "windows")]
fn run_dialog_script(script: &str, env: &[(&str, &str)]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new("powershell");
    // 설치판(콘솔 없는 앱)에서 검은 창이 잠깐 뜨지 않게 한다. 대화상자 자체는 그대로 보인다.
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.args(["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command", script]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| format!("대화상자 실행 실패: {}", e))?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[tauri::command]
async fn pick_folder_dialog(initial_dir: Option<String>) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            const SCRIPT: &str = r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.FolderBrowserDialog
                $dlg.Description = '프로젝트 폴더를 선택하세요'
                $dlg.UseDescriptionForTitle = $true
                $dlg.AutoUpgradeEnabled = $true
                $init = $env:EXAONE_DLG_INIT
                if ($init -and (Test-Path -LiteralPath $init)) { $dlg.SelectedPath = $init }
                if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    Write-Output $dlg.SelectedPath
                }
            "#;
            let init = initial_dir.unwrap_or_default();
            let path = run_dialog_script(SCRIPT, &[("EXAONE_DLG_INIT", &init)])?.trim().to_string();
            Ok(if path.is_empty() { None } else { Some(path) })
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

/// 고른 파일은 프로젝트 밖에 있어도 이번 실행 동안 읽고(첨부·열기) 저장할 수 있게 허용한다.
#[tauri::command]
async fn pick_files_dialog(
    grants: State<'_, PathGrants>,
    initial_dir: Option<String>,
) -> Result<Vec<String>, String> {
    let files: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
        #[cfg(target_os = "windows")]
        {
            const SCRIPT: &str = r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.OpenFileDialog
                $dlg.Title = '파일을 선택하세요'
                $dlg.Multiselect = $true
                $init = $env:EXAONE_DLG_INIT
                if ($init -and (Test-Path -LiteralPath $init)) { $dlg.InitialDirectory = $init }
                if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    foreach ($f in $dlg.FileNames) { Write-Output $f }
                }
            "#;
            let init = initial_dir.unwrap_or_default();
            Ok(run_dialog_script(SCRIPT, &[("EXAONE_DLG_INIT", &init)])?
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = initial_dir;
            Ok(Vec::new())
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    for f in &files {
        grants.grant(f);
    }
    Ok(files)
}

/// 답변 생성을 멈춘다. 대화를 지정하지 않으면 실행 중인 모든 대화를 멈춘다.
#[tauri::command]
fn cancel_generation(state: State<'_, AppState>, conversation_id: Option<String>) -> Result<(), String> {
    match conversation_id {
        Some(id) => {
            state.runs.cancel(&id);
        }
        None => state.runs.cancel_all(),
    }
    Ok(())
}

/// 지금 답변을 만들고 있는 대화 ID들 (화면을 새로 고쳤을 때 상태를 되살린다)
#[tauri::command]
fn running_conversations(state: State<'_, AppState>) -> Vec<String> {
    state.runs.running()
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
    )?
    .with_source("user");
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
    )?
    .with_source("user");
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
    )?
    .with_source("user");
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

/// 변경 보기: 스냅샷 하나의 이전·이후 내용 (문서는 Markdown으로 변환)
#[tauri::command]
async fn get_snapshot_diff(
    app: AppHandle,
    state: State<'_, AppState>,
    snapshot_id: String,
) -> Result<snapshot_diff::SnapshotDiff, String> {
    let (snap, later, root) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let snap = db
            .snapshots
            .iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| "변경 기록을 찾을 수 없습니다.".to_string())?
            .clone();
        let later = snapshot_diff::next_change_of_same_file(&db.snapshots, &snap).cloned();
        let root = db
            .projects
            .iter()
            .find(|p| p.id == snap.project_id)
            .map(|p| PathBuf::from(&p.root_path));
        (snap, later, root)
    };
    snapshot_diff::build_snapshot_diff(&app, &snap, later.as_ref(), root.as_deref()).await
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
    state.runs.cancel(&conversation_id);
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
    let run = state.runs.begin(&conversation_id)?;
    let cancel_flag = run.cancel_flag();

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
    let run = state.runs.begin(&conversation_id)?;
    let cancel_flag = run.cancel_flag();

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
    always: Option<bool>,
) -> Result<Conversation, String> {
    let run = state.runs.begin(&conversation_id)?;
    // 1) 대상 기록 확인 (잠금은 짧게만 잡는다)
    let (project, tool_name, args) = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let conv = db
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| "대화를 찾을 수 없습니다.".to_string())?;
        let project = conversation_project(&db, conv)?;
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
        if project.is_none() && !rec.tool_name.starts_with(mcp::TOOL_PREFIX) {
            return Err("일반 채팅에서는 이 도구 승인을 처리할 수 없습니다.".to_string());
        }
        // 실행 중 중복 클릭을 막기 위해 바로 상태를 바꿔 둔다.
        rec.status = "running".to_string();
        let args: Value = serde_json::from_str(&rec.arguments_json).unwrap_or_default();
        (project, rec.tool_name.clone(), args)
    };

    // 2) 잠금 밖에서 실제 작업 실행
    let project_id = project.as_ref().map(|p| p.id.clone()).unwrap_or_default();
    let root = project.as_ref().map(|p| PathBuf::from(&p.root_path)).unwrap_or_default();
    let mut new_snap: Option<FileSnapshot> = None;
    let mut produced: Vec<String> = Vec::new();
    let (status, output) = if !approve {
        ("rejected", "사용자가 실행을 거부했습니다.".to_string())
    } else {
        match tool_name.as_str() {
            name if name.starts_with(mcp::TOOL_PREFIX) => {
                let mgr = app.state::<Arc<mcp::McpManager>>().inner().clone();
                match mgr.tool_specs().await.into_iter().find(|t| t.exposed == name) {
                    Some(spec) => {
                        if always == Some(true) {
                            let _ = mgr.set_tool(&spec.server_id, &spec.tool, None, Some(true));
                        }
                        match mgr.call(&spec.server_id, &spec.tool, &args).await {
                            Ok((text, false)) => ("completed", text),
                            Ok((text, true)) => ("error", text),
                            Err(e) => ("error", e),
                        }
                    }
                    None => ("error", format!("'{}' 도구는 지금 연결된 MCP 서버에 없습니다.", name)),
                }
            }
            "delete_path" => {
                let rel = args.get("relative_path").and_then(|v| v.as_str()).unwrap_or("");
                match fs_ops::delete_path_with_snapshot(&root, &project_id, rel) {
                    Ok(snap) => {
                        new_snap = Some(snap.with_source("agent"));
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
            "run_python" => {
                let rel = args.get("relative_path").and_then(|v| v.as_str()).unwrap_or("");
                let skill = args.get("skill").and_then(|v| v.as_str()).unwrap_or("").trim();
                let run = if skill.is_empty() {
                    python_runner::run_script(&root, rel, python_runner::RUN_TIMEOUT).await
                } else {
                    // Skill 폴더의 스크립트: 실행 위치는 프로젝트 루트
                    let ctx = Some((project_id.as_str(), root.as_path()));
                    match skills::find_available(ctx, skill)
                        .ok_or_else(|| format!("'{}' Skill이 없거나 꺼져 있습니다.", skill))
                        .and_then(|s| skills::resolve_in_skill(&s, rel))
                    {
                        Ok(script) => {
                            let display = format!("[Skill {}] {}", skill, rel.replace('\\', "/"));
                            python_runner::run_script_at(&root, &script, &display, python_runner::RUN_TIMEOUT).await
                        }
                        Err(e) => Err(e),
                    }
                };
                match run {
                    Ok(outcome) => {
                        produced = outcome.produced_files();
                        (if outcome.succeeded() { "completed" } else { "error" }, outcome.summary())
                    }
                    Err(e) => ("error", e),
                }
            }
            // 보호된 설정 폴더(.git, .vscode 등)에 대한 쓰기
            "write_file" | "modify_file" => {
                let (rec, snap, _) =
                    exaone::execute_tool_now(&app, &root, &project_id, &tool_id, &tool_name, &args).await;
                new_snap = snap;
                if rec.status == "completed" {
                    ("completed", format!("승인 완료: {}", rec.output))
                } else {
                    ("error", rec.output)
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
                rec.produced_files = produced;
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
    let (updated_msg, new_snapshots) = exaone::run_exaone_turn(
        &app,
        TurnRequest {
            settings: &settings,
            project: project.as_ref(),
            conversation_id: &conversation_id,
            history: &history,
            continue_from: Some(assistant_msg),
        },
        run.cancel_flag(),
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
fn read_absolute_file_base64(grants: State<'_, PathGrants>, path: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 200 * 1024 * 1024;
    let p = PathBuf::from(&path);
    if !p.is_absolute() || !p.is_file() {
        return Err(format!("파일을 찾을 수 없습니다: {}", path));
    }
    if !grants.is_granted(&p) {
        return Err(not_granted(&path));
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
    let snap = fs_ops::write_bytes_with_snapshot(&root, &pid, &relative_path, &bytes, "write")?.with_source("user");
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
    grants: State<'_, PathGrants>,
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
            let snap = fs_ops::write_bytes_with_snapshot(&root, &pid, &rel, &bytes, "write")?.with_source("user");
            push_snapshot(&state, &snap)?;
            return Ok(SaveResult { absolute_path, relative_path: Some(rel), snapshot: Some(snap) });
        }
    }
    if !grants.is_granted(&target) {
        return Err(not_granted(&absolute_path));
    }
    std::fs::write(&target, bytes).map_err(|e| format!("파일 저장 실패: {}", e))?;
    Ok(SaveResult { absolute_path, relative_path: None, snapshot: None })
}

/// Windows 저장 대화상자. `filter` 예: "Word 문서 (*.docx)|*.docx"
/// 고른 경로는 이번 실행 동안 저장할 수 있게 허용한다.
#[tauri::command]
async fn pick_save_path_dialog(
    grants: State<'_, PathGrants>,
    initial_dir: Option<String>,
    default_name: String,
    filter: String,
) -> Result<Option<String>, String> {
    let picked = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        #[cfg(target_os = "windows")]
        {
            const SCRIPT: &str = r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.SaveFileDialog
                $dlg.Title = '다른 이름으로 저장'
                $dlg.FileName = $env:EXAONE_DLG_NAME
                try { $dlg.Filter = $env:EXAONE_DLG_FILTER } catch { $dlg.Filter = '모든 파일 (*.*)|*.*' }
                $dlg.OverwritePrompt = $true
                $dlg.AddExtension = $true
                $init = $env:EXAONE_DLG_INIT
                if ($init -and (Test-Path -LiteralPath $init)) { $dlg.InitialDirectory = $init }
                if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    Write-Output $dlg.FileName
                }
            "#;
            let init = initial_dir.unwrap_or_default();
            let path = run_dialog_script(
                SCRIPT,
                &[
                    ("EXAONE_DLG_INIT", &init),
                    ("EXAONE_DLG_NAME", &default_name),
                    ("EXAONE_DLG_FILTER", &filter),
                ],
            )?
            .trim()
            .to_string();
            Ok(if path.is_empty() { None } else { Some(path) })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (initial_dir, default_name, filter);
            Ok(None)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some(p) = &picked {
        grants.grant(p);
    }
    Ok(picked)
}

// ---------------------------------------------------------------------------
// 파일 탐색기
// ---------------------------------------------------------------------------

/// 폴더 하나의 바로 아래 항목 (빈 문자열이면 프로젝트 루트)
#[tauri::command]
fn list_project_dir(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<Vec<FileNode>, String> {
    let (root, _) = project_root(&state, &project_id)?;
    fs_ops::list_dir_children(&root, &relative_path)
}

#[tauri::command]
fn create_project_file(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<String, String> {
    let (root, _) = project_root(&state, &project_id)?;
    fs_ops::create_empty_file(&root, &relative_path)
}

#[tauri::command]
fn rename_project_path(
    state: State<'_, AppState>,
    project_id: String,
    from: String,
    to: String,
) -> Result<String, String> {
    let (root, _) = project_root(&state, &project_id)?;
    fs_ops::rename_path(&root, &from, &to)
}

/// 사용자가 탐색기에서 지운 항목은 Windows 휴지통으로 보낸다.
#[tauri::command]
async fn trash_project_path(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<(), String> {
    let (root, _) = project_root(&state, &project_id)?;
    tokio::task::spawn_blocking(move || fs_ops::move_to_trash(&root, &relative_path))
        .await
        .map_err(|e| e.to_string())?
}

/// @멘션용 파일 이름 검색
#[tauri::command]
async fn find_project_files(
    state: State<'_, AppState>,
    project_id: String,
    query: String,
) -> Result<Vec<String>, String> {
    let (root, _) = project_root(&state, &project_id)?;
    tokio::task::spawn_blocking(move || fs_ops::find_files_by_name(&root, &query, 30))
        .await
        .map_err(|e| e.to_string())
}

/// Windows 탐색기에서 해당 파일을 선택한 상태로 연다.
#[tauri::command]
fn reveal_project_path(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<(), String> {
    let (root, _) = project_root(&state, &project_id)?;
    let target = fs_ops::resolve_safe_path(&root, &relative_path)?;
    #[cfg(target_os = "windows")]
    {
        let arg = if target.is_dir() && relative_path.is_empty() {
            target.to_string_lossy().to_string()
        } else {
            format!("/select,{}", target.to_string_lossy())
        };
        Command::new("explorer")
            .arg(arg)
            .spawn()
            .map_err(|e| format!("탐색기를 열 수 없습니다: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    let _ = target;
    Ok(())
}

/// 저장 창에서 고른(허용된) 파일을 탐색기에서 선택한 상태로 연다.
#[tauri::command]
fn reveal_saved_path(grants: State<'_, PathGrants>, path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if !grants.is_granted(&target) {
        return Err(not_granted(&path));
    }
    if !target.is_file() {
        return Err(format!("파일을 찾을 수 없습니다: {}", path));
    }
    #[cfg(target_os = "windows")]
    Command::new("explorer")
        .arg(format!("/select,{}", target.to_string_lossy()))
        .spawn()
        .map_err(|e| format!("탐색기를 열 수 없습니다: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 실행 환경 (Python·Node.js·uv)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn runtime_status() -> Vec<runtime::RuntimeInfo> {
    runtime::status().await
}

/// 공식 배포본을 내려받아 앱 전용으로 설치한다 (끝날 때까지 기다린다. 진행 상황은 runtime_status로).
#[tauri::command]
async fn runtime_install(kind: String) -> Result<(), String> {
    runtime::install(&kind).await
}

#[tauri::command]
async fn runtime_remove(kind: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || runtime::remove(&kind)).await.map_err(|e| e.to_string())?
}

// ---------------------------------------------------------------------------
// 찾아보기 목록
// ---------------------------------------------------------------------------

/// 찾아보기 목록을 불러온다. 주소를 주지 않으면 기본 목록.
#[tauri::command]
async fn catalog_load(app: AppHandle, url: Option<String>) -> Result<catalog::CatalogResult, String> {
    let url = url.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| catalog::DEFAULT_CATALOG_URL.to_string());
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    catalog::load(url.trim(), &dir).await
}

#[tauri::command]
fn catalog_default_url() -> String {
    catalog::DEFAULT_CATALOG_URL.to_string()
}

/// 목록의 Skill을 받아 sha256을 확인하고 설치한다.
#[tauri::command]
async fn catalog_install_skill(url: String, sha256: String, overwrite: bool) -> Result<skills::SkillInfo, String> {
    catalog::install_skill(&url, &sha256, overwrite).await
}

/// Node.js·uv·Python·Docker가 이 PC에 있는지
#[tauri::command]
async fn catalog_runtimes() -> catalog::Runtimes {
    catalog::runtimes().await
}

// ---------------------------------------------------------------------------
// MCP 서버 관리
// ---------------------------------------------------------------------------

type McpState<'a> = State<'a, Arc<mcp::McpManager>>;

#[tauri::command]
fn mcp_list_servers(mcp: McpState<'_>) -> Vec<mcp::ServerView> {
    mcp.list()
}

/// 서버를 추가하거나 고친다. `env`·`headers`의 빈 값은 저장된 값을 유지한다.
#[tauri::command]
fn mcp_save_server(
    mcp: McpState<'_>,
    config: mcp::McpServerConfig,
    env: HashMap<String, String>,
    headers: HashMap<String, String>,
    oauth_client_secret: Option<String>,
) -> Result<mcp::McpServerConfig, String> {
    mcp.save(config, env, headers, oauth_client_secret)
}

/// 브라우저로 로그인한다 (끝날 때까지 기다린다, 최대 5분).
#[tauri::command]
async fn mcp_oauth_login(mcp: McpState<'_>, id: String) -> Result<(), String> {
    let m = mcp.inner().clone();
    m.login(&id).await
}

#[tauri::command]
fn mcp_oauth_cancel(mcp: McpState<'_>) {
    mcp.cancel_login()
}

#[tauri::command]
fn mcp_oauth_logout(mcp: McpState<'_>, id: String) -> Result<(), String> {
    mcp.logout(&id)
}

#[tauri::command]
fn mcp_delete_server(mcp: McpState<'_>, id: String) -> Result<(), String> {
    mcp.delete(&id)
}

#[tauri::command]
fn mcp_set_server_enabled(mcp: McpState<'_>, id: String, enabled: bool) -> Result<(), String> {
    mcp.set_enabled(&id, enabled)
}

#[tauri::command]
fn mcp_restart_server(mcp: McpState<'_>, id: String) {
    mcp.restart(&id)
}

#[tauri::command]
fn mcp_set_tool(
    mcp: McpState<'_>,
    id: String,
    tool: String,
    enabled: Option<bool>,
    auto_approve: Option<bool>,
) -> Result<(), String> {
    mcp.set_tool(&id, &tool, enabled, auto_approve)
}

// ---------------------------------------------------------------------------
// Skill 관리
// ---------------------------------------------------------------------------

/// 프로젝트 ID가 있으면 (ID, 루트)를 돌려준다.
fn skill_project(state: &AppState, project_id: Option<&str>) -> Result<Option<(String, PathBuf)>, String> {
    match project_id {
        Some(pid) if !pid.is_empty() && pid != GENERAL_CHAT_ID => {
            let (root, id) = project_root(state, pid)?;
            Ok(Some((id, root)))
        }
        _ => Ok(None),
    }
}

#[tauri::command]
async fn list_skills(state: State<'_, AppState>, project_id: Option<String>) -> Result<Vec<skills::SkillInfo>, String> {
    let proj = skill_project(&state, project_id.as_deref())?;
    tokio::task::spawn_blocking(move || skills::discover(proj.as_ref().map(|(id, root)| (id.as_str(), root.as_path()))))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_skill_enabled(id: String, enabled: bool) -> Result<(), String> {
    skills::set_enabled(&id, enabled)
}

/// Skill로 설치할 폴더 또는 zip(.skill) 파일을 고른다. 고른 경로만 설치할 수 있게 허용한다.
#[tauri::command]
async fn pick_skill_source(grants: State<'_, PathGrants>, kind: String) -> Result<Option<String>, String> {
    let picked: Option<String> = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        #[cfg(target_os = "windows")]
        {
            const FOLDER: &str = r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.FolderBrowserDialog
                $dlg.Description = 'SKILL.md가 든 Skill 폴더를 선택하세요'
                $dlg.UseDescriptionForTitle = $true
                $dlg.AutoUpgradeEnabled = $true
                if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    Write-Output $dlg.SelectedPath
                }
            "#;
            const ZIP: &str = r#"
                Add-Type -AssemblyName System.Windows.Forms
                $dlg = New-Object System.Windows.Forms.OpenFileDialog
                $dlg.Title = 'Skill 파일(.zip, .skill)을 선택하세요'
                $dlg.Filter = 'Skill 파일 (*.zip;*.skill)|*.zip;*.skill'
                if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    Write-Output $dlg.FileName
                }
            "#;
            let script = if kind == "folder" { FOLDER } else { ZIP };
            let path = run_dialog_script(script, &[])?.trim().to_string();
            Ok((!path.is_empty()).then_some(path))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = kind;
            Ok(None)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some(p) = &picked {
        grants.grant(p);
    }
    Ok(picked)
}

/// 고른 폴더나 zip 파일을 모든 대화용 Skill로 설치한다.
/// 같은 Skill이 있으면 `ALREADY_EXISTS:<이름>` 오류를 돌려주고, `overwrite`면 기존 것을 휴지통으로 옮긴다.
#[tauri::command]
async fn install_skill(
    grants: State<'_, PathGrants>,
    source_path: String,
    overwrite: bool,
) -> Result<skills::SkillInfo, String> {
    let src = PathBuf::from(&source_path);
    if !grants.is_granted(&src) {
        return Err(not_granted(&source_path));
    }
    tokio::task::spawn_blocking(move || {
        if src.is_dir() {
            skills::install_from_folder(&src, overwrite)
        } else {
            skills::install_from_zip(&src, overwrite)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn delete_skill(id: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || skills::delete_user_skill(&id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn create_skill(name: String) -> Result<skills::SkillInfo, String> {
    skills::create_user_skill(&name)
}

/// SKILL.md 원문 (미리보기)
#[tauri::command]
fn skill_text(state: State<'_, AppState>, id: String, project_id: Option<String>) -> Result<String, String> {
    let proj = skill_project(&state, project_id.as_deref())?;
    skills::skill_md_text(&id, proj.as_ref().map(|(i, r)| (i.as_str(), r.as_path())))
}

/// Skill 폴더를 탐색기로 열거나(`target`="folder"), SKILL.md를 메모장으로 연다(`target`="edit").
/// 아무 Skill도 고르지 않으면(`id`가 비면) 모든 대화용 Skill 폴더를 연다.
#[tauri::command]
fn open_skill_location(
    state: State<'_, AppState>,
    id: Option<String>,
    project_id: Option<String>,
    target: String,
) -> Result<(), String> {
    let dir = match id.as_deref() {
        Some(id) if !id.is_empty() => {
            let proj = skill_project(&state, project_id.as_deref())?;
            skills::dir_of(id, proj.as_ref().map(|(i, r)| (i.as_str(), r.as_path())))
                .ok_or_else(|| "Skill을 찾을 수 없습니다.".to_string())?
        }
        _ => {
            let d = skills::user_dir();
            std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
            d
        }
    };
    #[cfg(target_os = "windows")]
    {
        if target == "edit" {
            let file = std::fs::read_dir(&dir)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("SKILL.md")))
                .ok_or_else(|| "SKILL.md가 없습니다.".to_string())?;
            Command::new("notepad.exe")
                .arg(&file)
                .spawn()
                .map_err(|e| format!("메모장을 열 수 없습니다: {}", e))?;
        } else {
            Command::new("explorer")
                .arg(&dir)
                .spawn()
                .map_err(|e| format!("탐색기를 열 수 없습니다: {}", e))?;
        }
    }
    let _ = (dir, target);
    Ok(())
}

/// 이 프로젝트에서 쓸 Python (없으면 null)
#[tauri::command]
async fn python_status(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Option<python_runner::PythonInfo>, String> {
    let (root, _) = project_root(&state, &project_id)?;
    Ok(python_runner::find_python(Some(&root)).await)
}

/// 사용자가 실행 창에서 [실행]을 눌렀을 때: 프로젝트 안의 .py를 프로젝트 루트에서 실행한다.
#[tauri::command]
async fn run_python_script(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<python_runner::RunOutcome, String> {
    let (root, _) = project_root(&state, &project_id)?;
    python_runner::run_script(&root, &relative_path, python_runner::RUN_TIMEOUT).await
}

/// 실행에 필요한 패키지를 pip으로 설치한다 (사용자가 버튼을 눌렀을 때만).
#[tauri::command]
async fn install_python_package(
    state: State<'_, AppState>,
    project_id: String,
    package: String,
) -> Result<String, String> {
    let (root, _) = project_root(&state, &project_id)?;
    python_runner::install_package(Some(&root), &package).await
}

/// 기본 앱으로 열어도 되는 파일 (실행 파일·스크립트는 제외)
const OPENABLE_EXTENSIONS: &[&str] = &[
    "xlsx", "xls", "csv", "tsv", "docx", "doc", "pptx", "ppt", "pdf", "hwp", "hwpx", "txt", "md", "json", "xml",
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "mp3", "wav", "mp4", "zip",
];

pub fn is_openable_with_default_app(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| OPENABLE_EXTENSIONS.contains(&e.as_str()))
}

/// 프로젝트 파일을 Windows 기본 앱(엑셀 등)으로 연다. 문서·이미지·데이터 파일만 허용한다.
#[tauri::command]
fn open_project_file_external(
    state: State<'_, AppState>,
    project_id: String,
    relative_path: String,
) -> Result<(), String> {
    let (root, _) = project_root(&state, &project_id)?;
    let target = fs_ops::resolve_safe_path(&root, &relative_path)?;
    if !target.is_file() {
        return Err(format!("파일을 찾을 수 없습니다: {}", relative_path));
    }
    if !is_openable_with_default_app(&target) {
        return Err("이 형식은 기본 앱으로 열 수 없습니다. 탐색기에서 직접 여세요.".to_string());
    }
    #[cfg(target_os = "windows")]
    Command::new("explorer")
        .arg(&target)
        .spawn()
        .map_err(|e| format!("파일을 열 수 없습니다: {}", e))?;
    Ok(())
}

/// 채팅 답변의 링크를 기본 브라우저로 연다. http(s)·mailto만 허용한다.
#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| "올바른 링크가 아닙니다.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Err(format!("열 수 없는 링크 형식입니다: {}", parsed.scheme()));
    }
    #[cfg(target_os = "windows")]
    Command::new("explorer")
        .arg(parsed.as_str())
        .spawn()
        .map_err(|e| format!("브라우저를 열 수 없습니다: {}", e))?;
    Ok(())
}

/// 앱 화면이 외부 사이트로 이동하지 못하게 한다 (외부 링크는 open_external_url로 브라우저에서 연다).
pub fn is_app_url(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") => true,
            Some("localhost") => cfg!(debug_assertions) && url.port() == Some(1420),
            _ => false,
        },
        _ => false,
    }
}

fn navigation_guard<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("navigation-guard")
        .on_navigation(|_webview, url| {
            let ok = is_app_url(url);
            if !ok {
                eprintln!("차단한 화면 이동: {}", url);
            }
            ok
        })
        .build()
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
/// 예전 앱 ID의 데이터 폴더를 새 폴더로 옮긴다. 새 폴더가 이미 있으면 건드리지 않는다.
fn migrate_old_data_dir(new_dir: &Path) {
    const OLD_ID: &str = "ai.exaone.desktop";
    let Some(parent) = new_dir.parent() else { return };
    let old = parent.join(OLD_ID);
    if new_dir.exists() || !old.is_dir() || old == new_dir {
        return;
    }
    if let Err(e) = std::fs::rename(&old, new_dir) {
        eprintln!("이전 데이터 폴더를 옮기지 못했습니다 ({}): {}", old.display(), e);
        return;
    }
    // 앱 전용 Python 가상환경은 기반 Python의 절대 경로를 적어 두므로 새 경로로 고친다.
    let cfg = new_dir.join("runtimes").join("pyenv").join("pyvenv.cfg");
    if let Ok(text) = std::fs::read_to_string(&cfg) {
        let fixed = text.replace(&old.to_string_lossy().to_string(), &new_dir.to_string_lossy());
        let _ = std::fs::write(&cfg, fixed);
    }
}

pub fn run() {
    // 이미 실행 중이면 새로 띄우지 않고 기존 창을 앞으로 가져온다 (저장 파일·로그인 포트 충돌 방지).
    let mut instance = match single_instance::acquire() {
        single_instance::Instance::AlreadyRunning => return,
        single_instance::Instance::First(l) => Some(l),
        single_instance::Instance::Unknown => None,
    };
    tauri::Builder::default()
        .plugin(navigation_guard())
        .setup(move |app| {
            if let Some(listener) = instance.take() {
                let handle = app.handle().clone();
                single_instance::listen(listener, move || {
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                });
            }
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("k-exaone-desktop-data"));
            // 앱 ID를 바꾸기 전(ai.exaone.desktop) 폴더에 있던 대화·설정·실행 환경을 한 번 옮겨 온다.
            migrate_old_data_dir(&app_data_dir);
            if let Ok(local) = app.path().app_local_data_dir() {
                migrate_old_data_dir(&local);
            }
            fs_ops::set_blob_dir(app_data_dir.join("blobs"));
            skills::set_user_dir(app_data_dir.join("skills"));
            // 실행 환경(Python·Node.js·uv)은 크므로 로밍되지 않는 로컬 앱 데이터에 둔다.
            runtime::set_root(
                app.path()
                    .app_local_data_dir()
                    .unwrap_or_else(|_| app_data_dir.clone())
                    .join("runtimes"),
            );
            let mcp_manager = mcp::McpManager::new(app_data_dir.join("mcp_servers.json"), Box::new(mcp::KeyringSecrets));
            mcp_manager.start_all();
            app.manage(mcp_manager);
            // 처음 실행할 때 만드는 기본 작업 폴더: 문서\EXAONE (설치 폴더는 쓸 수 없는 곳일 수 있다)
            let default_workspace = app
                .path()
                .document_dir()
                .map(|d| d.join("EXAONE"))
                .unwrap_or_else(|_| app_data_dir.join("workspace"));
            let store = StoreManager::new(&app_data_dir).with_default_workspace(default_workspace);
            let mut initial_db = store.load();
            load_api_key_into(&store, &mut initial_db);
            app.manage(AppState {
                store,
                db: Mutex::new(initial_db),
                runs: runs::RunRegistry::default(),
            });
            app.manage(doc_bridge::DocBridge::default());
            app.manage(PathGrants::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_initial_state,
            save_settings,
            clear_api_key,
            open_external_url,
            pick_folder_dialog,
            pick_files_dialog,
            cancel_generation,
            running_conversations,
            create_or_update_project,
            delete_project,
            list_project_files,
            read_project_file,
            write_project_file,
            modify_project_file,
            create_project_folder,
            delete_project_path,
            undo_file_snapshot,
            get_snapshot_diff,
            python_status,
            run_python_script,
            install_python_package,
            list_skills,
            set_skill_enabled,
            pick_skill_source,
            install_skill,
            delete_skill,
            create_skill,
            skill_text,
            open_skill_location,
            runtime_status,
            runtime_install,
            runtime_remove,
            catalog_load,
            catalog_default_url,
            catalog_install_skill,
            catalog_runtimes,
            mcp_list_servers,
            mcp_save_server,
            mcp_delete_server,
            mcp_set_server_enabled,
            mcp_restart_server,
            mcp_set_tool,
            mcp_oauth_login,
            mcp_oauth_cancel,
            mcp_oauth_logout,
            open_project_file_external,
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
            list_project_dir,
            create_project_file,
            rename_project_path,
            trash_project_path,
            find_project_files,
            reveal_project_path,
            reveal_saved_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Desktop App for LG K-EXAONE")
        .run(|app, event| {
            // 앱을 끝낼 때 로컬 MCP 서버 프로세스도 함께 끝낸다.
            if let tauri::RunEvent::Exit = event {
                if let Some(m) = app.try_state::<Arc<mcp::McpManager>>() {
                    m.shutdown_all();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_documents_open_with_the_default_app() {
        for ok in ["out/report.xlsx", "a.CSV", "chart.png", "보고서.docx"] {
            assert!(is_openable_with_default_app(Path::new(ok)), "{}", ok);
        }
        for bad in ["make.py", "run.bat", "x.exe", "a.ps1", "b.vbs", "c.lnk", "d.js", "e.cmd", "f.html", "noext"] {
            assert!(!is_openable_with_default_app(Path::new(bad)), "{}", bad);
        }
    }

    #[test]
    fn navigation_is_limited_to_the_app() {
        let ok = |u: &str| is_app_url(&tauri::Url::parse(u).unwrap());
        assert!(ok("tauri://localhost/index.html"));
        assert!(ok("http://tauri.localhost/"));
        assert!(!ok("https://evil.example.com/"));
        assert!(!ok("http://tauri.localhost.evil.com/"));
        assert!(!ok("file:///C:/Windows/win.ini"));
        assert!(!ok("javascript:alert(1)"));
        assert_eq!(ok("http://localhost:1420/"), cfg!(debug_assertions));
        assert!(!ok("http://localhost:9999/"));
    }

    #[test]
    fn only_dialog_picked_paths_are_granted() {
        let dir = tempfile::tempdir().unwrap();
        let picked = dir.path().join("보고서.docx");
        std::fs::write(&picked, b"x").unwrap();
        let other = dir.path().join("other.txt");

        let grants = PathGrants::default();
        assert!(!grants.is_granted(&picked));
        grants.grant(&picked.to_string_lossy());
        assert!(grants.is_granted(&picked));
        // 대소문자·다른 표기도 같은 파일로 본다.
        let upper = PathBuf::from(picked.to_string_lossy().to_uppercase());
        assert!(grants.is_granted(&upper));
        assert!(!grants.is_granted(&other));
        assert!(!grants.is_granted(Path::new("보고서.docx")));
        // 상위 폴더를 거쳐 돌아오는 경로도 정규화된다.
        let dotted = dir.path().join("sub").join("..").join("보고서.docx");
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        assert!(grants.is_granted(&dotted));
    }
}
