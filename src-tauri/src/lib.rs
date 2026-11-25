pub mod catalog;
pub mod context_budget;
pub mod doc_bridge;
pub mod agent;
mod dev_servers;
pub mod fs_ops;
pub mod html_preview;
pub mod ipc_guard;
pub mod lmstudio;
pub mod providers;
pub mod knowledge;
pub mod memory;
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
pub mod terminal;

#[cfg(test)]
mod harness;

use agent::TurnRequest;
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
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
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

/// 이 대화가 쓸 모델 key: 대화에 고른 모델 → 설정의 기본 모델 순.
fn conversation_model(settings: &AppSettings, conv: &Conversation) -> Result<String, String> {
    let pick = |s: &str| {
        let t = s.trim();
        (!t.is_empty()).then(|| t.to_string())
    };
    conv.model_key
        .as_deref()
        .and_then(pick)
        .or_else(|| pick(&settings.model))
        .ok_or_else(|| {
            format!(
                "사용할 모델을 고르세요. [{} 연결]에서 기본 모델을 정하거나 대화 위의 모델 선택에서 고를 수 있습니다.",
                settings.provider.label()
            )
        })
}

fn find_conv_mut<'a>(db: &'a mut AppDatabase, conversation_id: &str) -> Result<&'a mut Conversation, String> {
    db.conversations
        .iter_mut()
        .find(|c| c.id == conversation_id)
        .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())
}

/// 첨부 파일 내용 블록의 머리(메시지 본문 뒤에 붙는다)
const ATTACHMENTS_HEADER: &str = "\n\n---\n[첨부 파일 내용]\n";

/// 저장된 사용자 메시지에서 첨부 파일 내용 블록을 뺀 본문(지식 폴더 검색어로 쓴다).
fn without_attachments(content: &str) -> &str {
    content.find(ATTACHMENTS_HEADER).map_or(content, |i| &content[..i])
}

/// 승인 뒤 이어가기에서 참고 자료·Skill을 다시 고를 때 쓰는 글: 마지막 사용자 메시지에서 첨부 내용을 뺀 본문.
fn last_user_query(history: &[ChatMessage]) -> String {
    history
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| without_attachments(&m.content).to_string())
        .unwrap_or_default()
}

/// 이번 턴에 자동으로 불러올 Skill(설정 "Skill 자동 불러오기"가 꺼져 있으면 None). `message`는 첨부 내용을 뺀 사용자 글,
/// `history`는 대화 기록(맞는 Skill이 없으면 바로 앞 답변에서 자동으로 불러온 Skill을 이어 쓴다).
fn auto_skill_for_turn(
    settings: &AppSettings,
    project: Option<&Project>,
    message: &str,
    history: &[ChatMessage],
    context_length: u32,
) -> Option<skills::AutoSkill> {
    let ctx = project.map(|p| (p.id.as_str(), Path::new(&p.root_path)));
    skills::auto_for_turn(ctx, message, history, context_length, settings.skill_auto_load)
}

/// 참고 자료와 Skill 지침을 함께 넣을 때 합계 상한(컨텍스트의 25%)에 맞춰 참고 자료를 먼저 줄인다.
fn fit_knowledge(knowledge: String, skill: Option<&skills::AutoSkill>, context_length: u32) -> String {
    match skill {
        Some(s) => knowledge::fit_beside_skill(&knowledge, s.text.chars().count(), context_length),
        None => knowledge,
    }
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
    let mut block = String::from(ATTACHMENTS_HEADER);
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
                fs_ops::truncate_for_model(&t, agent::MAX_TOOL_OUTPUT_CHARS * 2)
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
    merge_tool_snapshots(&mut db, new_snapshots);
    // 오래된 되돌리기 기록을 정리한다 (그 기록이 쓰던 백업 파일도 지운다).
    let pruned_blobs = db.prune_snapshots();
    let conv = find_conv_mut(&mut db, conversation_id)?;
    upsert_message(conv, assistant_msg);
    conv.updated_at = chrono::Local::now().to_rfc3339();
    let updated = conv.clone();
    state.store.save(&db)?;
    for blob in pruned_blobs {
        fs_ops::delete_blob(&blob);
    }
    Ok(updated)
}

fn upsert_message(conv: &mut Conversation, msg: ChatMessage) {
    if let Some(existing) = conv.messages.iter_mut().find(|m| m.id == msg.id) {
        *existing = msg;
    } else {
        conv.messages.push(msg);
    }
}

fn merge_tool_snapshots(db: &mut AppDatabase, snapshots: impl IntoIterator<Item = FileSnapshot>) {
    let mut seen: HashSet<_> = db.snapshots.iter().map(|s| s.id.clone()).collect();
    db.snapshots.extend(snapshots.into_iter().filter(|s| seen.insert(s.id.clone())));
}

/// 중간 답변과 작업의 되돌리기 기록을 한 저장 파일에 반영한다.
fn save_checkpoint(state: &AppState, conversation_id: &str, msg: &ChatMessage, snapshots: &[FileSnapshot]) -> bool {
    let Ok(mut db) = state.db.lock() else { return false };
    let Ok(conv) = find_conv_mut(&mut db, conversation_id) else { return false };
    upsert_message(conv, msg.clone());
    merge_tool_snapshots(&mut db, snapshots.iter().cloned());
    state.store.save(&db).is_ok()
}

#[tauri::command]
fn get_initial_state(state: State<'_, AppState>) -> Result<AppDatabase, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db.clone())
}

/// 설정 저장. `api_key`(활성 공급자 토큰)가 비어 있으면 기존 토큰을 그대로 둔다 (키 삭제는 `clear_api_key`).
#[tauri::command]
fn save_settings(state: State<'_, AppState>, mut settings: AppSettings) -> Result<AppSettings, String> {
    settings.sync_active_to_profile();
    providers::validate_server_url(settings.provider, &settings.server_url)?;
    for (&p, prof) in &settings.providers {
        if !prof.server_url.trim().is_empty() {
            providers::validate_server_url(p, &prof.server_url)?;
        }
    }

    let new_key = settings.api_key.trim().to_string();
    if !new_key.is_empty() {
        secrets::store_provider_api_key(settings.provider, &new_key)?;
    }

    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let api_key = if !new_key.is_empty() {
        new_key
    } else if settings.provider == db.settings.provider {
        db.settings.api_key.clone()
    } else {
        secrets::load_provider_api_key(settings.provider)?.unwrap_or_default()
    };

    for p in store::LlmProvider::ALL {
        if p != settings.provider {
            if let (Some(prev), Some(next_prof)) = (db.settings.providers.get(&p), settings.providers.get_mut(&p)) {
                next_prof.api_key_set = prev.api_key_set;
            }
        }
    }
    settings.api_key_set = !api_key.is_empty();
    settings.api_key = api_key;
    settings.sync_active_to_profile();

    db.settings = settings;
    state.store.save(&db)?;
    // api_key는 직렬화되지 않으므로 화면에는 키가 전달되지 않는다.
    Ok(db.settings.clone())
}

fn settings_snapshot(state: &State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.db.lock().map_err(|e| e.to_string())?.settings.clone())
}

#[tauri::command]
async fn lm_list_models(state: State<'_, AppState>) -> Result<Vec<lmstudio::LmModel>, String> {
    providers::list_models(&settings_snapshot(&state)?).await
}

/// 선택한 모델이 원하는 컨텍스트로 쓸 수 있는지 확인한다(로드는 하지 않는다).
#[tauri::command]
async fn lm_plan_load(state: State<'_, AppState>, key: String) -> Result<lmstudio::LoadPlan, String> {
    providers::plan_load(&settings_snapshot(&state)?, &key).await
}

#[tauri::command]
async fn lm_load_model(
    state: State<'_, AppState>,
    key: String,
    context_length: u32,
) -> Result<lmstudio::LoadedInstance, String> {
    providers::load_model(&settings_snapshot(&state)?, &key, context_length).await
}

#[tauri::command]
async fn lm_unload(state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    providers::unload_instance(&settings_snapshot(&state)?, &instance_id).await
}

#[tauri::command]
async fn lm_server_running(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(providers::server_running(&settings_snapshot(&state)?).await)
}

#[tauri::command]
async fn lm_start_server(state: State<'_, AppState>) -> Result<(), String> {
    providers::start_server(&settings_snapshot(&state)?).await
}

#[tauri::command]
fn clear_api_key(state: State<'_, AppState>, provider: Option<store::LlmProvider>) -> Result<AppSettings, String> {
    let target = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        provider.unwrap_or(db.settings.provider)
    };
    secrets::delete_provider_api_key(target)?;
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    db.settings.ensure_provider_profiles();
    if let Some(prof) = db.settings.providers.get_mut(&target) {
        prof.api_key_set = false;
    }
    if target == db.settings.provider {
        db.settings.api_key.clear();
        db.settings.api_key_set = false;
    }
    state.store.save(&db)?;
    Ok(db.settings.clone())
}

/// 자격 증명 관리자에서 키를 불러온다. 예전 버전이 설정 파일에 평문으로 남긴 키는 옮기고 파일에서 지운다.
fn load_api_key_into(store: &StoreManager, db: &mut AppDatabase) {
    db.settings.ensure_provider_profiles();
    let active = db.settings.provider;
    let legacy = std::mem::take(&mut db.settings.api_key).trim().to_string();
    if !legacy.is_empty() {
        match secrets::store_provider_api_key(active, &legacy) {
            // api_key는 직렬화되지 않으므로 다시 저장하면 파일에서 평문 키가 사라진다.
            Ok(()) => {
                if let Err(e) = store.save(db) {
                    eprintln!("설정 파일 정리 실패: {}", e);
                }
            }
            Err(e) => eprintln!("{}", e),
        }
        db.settings.api_key = legacy;
    }
    for p in store::LlmProvider::ALL {
        match secrets::load_provider_api_key(p) {
            Ok(Some(k)) => {
                let has_key = !k.is_empty();
                if let Some(prof) = db.settings.providers.get_mut(&p) {
                    prof.api_key_set = has_key;
                }
                if p == active && db.settings.api_key.is_empty() {
                    db.settings.api_key = k;
                }
            }
            Ok(None) => {
                if let Some(prof) = db.settings.providers.get_mut(&p) {
                    prof.api_key_set = p == active && !db.settings.api_key.is_empty();
                }
            }
            Err(e) => eprintln!("{}", e),
        }
    }
    db.settings.api_key_set = !db.settings.api_key.is_empty();
    db.settings.sync_active_to_profile();
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
                $init = $env:ANYLM_DLG_INIT
                if ($init -and (Test-Path -LiteralPath $init)) { $dlg.SelectedPath = $init }
                if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    Write-Output $dlg.SelectedPath
                }
            "#;
            let init = initial_dir.unwrap_or_default();
            let path = run_dialog_script(SCRIPT, &[("ANYLM_DLG_INIT", &init)])?.trim().to_string();
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
                $init = $env:ANYLM_DLG_INIT
                if ($init -and (Test-Path -LiteralPath $init)) { $dlg.InitialDirectory = $init }
                if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
                    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
                    foreach ($f in $dlg.FileNames) { Write-Output $f }
                }
            "#;
            let init = initial_dir.unwrap_or_default();
            Ok(run_dialog_script(SCRIPT, &[("ANYLM_DLG_INIT", &init)])?
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
    let proj = upsert_project(&mut db.projects, id, name, root_path, instructions);
    state.store.save(&db)?;
    Ok(proj)
}

/// 프로젝트를 고치거나 새로 만든다. 고칠 때는 이름·폴더·지침만 바꾸고 메모리 등 나머지는 그대로 둔다.
fn upsert_project(
    projects: &mut Vec<Project>,
    id: Option<String>,
    name: String,
    root_path: String,
    instructions: String,
) -> Project {
    if let Some(existing_id) = id {
        if let Some(proj) = projects.iter_mut().find(|p| p.id == existing_id) {
            proj.name = name;
            proj.root_path = root_path;
            proj.instructions = instructions;
            return proj.clone();
        }
    }
    let proj = Project {
        id: format!("proj-{}", Uuid::new_v4().simple()),
        name,
        root_path,
        instructions,
        created_at: chrono::Local::now().to_rfc3339(),
        auto_approve: Vec::new(),
        memory: Vec::new(),
        knowledge_folders: Vec::new(),
        dev_servers: Vec::new(),
    };
    projects.push(proj.clone());
    proj
}

/// 프로젝트 메모리에 사용자가 직접 항목을 더한다.
#[tauri::command]
fn add_project_memory(state: State<'_, AppState>, project_id: String, text: String) -> Result<Project, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = memory::project_memory_mut(&mut db.projects, &project_id)?;
    let now = chrono::Local::now().to_rfc3339();
    memory::add_item(&mut proj.memory, &text, "user", &now)?;
    let updated = proj.clone();
    state.store.save(&db)?;
    Ok(updated)
}

#[tauri::command]
fn update_project_memory(
    state: State<'_, AppState>,
    project_id: String,
    memory_id: String,
    text: String,
) -> Result<Project, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = memory::project_memory_mut(&mut db.projects, &project_id)?;
    let now = chrono::Local::now().to_rfc3339();
    memory::update_item(&mut proj.memory, &memory_id, &text, &now)?;
    let updated = proj.clone();
    state.store.save(&db)?;
    Ok(updated)
}

#[tauri::command]
fn delete_project_memory(state: State<'_, AppState>, project_id: String, memory_id: String) -> Result<Project, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = memory::project_memory_mut(&mut db.projects, &project_id)?;
    memory::delete_item(&mut proj.memory, &memory_id)?;
    let updated = proj.clone();
    state.store.save(&db)?;
    Ok(updated)
}

#[tauri::command]
fn delete_project(app: AppHandle, state: State<'_, AppState>, project_id: String) -> Result<(), String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    if db.projects.len() <= 1 {
        return Err("최소 1개의 프로젝트는 유지되어야 합니다.".to_string());
    }
    db.projects.retain(|p| p.id != project_id);
    db.conversations.retain(|c| c.project_id != project_id);
    state.store.save(&db)?;
    // 앱 데이터 폴더의 색인만 지운다(지식 폴더 자체는 건드리지 않는다).
    if let Some(k) = app.try_state::<knowledge::KnowledgeManager>() {
        k.forget(&project_id);
    }
    Ok(())
}

/// 지식 폴더 목록을 바꾸고 저장한 뒤 화면에 알린다.
fn update_knowledge_folders(
    app: &AppHandle,
    state: &AppState,
    project_id: &str,
    change: impl FnOnce(&mut Vec<String>) -> Result<(), String>,
) -> Result<Project, String> {
    let updated = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let proj = memory::project_memory_mut(&mut db.projects, project_id)?;
        change(&mut proj.knowledge_folders)?;
        let updated = proj.clone();
        state.store.save(&db)?;
        updated
    };
    let _ = app.emit(agent::PROJECT_UPDATED_EVENT, &updated);
    Ok(updated)
}

/// 색인을 뒤에서 돌린다(화면이 멈추지 않게). 진행·결과는 `anylm-knowledge-status` 이벤트로 알린다.
fn spawn_knowledge_refresh(app: &AppHandle, project: &Project, mode: knowledge::RefreshMode) {
    let app = app.clone();
    let id = project.id.clone();
    let folders = project.knowledge_folders.clone();
    tauri::async_runtime::spawn(async move {
        knowledge::refresh_project(&app, &id, folders, mode).await;
    });
}

/// 지식 폴더를 더하고 색인을 시작한다. 폴더는 프로젝트 폴더 밖이어도 되고, 앱은 읽기만 한다.
#[tauri::command]
fn add_knowledge_folder(app: AppHandle, state: State<'_, AppState>, project_id: String, path: String) -> Result<Project, String> {
    let updated = update_knowledge_folders(&app, &state, &project_id, |list| knowledge::add_folder(list, &path).map(|_| ()))?;
    spawn_knowledge_refresh(&app, &updated, knowledge::RefreshMode::Full);
    Ok(updated)
}

#[tauri::command]
fn remove_knowledge_folder(app: AppHandle, state: State<'_, AppState>, project_id: String, path: String) -> Result<Project, String> {
    let updated = update_knowledge_folders(&app, &state, &project_id, |list| knowledge::remove_folder(list, &path))?;
    // 남은 폴더로 갱신해 빠진 폴더의 조각을 색인에서 지운다(바뀌지 않은 파일은 다시 읽지 않는다).
    spawn_knowledge_refresh(&app, &updated, knowledge::RefreshMode::Full);
    Ok(updated)
}

/// [다시 색인]: 모든 파일을 다시 추출한다.
#[tauri::command]
fn reindex_knowledge(app: AppHandle, state: State<'_, AppState>, project_id: String) -> Result<(), String> {
    let project = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.projects.iter().find(|p| p.id == project_id).cloned().ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?
    };
    if app.try_state::<knowledge::KnowledgeManager>().is_some_and(|k| k.is_indexing(&project_id)) {
        return Err("이미 색인하는 중입니다. 끝난 뒤 다시 누르세요.".to_string());
    }
    spawn_knowledge_refresh(&app, &project, knowledge::RefreshMode::Rebuild);
    Ok(())
}

#[tauri::command]
async fn knowledge_status(app: AppHandle, state: State<'_, AppState>, project_id: String) -> Result<knowledge::KnowledgeStatus, String> {
    let folders = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.projects.iter().find(|p| p.id == project_id).map(|p| p.knowledge_folders.clone()).unwrap_or_default()
    };
    let mgr = app
        .try_state::<knowledge::KnowledgeManager>()
        .ok_or_else(|| "지식 폴더 색인을 사용할 수 없습니다.".to_string())?;
    Ok(mgr.status(&project_id, &folders).await)
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
        mode: None,
        compaction: None,
        model_key: None,
    };
    db.conversations.insert(0, conv.clone());
    state.store.save(&db)?;
    Ok(conv)
}

/// 대화에서 쓸 모델을 정한다. 빈 문자열이면 지워서 기본 모델을 따르게 한다.
#[tauri::command]
fn set_conversation_model(
    state: State<'_, AppState>,
    conversation_id: String,
    model_key: String,
) -> Result<Conversation, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let conv = find_conv_mut(&mut db, &conversation_id)?;
    let key = model_key.trim();
    conv.model_key = (!key.is_empty()).then(|| key.to_string());
    let snapshot = conv.clone();
    state.store.save(&db)?;
    Ok(snapshot)
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

/// 압축으로 덮을 앞쪽 메시지 수. 남는 최근 구간이 assistant로 시작하지 않게 직전 user 경계에 맞추고,
/// 덮을 게 없으면 `None`.
fn compaction_cut(conv: &Conversation, keep_recent: usize) -> Option<usize> {
    let total = conv.messages.len();
    let mut covered = total.saturating_sub(keep_recent);
    while covered > 0
        && covered < total
        && conv.messages.get(covered).map(|m| m.role.as_str()) != Some("user")
    {
        covered -= 1;
    }
    (covered > 0).then_some(covered)
}

/// `a`가 `b`보다 나중 시각인지(RFC 3339). 어느 쪽이든 읽을 수 없으면 `false`.
fn created_after(a: &str, b: &str) -> bool {
    match (chrono::DateTime::parse_from_rfc3339(a), chrono::DateTime::parse_from_rfc3339(b)) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}

/// 보내기 직전에 자동 요약 압축을 해야 하는지: 설정 켜짐 && 덮을 새 대화가 있음(이전 압축 지점 뒤)
/// && 다음 요청 크기가 로드된 컨텍스트의 70% 이상.
///
/// 다음 요청 크기는 마지막 답변의 `prompt_tokens` + 새 글자 추정이다. 단, 압축이 있고 그 답변이
/// 압축보다 먼저 만들어졌거나(`created_at` < 압축의 `created_at`, 요약 카드를 고치면 압축 시각도 바뀐다)
/// 압축이 덮은 구간 안에 있으면, 그 토큰 수는 압축 전 대화를 잰 값이라 쓰지 않는다. 그때는 실제로
/// 보낼 부분(요약 + 덮이지 않은 메시지 + 새 글자)을 글자 수로 어림한다. 시각을 읽을 수 없으면
/// 오래된 값으로 본다(어림으로 대신하므로 안전한 쪽).
#[cfg(test)]
fn should_auto_compact(
    settings: &AppSettings,
    conv: &Conversation,
    new_user_chars: usize,
    context_length: u32,
) -> bool {
    should_auto_compact_with(settings, conv, new_user_chars, 0, context_length)
}

/// `should_auto_compact`에 이번 턴에만 더해 보내는 글자(지식 폴더 `[참고 자료]` 메시지)를 함께 본다.
/// 마지막 답변의 `prompt_tokens`를 쓸 때는 그 요청에도 같은 예산의 참고 자료가 이미 들어 있었으므로 더하지 않고,
/// 글자로 어림할 때만 더한다(어림은 시스템 프롬프트를 세지 않으므로).
fn should_auto_compact_with(
    settings: &AppSettings,
    conv: &Conversation,
    new_user_chars: usize,
    extra_system_chars: usize,
    context_length: u32,
) -> bool {
    if !settings.auto_compact || compaction_job(conv, 2).is_none() {
        return false;
    }
    let comp = conv.compaction.as_ref().filter(|c| c.covered_messages > 0);
    let last = conv.messages.iter().enumerate().rev().find(|(_, m)| m.role == "assistant");
    let fresh = |i: usize, m: &ChatMessage| match comp {
        None => true,
        Some(c) => i >= c.covered_messages && created_after(&m.created_at, &c.created_at),
    };
    let ratio = match (last, comp) {
        (Some((i, m)), _) if fresh(i, m) => {
            context_budget::usage_ratio(m.stats.as_ref().map(|s| s.prompt_tokens), new_user_chars, context_length)
        }
        (_, Some(c)) => {
            let active = &conv.messages[c.covered_messages.min(conv.messages.len())..];
            let active_chars = agent::conversation_source_chars(Some(c), active);
            context_budget::usage_ratio(None, active_chars + new_user_chars + extra_system_chars, context_length)
        }
        (_, None) => context_budget::usage_ratio(None, new_user_chars + extra_system_chars, context_length),
    };
    ratio >= context_budget::AUTO_COMPACT_RATIO
}

const AUTO_COMPACTION_EVENT: &str = "anylm-auto-compaction";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AutoCompactionPayload<'a> {
    conversation_id: &'a str,
    /// "started" | "done" | "fallback"
    phase: &'a str,
}

fn emit_auto_compaction(app: &AppHandle, conversation_id: &str, phase: &str) {
    let _ = app.emit(AUTO_COMPACTION_EVENT, AutoCompactionPayload { conversation_id, phase });
}

/// 자동 압축에 쓸 입력(보내기 전 대화 기준).
struct CompactionJob {
    covered: usize,
    prev: Option<store::ContextCompaction>,
    slice: Vec<ChatMessage>,
}

/// 이전 압축이 있으면 그 요약 + 새로 덮을 메시지(`prev.covered..cut`)로 이어 요약한다(수동 압축과 같은 규칙).
/// 새 지점이 이전 지점보다 뒤가 아니면 `None`: 이미 요약된 대화를 처음부터 다시 요약해
/// 사용자가 고친 요약이나 AI 요약을 버리지 않는다.
fn compaction_job(conv: &Conversation, keep_recent: usize) -> Option<CompactionJob> {
    let covered = compaction_cut(conv, keep_recent)?;
    let (prev, slice) = match conv.compaction.as_ref().filter(|p| p.covered_messages > 0) {
        Some(p) if covered <= p.covered_messages => return None,
        Some(p) => (Some(p.clone()), conv.messages[p.covered_messages..covered].to_vec()),
        None => (None, conv.messages[..covered].to_vec()),
    };
    Some(CompactionJob { covered, prev, slice })
}

/// 보내기 전 자동 압축 판단과 입력을 한 번에 만든다(`send_chat_message`가 쓴다).
fn auto_compaction_job(
    settings: &AppSettings,
    conv: &Conversation,
    new_user_chars: usize,
    context_length: u32,
    extra_system_chars: usize,
) -> Option<CompactionJob> {
    should_auto_compact_with(settings, conv, new_user_chars, extra_system_chars, context_length)
        .then(|| compaction_job(conv, 2))
        .flatten()
}

/// 자동 요약: AI 요약, 실패하면 로컬 요약. 결과를 대화에 저장하고 돌려준다.
/// 요약을 만들 수 없으면 `None`(보내기는 그대로 진행한다).
async fn run_auto_compaction(
    app: &AppHandle,
    state: &AppState,
    conversation_id: &str,
    settings: &AppSettings,
    reasoning_options: &[String],
    job: CompactionJob,
    cancel_flag: &std::sync::atomic::AtomicBool,
) -> Option<store::ContextCompaction> {
    emit_auto_compaction(app, conversation_id, "started");
    let source_chars = agent::conversation_source_chars(job.prev.as_ref(), &job.slice);
    let (summary, phase) = match agent::summarize_for_compaction(
        settings, job.prev.as_ref(), &job.slice, None, reasoning_options, cancel_flag,
    )
    .await
    {
        Ok(s) => (s, "done"),
        Err(_) => (
            agent::build_local_compaction_summary(job.prev.as_ref(), &job.slice, None),
            "fallback",
        ),
    };
    let compaction = store::ContextCompaction {
        covered_messages: job.covered,
        created_at: chrono::Local::now().to_rfc3339(),
        summary,
        focus: None,
        source_chars: Some(source_chars),
    };
    let saved = (|| {
        let mut db = state.db.lock().ok()?;
        let conv = db.conversations.iter_mut().find(|c| c.id == conversation_id)?;
        conv.compaction = Some(compaction.clone());
        let _ = state.store.save(&db);
        Some(())
    })();
    emit_auto_compaction(app, conversation_id, if saved.is_some() { phase } else { "fallback" });
    saved.map(|_| compaction)
}

#[tauri::command]
async fn send_chat_message(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    user_content: String,
    attached_files: Vec<String>,
    mode: Option<String>,
) -> Result<Conversation, String> {
    let run = state.runs.begin(&conversation_id)?;
    let cancel_flag = run.cancel_flag();

    // 첨부 파일은 잠금 밖에서 읽는다 (PDF/DOCX/PPTX는 웹뷰 변환기를 거친다).
    let (attach_project, prior_messages) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let conv = db
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;
        (conversation_project(&db, conv)?, conv.messages.clone())
    };
    let attachments_block = read_attachments(&app, attach_project.as_ref(), &attached_files).await;

    // 모델 목록은 턴마다 한 번만 읽는다(추론 선택지·로드된 컨텍스트: 자동 압축 판단과 턴이 함께 쓴다).
    let (pre_settings, pre_model_key) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let conv = db
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;
        (db.settings.clone(), conversation_model(&db.settings, conv)?)
    };
    let model_info = providers::turn_model_info(&pre_settings, &pre_model_key).await;
    let context_length = context_budget::resolve_context(model_info.loaded_context);
    // 지식 폴더: 빠른 갱신 뒤 이번 사용자 메시지(첨부 내용 제외)로 참고 자료를 찾는다. 자동 압축 판단에도 넣는다.
    let knowledge = knowledge::context_for_turn(&app, attach_project.as_ref(), &user_content, context_length).await;
    // Skill 자동 불러오기: 같은 글(첨부 내용 제외)로 고른다. 참고 자료처럼 자동 압축 판단에도 넣는다.
    let auto_skill = auto_skill_for_turn(&pre_settings, attach_project.as_ref(), &user_content, &prior_messages, context_length);
    drop(prior_messages);
    let knowledge = fit_knowledge(knowledge, auto_skill.as_ref(), context_length);

    let (settings, project, history_for_api, turn_mode, mut compaction, model_key, auto_job) = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db.settings.clone();
        let conv_idx = db
            .conversations
            .iter()
            .position(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;

        let project = conversation_project(&db, &db.conversations[conv_idx])?;
        let full_content = format!("{}{}", user_content, attachments_block);
        // 사용자 메시지를 넣기 전 대화로 자동 압축 여부를 정한다.
        let auto_job = auto_compaction_job(
            &settings,
            &db.conversations[conv_idx],
            full_content.chars().count(),
            context_length,
            knowledge.chars().count() + auto_skill.as_ref().map_or(0, |s| s.text.chars().count()),
        );

        let user_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: full_content,
            reasoning_content: None,
            tool_records: Vec::new(),
            created_at: chrono::Local::now().to_rfc3339(),
            ..Default::default()
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
        if mode.is_some() {
            conv.mode = mode.clone();
        }
        let turn_mode = conv.mode.clone().unwrap_or_default();
        let history = conv.messages.clone();
        let compaction = conv.compaction.clone();
        let model_key = conversation_model(&settings, conv)?;
        state.store.save(&db)?;
        (settings, project, history, turn_mode, compaction, model_key, auto_job)
    };
    let checkpoint = |m: &ChatMessage, snapshots: &[FileSnapshot]| save_checkpoint(&state, &conversation_id, m, snapshots);

    if let Some(job) = auto_job {
        // 요약도 이 대화의 모델로 요청한다.
        let mut summary_settings = settings.clone();
        summary_settings.model = model_key.clone();
        if let Some(c) = run_auto_compaction(
            &app, &state, &conversation_id, &summary_settings,
            &model_info.reasoning_options, job, &cancel_flag,
        )
        .await
        {
            compaction = Some(c);
        }
    }

    let turn_res = agent::run_agent_turn(
        &app,
        TurnRequest {
            settings: &settings,
            model_key: &model_key,
            model_display_name: model_info.display_name.as_deref().unwrap_or(""),
            reasoning_options: &model_info.reasoning_options,
            context_length,
            project: project.as_ref(),
            conversation_id: &conversation_id,
            history: &history_for_api,
            continue_from: None,
            mode: &turn_mode,
            compaction: compaction.as_ref(),
            checkpoint: Some(&checkpoint),
            knowledge: &knowledge,
            auto_skill: auto_skill.as_ref(),
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

    let (settings, project, history_for_api, backup_messages, backup_compaction, turn_mode, compaction, model_key) = {
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
        let backup_compaction = conv.compaction.clone();
        if let Some(ref c) = conv.compaction {
            if c.covered_messages > msg_pos {
                conv.compaction = None;
            }
        }
        conv.messages.truncate(msg_pos);
        expire_pending_approvals(conv);

        let updated_user_msg = ChatMessage {
            id: message_id,
            role: "user".to_string(),
            content: new_content.clone(),
            reasoning_content: None,
            tool_records: Vec::new(),
            created_at: chrono::Local::now().to_rfc3339(),
            ..Default::default()
        };

        if conv.messages.is_empty() {
            let short_title: String = new_content.chars().take(28).collect();
            if !short_title.is_empty() {
                conv.title = short_title;
            }
        }

        conv.messages.push(updated_user_msg);
        conv.updated_at = chrono::Local::now().to_rfc3339();
        let turn_mode = conv.mode.clone().unwrap_or_default();
        let history = conv.messages.clone();
        let compaction = conv.compaction.clone();
        let model_key = conversation_model(&settings, conv)?;
        state.store.save(&db)?;
        (settings, project, history, backup, backup_compaction, turn_mode, compaction, model_key)
    };
    let checkpoint = |m: &ChatMessage, snapshots: &[FileSnapshot]| save_checkpoint(&state, &conversation_id, m, snapshots);

    let model_info = providers::turn_model_info(&settings, &model_key).await;
    let context_length = context_budget::resolve_context(model_info.loaded_context);
    let knowledge = knowledge::context_for_turn(&app, project.as_ref(), &new_content, context_length).await;
    let auto_skill = auto_skill_for_turn(&settings, project.as_ref(), &new_content, &history_for_api, context_length);
    let knowledge = fit_knowledge(knowledge, auto_skill.as_ref(), context_length);
    let turn_res = agent::run_agent_turn(
        &app,
        TurnRequest {
            settings: &settings,
            model_key: &model_key,
            model_display_name: model_info.display_name.as_deref().unwrap_or(""),
            reasoning_options: &model_info.reasoning_options,
            context_length,
            project: project.as_ref(),
            conversation_id: &conversation_id,
            history: &history_for_api,
            continue_from: None,
            mode: &turn_mode,
            compaction: compaction.as_ref(),
            checkpoint: Some(&checkpoint),
            knowledge: &knowledge,
            auto_skill: auto_skill.as_ref(),
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
                    conv.compaction = backup_compaction;
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
/// 사용자가 승인한 도구를 실행한 결과
pub(crate) struct ApprovedRun {
    pub status: &'static str,
    pub output: String,
    pub snapshot: Option<FileSnapshot>,
    pub produced: Vec<String>,
}

/// 승인 대기 중이던 도구(삭제, 명령·스크립트 실행, 보호 경로 쓰기, MCP 도구)를 실제로 실행한다.
pub(crate) async fn run_approved_tool<R: tauri::Runtime>(
    app: &AppHandle<R>,
    project: Option<&Project>,
    tool_id: &str,
    tool_name: &str,
    args: &Value,
    always: bool,
    cancel: &std::sync::atomic::AtomicBool,
) -> ApprovedRun {
    let project_id = project.map(|p| p.id.clone()).unwrap_or_default();
    let root = project.map(|p| PathBuf::from(&p.root_path)).unwrap_or_default();
    let mut new_snap: Option<FileSnapshot> = None;
    let mut produced: Vec<String> = Vec::new();
    let (status, output) = match tool_name {
        name if name.starts_with(mcp::TOOL_PREFIX) => match app.try_state::<Arc<mcp::McpManager>>() {
            Some(state) => {
                let mgr = state.inner().clone();
                match mgr.tool_specs().await.into_iter().find(|t| t.exposed == name) {
                    Some(spec) => {
                        if always {
                            let _ = mgr.set_tool(&spec.server_id, &spec.tool, None, Some(true));
                        }
                        match mgr.call(&spec.server_id, &spec.tool, args).await {
                            Ok((text, false)) => ("completed", text),
                            Ok((text, true)) => ("error", text),
                            Err(e) => ("error", e),
                        }
                    }
                    None => ("error", format!("'{}' 도구는 지금 연결된 MCP 서버에 없습니다.", name)),
                }
            }
            None => ("error", "MCP 서버 관리자가 준비되지 않았습니다.".to_string()),
        },
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
            // `python x.py` 한 줄이면 앱의 Python 실행기로 돌린다 (PC에 Python이 따로 없어도 되고, 만든 파일도 알려 준다).
            let script = python_runner::simple_script_command(cmd)
                .filter(|rel| fs_ops::resolve_safe_path(&root, rel).is_ok_and(|p| p.is_file()));
            match script {
                Some(rel) => match python_runner::run_script(&root, &rel, python_runner::RUN_TIMEOUT).await {
                    Ok(outcome) => {
                        produced = outcome.produced_files();
                        (if outcome.succeeded() { "completed" } else { "error" }, outcome.summary())
                    }
                    Err(e) => ("error", e),
                },
                None => match fs_ops::execute_shell_command(&root, cmd, SHELL_TIMEOUT).await {
                    Ok(out) => ("completed", out),
                    Err(e) => ("error", e),
                },
            }
        }
        "start_server" => match project {
            Some(project) => match dev_servers::start_for_model(app, project, args, cancel).await {
                Ok(result) => ("completed", result),
                Err(err) => ("error", err),
            },
            None => ("error", "개발 서버는 프로젝트에서만 실행할 수 있습니다.".into()),
        },
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
            let (rec, snap, _) = agent::execute_tool_now(app, &root, &project_id, tool_id, tool_name, args).await;
            new_snap = snap;
            if rec.status == "completed" {
                ("completed", format!("승인 완료: {}", rec.output))
            } else {
                ("error", rec.output)
            }
        }
        other => ("error", format!("승인이 필요 없는 도구입니다: {}", other)),
    };
    ApprovedRun { status, output, snapshot: new_snap, produced }
}

// ---------------------------------------------------------------------------
// 자동 업데이트
// 깃허브 릴리스의 latest.json을 읽어 새 버전이 있으면 알려 주고, 사용자가 누르면 내려받아 설치한다.
// 설치 파일은 이 앱의 서명 키로 서명된 것만 받아들인다 (tauri.conf.json의 plugins.updater.pubkey).
// ---------------------------------------------------------------------------

pub const UPDATE_PROGRESS_EVENT: &str = "update-progress";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    current_version: String,
    version: String,
    /// 릴리스 노트
    notes: Option<String>,
    date: Option<String>,
}

/// 지금 실행 중인 앱의 버전
#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

fn shutdown_background_for_update<R: Runtime>(app: &AppHandle<R>) {
    if let Some(s) = app.try_state::<AppState>() {
        s.runs.cancel_all();
    }
    if let Some(m) = app.try_state::<Arc<mcp::McpManager>>() {
        m.shutdown_all();
    }
    if let Some(t) = app.try_state::<terminal::TerminalManager>() {
        t.close_all();
    }
    if let Some(g) = app.try_state::<single_instance::ListenerGuard>() {
        g.shutdown();
    }
}

async fn find_update(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let before_exit = app.clone();
    let updater = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(20))
        .on_before_exit(move || {
            shutdown_background_for_update(&before_exit);
            before_exit.cleanup_before_exit();
        })
        .build()
        .map_err(|e| format!("업데이트 확인을 준비하지 못했습니다: {}", e))?;
    updater
        .check()
        .await
        .map_err(|e| format!("새 버전이 있는지 확인하지 못했습니다: {}", e))
}

/// 새 버전이 있으면 그 정보를, 최신이면 null을 돌려준다.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    Ok(find_update(&app).await?.map(|u| UpdateInfo {
        current_version: u.current_version.clone(),
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    }))
}

/// 새 버전을 내려받아 설치한다. 설치가 시작되면 앱이 닫히고, 설치가 끝나면 새 버전으로 다시 열린다.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let mut update = find_update(&app)
        .await?
        .ok_or_else(|| "이미 최신 버전입니다.".to_string())?;
    update.timeout = Some(std::time::Duration::from_secs(180));
    let progress = app.clone();
    let mut downloaded: u64 = 0;
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(
                    UPDATE_PROGRESS_EVENT,
                    serde_json::json!({ "downloaded": downloaded, "total": total }),
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("새 버전을 내려받지 못했습니다: {}", e))?;
    // 설치 프로그램이 앱을 닫기 전에 진행 중인 작업·로컬 MCP 서버·터미널·단일 인스턴스 포트를 먼저 정리한다.
    shutdown_background_for_update(&app);
    update
        .install(bytes)
        .map_err(|e| format!("새 버전을 설치하지 못했습니다: {}", e))?;
    // Windows에서는 설치 프로그램이 앱을 닫고 다시 연다. 그렇지 않은 환경에서는 직접 다시 시작한다.
    app.restart();
}

/// 프로젝트에서 묻지 않고 실행할 도구를 켜거나 끈다.
#[tauri::command]
fn set_project_auto_approve(
    state: State<'_, AppState>,
    project_id: String,
    tool: String,
    enabled: bool,
) -> Result<Project, String> {
    if !agent::AUTO_APPROVABLE.contains(&tool.as_str()) {
        return Err(format!("'{}' 도구는 항상 허용으로 둘 수 없습니다.", tool));
    }
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let proj = db
        .projects
        .iter_mut()
        .find(|p| p.id == project_id)
        .ok_or_else(|| "프로젝트를 찾을 수 없습니다.".to_string())?;
    proj.auto_approve.retain(|t| t != &tool);
    if enabled {
        proj.auto_approve.push(tool);
    }
    let updated = proj.clone();
    state.store.save(&db)?;
    Ok(updated)
}

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
    let (project, tool_name, args, model_key) = {
        let mut db = state.db.lock().map_err(|e| e.to_string())?;
        let conv = db
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| "대화를 찾을 수 없습니다.".to_string())?;
        let project = conversation_project(&db, conv)?;
        // 이어서 답변할 모델이 없으면 도구를 실행하기 전에 알린다.
        let model_key = conversation_model(&db.settings, conv)?;
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
        (project, rec.tool_name.clone(), args, model_key)
    };

    // "항상 허용": 이 프로젝트에서는 이 도구를 앞으로 묻지 않고 실행한다 (MCP 도구는 서버 설정에 따로 적는다).
    let mut project = project;
    if approve && always == Some(true) && agent::AUTO_APPROVABLE.contains(&tool_name.as_str()) {
        if let Some(p) = project.as_mut() {
            let mut db = state.db.lock().map_err(|e| e.to_string())?;
            if let Some(stored) = db.projects.iter_mut().find(|x| x.id == p.id) {
                if !stored.auto_approve.contains(&tool_name) {
                    stored.auto_approve.push(tool_name.clone());
                }
                *p = stored.clone();
            }
            state.store.save(&db)?;
        }
    }

    // 2) 잠금 밖에서 실제 작업 실행
    let ApprovedRun { status, output, snapshot: new_snap, produced } = if approve {
        run_approved_tool(&app, project.as_ref(), &tool_id, &tool_name, &args, always == Some(true), &run.cancel_flag()).await
    } else {
        ApprovedRun {
            status: "rejected",
            output: "사용자가 실행을 거부했습니다.".to_string(),
            snapshot: None,
            produced: Vec::new(),
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
            Some((
                model_key,
                settings,
                conv.messages.clone(),
                msg.clone(),
                conv.mode.clone().unwrap_or_default(),
                conv.compaction.clone(),
            ))
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
    let (model_key, settings, history, assistant_msg, turn_mode, compaction) = continuation;
    let checkpoint = |m: &ChatMessage, snapshots: &[FileSnapshot]| save_checkpoint(&state, &conversation_id, m, snapshots);
    let model_info = providers::turn_model_info(&settings, &model_key).await;
    let context_length = context_budget::resolve_context(model_info.loaded_context);
    // 승인 뒤 이어가기도 같은 질문(마지막 사용자 메시지, 첨부 내용 제외)으로 참고 자료와 Skill을 다시 만들어 같은 자리에 넣는다.
    // Skill은 다시 고르지 않고, 멈췄던 답변의 자동 불러오기 카드에 적힌 Skill을 그대로 넣는다(그사이 설정을 껐으면 넣지 않는다).
    let last_user = last_user_query(&history);
    let knowledge = knowledge::context_for_turn(&app, project.as_ref(), &last_user, context_length).await;
    let auto_skill = settings
        .skill_auto_load
        .then(|| {
            skills::auto_for_resume(project.as_ref().map(|p| (p.id.as_str(), Path::new(&p.root_path))), &assistant_msg, context_length)
        })
        .flatten();
    let knowledge = fit_knowledge(knowledge, auto_skill.as_ref(), context_length);
    let (updated_msg, new_snapshots) = agent::run_agent_turn(
        &app,
        TurnRequest {
            settings: &settings,
            model_key: &model_key,
            model_display_name: model_info.display_name.as_deref().unwrap_or(""),
            reasoning_options: &model_info.reasoning_options,
            context_length,
            project: project.as_ref(),
            conversation_id: &conversation_id,
            history: &history,
            continue_from: Some(assistant_msg),
            mode: &turn_mode,
            compaction: compaction.as_ref(),
            checkpoint: Some(&checkpoint),
            knowledge: &knowledge,
            auto_skill: auto_skill.as_ref(),
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

/// 대화 히스토리의 앞부분을 요약해 컨텍스트 압축 지점을 생성·갱신한다.
/// 기존 메시지는 화면 표시와 기록 보존을 위해 삭제하지 않고 그대로 둔다.
#[tauri::command]
async fn compact_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
    use_ai: bool,
    keep_recent: usize,
    focus: Option<String>,
) -> Result<Conversation, String> {
    let run = state.runs.begin(&conversation_id)?;
    let cancel_flag = run.cancel_flag();
    let focus_clean = focus
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let (settings, covered, prev_ctx, slice_to_summarize, source_chars) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let conv = db
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;

        let covered = compaction_cut(conv, keep_recent).ok_or_else(|| {
            "압축할 이전 대화가 부족합니다. 최근 유지 메시지 수를 줄이거나 대화를 더 진행한 후 압축하세요."
                .to_string()
        })?;

        let (prev_ctx, slice) = match conv.compaction.as_ref() {
            Some(prev) if prev.covered_messages > 0 && prev.covered_messages < covered => {
                (Some(prev.clone()), conv.messages[prev.covered_messages..covered].to_vec())
            }
            _ => (None, conv.messages[..covered].to_vec()),
        };

        let source_chars = agent::conversation_source_chars(prev_ctx.as_ref(), &slice);
        // 요약도 이 대화의 모델로 요청한다(대화 모델이 없으면 기본 모델).
        let mut settings = db.settings.clone();
        settings.model = conversation_model(&settings, conv).unwrap_or_default();
        (settings, covered, prev_ctx, slice, source_chars)
    };

    let summary = if use_ai {
        let reasoning_options = providers::turn_model_info(&settings, &settings.model).await.reasoning_options;
        agent::summarize_for_compaction(
            &settings,
            prev_ctx.as_ref(),
            &slice_to_summarize,
            focus_clean.as_deref(),
            &reasoning_options,
            &cancel_flag,
        )
        .await?
    } else {
        agent::build_local_compaction_summary(
            prev_ctx.as_ref(),
            &slice_to_summarize,
            focus_clean.as_deref(),
        )
    };

    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let conv = db
        .conversations
        .iter_mut()
        .find(|c| c.id == conversation_id)
        .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;
    let safe_covered = covered.min(conv.messages.len());
    conv.compaction = Some(crate::store::ContextCompaction {
        covered_messages: safe_covered,
        created_at: chrono::Local::now().to_rfc3339(),
        summary,
        focus: focus_clean,
        source_chars: Some(source_chars),
    });
    conv.updated_at = chrono::Local::now().to_rfc3339();
    let updated = conv.clone();
    state.store.save(&db)?;
    Ok(updated)
}

/// 적용된 컨텍스트 압축 요약을 직접 수정하거나(Some) 압축을 해제(None)한다.
#[tauri::command]
fn update_conversation_compaction(
    state: State<'_, AppState>,
    conversation_id: String,
    summary: Option<String>,
) -> Result<Conversation, String> {
    let mut db = state.db.lock().map_err(|e| e.to_string())?;
    let conv = db
        .conversations
        .iter_mut()
        .find(|c| c.id == conversation_id)
        .ok_or_else(|| "대화 세션을 찾을 수 없습니다.".to_string())?;

    match summary {
        Some(text) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                return Err("압축 요약 내용이 비어 있을 수 없습니다.".to_string());
            }
            let comp = conv
                .compaction
                .as_mut()
                .ok_or_else(|| "적용된 컨텍스트 압축이 없습니다.".to_string())?;
            comp.summary = trimmed;
            comp.created_at = chrono::Local::now().to_rfc3339();
        }
        None => {
            conv.compaction = None;
        }
    }

    conv.updated_at = chrono::Local::now().to_rfc3339();
    let updated = conv.clone();
    state.store.save(&db)?;
    Ok(updated)
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

/// HTML 캔버스용 격리 웹뷰를 준비한다. 편집 중인 HTML은 메모리에서 제공하고,
/// 연결 리소스는 프로젝트(또는 사용자가 직접 고른 파일의 폴더) 안에서만 읽는다.
#[tauri::command]
fn prepare_html_preview(
    state: State<'_, AppState>,
    grants: State<'_, PathGrants>,
    previews: State<'_, Result<html_preview::PreviewServer, String>>,
    project_id: Option<String>,
    relative_path: Option<String>,
    absolute_path: Option<String>,
    html: String,
) -> Result<html_preview::PreviewHandle, String> {
    let (root, entry_path) = if let (Some(pid), Some(rel)) = (project_id.as_deref(), relative_path.as_deref()) {
        let (root, _) = project_root(&state, pid)?;
        let target = fs_ops::resolve_safe_path(&root, rel)?;
        if !target.is_file() {
            return Err(format!("HTML 파일을 찾을 수 없습니다: {}", rel));
        }
        (Some(root), rel.replace('\\', "/"))
    } else if let Some(path) = absolute_path.as_deref() {
        let target = PathBuf::from(path);
        if !target.is_absolute() || !target.is_file() || !grants.is_granted(&target) {
            return Err(not_granted(path));
        }
        let root = target.parent().ok_or_else(|| "HTML 파일의 폴더를 찾을 수 없습니다.".to_string())?.to_path_buf();
        let name = target.file_name().and_then(|n| n.to_str()).ok_or_else(|| "HTML 파일 이름을 읽을 수 없습니다.".to_string())?;
        (Some(root), name.to_string())
    } else if relative_path.is_none() {
        (None, "index.html".to_string())
    } else {
        return Err("HTML 파일의 프로젝트를 찾을 수 없습니다.".to_string());
    };
    previews.inner().as_ref().map_err(Clone::clone)?.create(root, entry_path, html.into_bytes())
}

#[tauri::command]
fn close_html_preview(previews: State<'_, Result<html_preview::PreviewServer, String>>, token: String) {
    if let Ok(previews) = previews.inner().as_ref() {
        previews.close(&token);
    }
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
                $dlg.FileName = $env:ANYLM_DLG_NAME
                try { $dlg.Filter = $env:ANYLM_DLG_FILTER } catch { $dlg.Filter = '모든 파일 (*.*)|*.*' }
                $dlg.OverwritePrompt = $true
                $dlg.AddExtension = $true
                $init = $env:ANYLM_DLG_INIT
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
                    ("ANYLM_DLG_INIT", &init),
                    ("ANYLM_DLG_NAME", &default_name),
                    ("ANYLM_DLG_FILTER", &filter),
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
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    catalog::load(url.as_deref().unwrap_or_default(), &dir).await
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
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "mp3", "wav", "mp4", "zip",
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
/// 앱 출처(`ipc_guard::origins_of`)와 scheme·호스트·포트가 정확히 같아야 한다 (결정 0019).
pub fn is_app_url(url: &tauri::Url, app_origins: &[String]) -> bool {
    ipc_guard::is_app_origin(url.as_str(), app_origins)
}

fn navigation_guard<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("navigation-guard")
        .on_navigation(|webview, url| {
            let ok = is_app_url(url, &ipc_guard::origins_of(webview.app_handle()));
            if !ok {
                eprintln!("차단한 화면 이동: {}", url);
            }
            ok
        })
        .on_webview_ready(|webview| ipc_guard::install_frame_guard(&webview))
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
pub fn run() {
    // 이미 실행 중이면 새로 띄우지 않고 기존 창을 앞으로 가져온다 (저장 파일·로그인 포트 충돌 방지).
    let mut instance = match single_instance::acquire() {
        single_instance::Instance::AlreadyRunning => return,
        single_instance::Instance::First(l) => Some(l),
        single_instance::Instance::Unknown => None,
    };
    tauri::Builder::default()
        .plugin(navigation_guard())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            ipc_guard::remember_origins(ipc_guard::origins_of(app.handle()));
            if let Some(listener) = instance.take() {
                let handle = app.handle().clone();
                let guard = single_instance::listen(listener, move || {
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                });
                app.manage(guard);
            }
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("any-lm-desktop-data"));
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
            // 처음 실행할 때 만드는 기본 작업 폴더: 문서\AnyLM (설치 폴더는 쓸 수 없는 곳일 수 있다)
            let default_workspace = app
                .path()
                .document_dir()
                .map(|d| d.join("AnyLM"))
                .unwrap_or_else(|_| app_data_dir.join("workspace"));
            let store = StoreManager::new(&app_data_dir).with_default_workspace(default_workspace);
            let mut initial_db = store.load();
            for blob in initial_db.prune_snapshots() {
                fs_ops::delete_blob(&blob);
            }
            load_api_key_into(&store, &mut initial_db);
            app.manage(AppState {
                store,
                db: Mutex::new(initial_db),
                runs: runs::RunRegistry::default(),
            });
            app.manage(doc_bridge::DocBridge::default());
            app.manage(knowledge::KnowledgeManager::new(app_data_dir.join("knowledge")));
            app.manage(PathGrants::default());
            app.manage(terminal::TerminalManager::new(terminal::tauri_sink(app.handle().clone())));
            app.manage(dev_servers::DevServers::default());
            app.manage(html_preview::PreviewServer::start().map_err(|err| format!("HTML 미리보기 서버를 시작할 수 없습니다: {err}")));
            Ok(())
        })
        // 앱 명령은 앱 화면의 출처에서 온 요청만 실행한다 (결정 0019).
        .invoke_handler(ipc_guard::guard_invoke(tauri::generate_handler![
            get_initial_state,
            save_settings,
            clear_api_key,
            lm_list_models,
            lm_plan_load,
            lm_load_model,
            lm_unload,
            set_conversation_model,
            add_project_memory,
            update_project_memory,
            delete_project_memory,
            add_knowledge_folder,
            remove_knowledge_folder,
            reindex_knowledge,
            knowledge_status,
            lm_server_running,
            lm_start_server,
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
            set_project_auto_approve,
            app_version,
            check_update,
            install_update,
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
            compact_conversation,
            update_conversation_compaction,
            run_manual_terminal_command,
            read_project_file_base64,
            read_absolute_file_base64,
            prepare_html_preview,
            close_html_preview,
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
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
            terminal::terminal_list,
            terminal::terminal_snapshot,
            terminal::terminal_busy,
            terminal::terminal_shells,
            dev_servers::dev_server_options,
            dev_servers::dev_server_start,
            dev_servers::dev_server_list,
            dev_servers::dev_preview_check,
        ]))
        .build(tauri::generate_context!())
        .expect("error while running Any LM Desktop")
        .run(|app, event| {
            // 앱을 끝낼 때 진행 중인 작업·로컬 MCP 서버·단일 인스턴스 포트를 정리한다.
            if let tauri::RunEvent::Exit = event {
                if let Some(s) = app.try_state::<AppState>() {
                    s.runs.cancel_all();
                }
                if let Some(m) = app.try_state::<Arc<mcp::McpManager>>() {
                    m.shutdown_all();
                }
                // 작업 패널 터미널: 세션마다 프로세스 트리 전체를 끝낸다.
                if let Some(t) = app.try_state::<terminal::TerminalManager>() {
                    t.close_all();
                }
                if let Some(g) = app.try_state::<single_instance::ListenerGuard>() {
                    g.shutdown();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn knowledge_query_drops_the_attachments_block() {
        let stored = format!("ZK-7741 출시일은?{}### a.txt\n첨부 본문", super::ATTACHMENTS_HEADER);
        assert_eq!(super::without_attachments(&stored), "ZK-7741 출시일은?");
        assert_eq!(super::without_attachments("첨부 없음"), "첨부 없음");
        // 승인 뒤 이어가기는 마지막 사용자 메시지의 본문(첨부 제외)으로 참고 자료·Skill을 다시 고른다.
        let user = super::ChatMessage { role: "user".into(), content: stored.clone(), ..Default::default() };
        let answer = super::ChatMessage { role: "assistant".into(), content: "답".into(), ..Default::default() };
        assert_eq!(super::last_user_query(&[user, answer]), "ZK-7741 출시일은?");
        assert_eq!(super::last_user_query(&[]), "");
    }

    use super::*;

    #[test]
    fn updating_a_project_keeps_its_memory_auto_approve_and_knowledge_folders() {
        let mut projects = vec![Project {
            id: "p1".into(),
            name: "옛 이름".into(),
            root_path: "C:/a".into(),
            instructions: "옛 지침".into(),
            created_at: "t".into(),
            auto_approve: vec!["run_python".into()],
            memory: vec![store::MemoryItem {
                id: "m1".into(),
                text: "기억".into(),
                created_at: "t".into(),
                updated_at: "t".into(),
                source: "model".into(),
            }],
            knowledge_folders: vec![r"D:\자료".into()],
            dev_servers: Vec::new(),
        }];
        let updated = upsert_project(&mut projects, Some("p1".into()), "새 이름".into(), "C:/b".into(), "새 지침".into());
        assert_eq!(updated.name, "새 이름");
        assert_eq!(updated.instructions, "새 지침");
        assert_eq!(updated.memory.len(), 1);
        assert_eq!(projects[0].memory[0].text, "기억");
        assert_eq!(projects[0].auto_approve, vec!["run_python".to_string()]);
        assert_eq!(updated.knowledge_folders, vec![r"D:\자료".to_string()], "지식 폴더도 그대로");
        // 새로 만들면 메모리는 비어 있다
        let created = upsert_project(&mut projects, None, "n".into(), "C:/c".into(), "".into());
        assert!(created.memory.is_empty());
        assert!(created.knowledge_folders.is_empty());
        assert_eq!(projects.len(), 2);
    }

    fn conv_with(n: usize, last_prompt_tokens: u64) -> Conversation {
        let mut c = Conversation::default();
        for i in 0..n {
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            let mut m = ChatMessage { role: role.into(), content: format!("m{}", i), ..Default::default() };
            if i + 1 == n && role == "assistant" {
                m.stats = Some(store::TurnStats { prompt_tokens: last_prompt_tokens, ..Default::default() });
            }
            c.messages.push(m);
        }
        // 마지막이 user로 끝나면 직전 어시스턴트에 토큰 수를 단다.
        if n >= 2 && n % 2 == 1 {
            c.messages[n - 2].stats = Some(store::TurnStats { prompt_tokens: last_prompt_tokens, ..Default::default() });
        }
        c
    }

    #[test]
    fn auto_compact_triggers_at_70_percent() {
        let conv = conv_with(10, 23_000);
        assert!(should_auto_compact(&AppSettings::default(), &conv, 0, 32768));
        assert!(!should_auto_compact(&AppSettings::default(), &conv_with(10, 20_000), 0, 32768));
        // 새로 보낼 글자도 센다: 20000 + 4000자/1.5 = 약 22667 토큰 -> 아직, 6000자면 24000 -> 넘음
        assert!(!should_auto_compact(&AppSettings::default(), &conv_with(10, 20_000), 3_000, 32768));
        assert!(should_auto_compact(&AppSettings::default(), &conv_with(10, 20_000), 6_000, 32768));
        // 토큰 기록이 없으면 새 글자만으로 계산한다.
        assert!(!should_auto_compact(&AppSettings::default(), &conv_with(10, 0), 100, 32768));
    }

    #[test]
    fn auto_compact_respects_setting() {
        let mut s = AppSettings::default();
        s.auto_compact = false;
        assert!(!should_auto_compact(&s, &conv_with(10, 30_000), 0, 32768));
    }

    #[test]
    fn auto_compact_skips_when_nothing_to_cover() {
        assert!(!should_auto_compact(&AppSettings::default(), &conv_with(1, 30_000), 0, 32768));
        assert!(!should_auto_compact(&AppSettings::default(), &conv_with(2, 30_000), 0, 32768));
    }

    /// 대화 앞 `covered`개를 덮은 압축을 단다(압축 시각 `at`).
    fn with_compaction(mut c: Conversation, covered: usize, at: &str, summary: &str) -> Conversation {
        c.compaction = Some(store::ContextCompaction {
            covered_messages: covered,
            created_at: at.into(),
            summary: summary.into(),
            focus: None,
            source_chars: None,
        });
        c
    }

    fn stamp(c: &mut Conversation, from: usize, at: &str) {
        for m in &mut c.messages[from..] {
            m.created_at = at.into();
        }
    }

    #[test]
    fn auto_compact_ignores_prompt_tokens_measured_before_the_compaction() {
        // u0 a1 u2 a3(30000토큰, 압축 전 측정) + 압축(앞 2개, a3 뒤에 만듦). 수동 압축 직후의 모습.
        let mut c = conv_with(4, 30_000);
        stamp(&mut c, 0, "2026-11-23T10:00:00+09:00");
        let c = with_compaction(c, 2, "2026-11-23T10:05:00+09:00", "고친 요약");
        assert!(!should_auto_compact(&AppSettings::default(), &c, 0, 32768));
        assert!(auto_compaction_job(&AppSettings::default(), &c, 0, 32768, 0).is_none());

        // 대화가 더 이어져 덮을 새 구간이 있어도, 마지막 답변이 압축 전(또는 요약 카드를 고치기 전)에
        // 잰 값이면 쓰지 않는다: u0 a1 u2 a3 u4 a5(전부 압축 전) → 실제로 보낼 부분은 작다.
        let mut c = conv_with(6, 30_000);
        stamp(&mut c, 0, "2026-11-23T10:00:00+09:00");
        let c = with_compaction(c, 2, "2026-11-23T10:05:00+09:00", "요약");
        assert!(!should_auto_compact(&AppSettings::default(), &c, 0, 32768));

        // 시각을 읽을 수 없는 오래된 기록도 같다(글자 어림으로 대신한다).
        let c = with_compaction(conv_with(6, 30_000), 2, "", "요약");
        assert!(!should_auto_compact(&AppSettings::default(), &c, 0, 32768));

        // 단, 보낼 부분 자체가 크면(요약+남은 메시지 글자 어림) 압축한다: 40000자 / 1.5 ≈ 26667토큰 ≥ 70%.
        let mut c = conv_with(6, 30_000);
        stamp(&mut c, 0, "2026-11-23T10:00:00+09:00");
        c.messages[3].content = "가".repeat(40_000);
        let c = with_compaction(c, 2, "2026-11-23T10:05:00+09:00", "요약");
        assert!(should_auto_compact(&AppSettings::default(), &c, 0, 32768));
    }

    #[test]
    fn knowledge_section_counts_toward_auto_compaction_only_when_estimating() {
        let s = AppSettings::default();
        // 압축 전에 잰 토큰은 쓰지 않고 글자로 어림하는 경우: 보낼 대화 약 30,000자(61%) + 참고 자료 7,000자 → 75%
        let mut c = conv_with(6, 30_000);
        stamp(&mut c, 0, "2026-11-23T10:00:00+09:00");
        c.messages[3].content = "가".repeat(30_000);
        let c = with_compaction(c, 2, "2026-11-23T10:05:00+09:00", "요약");
        assert!(!should_auto_compact_with(&s, &c, 0, 0, 32768));
        assert!(should_auto_compact_with(&s, &c, 0, 7_000, 32768));
        assert!(auto_compaction_job(&s, &c, 0, 32768, 7_000).is_some());
        // 마지막 답변의 prompt_tokens를 믿을 수 있으면(그 요청에도 참고 자료가 들어 있었다) 더하지 않는다.
        let fresh = conv_with(10, 20_000);
        assert_eq!(should_auto_compact_with(&s, &fresh, 0, 7_000, 32768), should_auto_compact(&s, &fresh, 0, 32768));
    }

    #[test]
    fn auto_compact_skips_when_cut_is_not_past_previous_compaction() {
        // 새 답변이 압축 뒤에 잰 높은 값이어도, 덮을 지점(4)이 이전 지점(4) 뒤가 아니면 다시 요약하지 않는다.
        let mut c = conv_with(6, 30_000);
        stamp(&mut c, 0, "2026-11-23T10:00:00+09:00");
        stamp(&mut c, 4, "2026-11-23T10:10:00+09:00");
        let c = with_compaction(c, 4, "2026-11-23T10:05:00+09:00", "요약");
        assert_eq!(compaction_cut(&c, 2), Some(4));
        assert!(compaction_job(&c, 2).is_none());
        assert!(!should_auto_compact(&AppSettings::default(), &c, 0, 32768));
    }

    #[test]
    fn auto_path_with_previous_compaction_builds_on_previous_summary() {
        // u0 a1 | 압축(앞 2개) | u2 a3 u4 a5 — a5는 압축 뒤에 받은 답변이고 25000토큰(76%).
        let mut c = conv_with(6, 25_000);
        stamp(&mut c, 0, "2026-11-23T10:00:00+09:00");
        stamp(&mut c, 2, "2026-11-23T10:10:00+09:00");
        let c = with_compaction(c, 2, "2026-11-23T10:05:00+09:00", "이전 누적 요약 본문");
        let job = auto_compaction_job(&AppSettings::default(), &c, 0, 32768, 0).expect("70%를 넘었으니 압축한다");
        assert_eq!(job.covered, 4);
        assert_eq!(job.prev.as_ref().map(|p| p.summary.as_str()), Some("이전 누적 요약 본문"));
        let slice: Vec<&str> = job.slice.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(slice, vec!["m2", "m3"], "새로 덮는 메시지만 요약에 넣는다");
        // 로컬 요약(실패 시 대체)도 이전 요약 위에 쌓인다. AI 요약도 같은 prev/slice를 받는다.
        let summary = agent::build_local_compaction_summary(job.prev.as_ref(), &job.slice, None);
        assert!(summary.contains("이전 누적 요약 본문"), "{}", summary);
        assert_eq!(agent::conversation_source_chars(job.prev.as_ref(), &job.slice), "이전 누적 요약 본문".chars().count() + 4);
        // 70% 아래면 압축하지 않는다.
        assert!(auto_compaction_job(&AppSettings::default(), &conv_with(6, 20_000), 0, 32768, 0).is_none());
    }

    #[test]
    fn compaction_cut_matches_original_boundary_rule() {
        assert_eq!(compaction_cut(&conv_with(6, 0), 2), Some(4));
        assert_eq!(compaction_cut(&conv_with(2, 0), 2), None);
        // user,assistant,user,assistant,user + keep 2 -> 남는 구간이 assistant로 시작하면 user 경계(2)로 당긴다
        assert_eq!(compaction_cut(&conv_with(5, 0), 2), Some(2));
    }

    #[test]
    fn conversation_model_prefers_conversation_then_default() {
        let mut s = AppSettings::default();
        s.model = "qwen3.5-9b".into();
        let mut c = Conversation::default();
        assert_eq!(conversation_model(&s, &c).unwrap(), "qwen3.5-9b");
        c.model_key = Some("qwen3.6-35b-a3b".into());
        assert_eq!(conversation_model(&s, &c).unwrap(), "qwen3.6-35b-a3b");
        s.model.clear();
        c.model_key = None;
        assert!(conversation_model(&s, &c).unwrap_err().contains("사용할 모델을 고르세요"));
        c.model_key = Some("  ".into());
        assert!(conversation_model(&s, &c).is_err());
    }

    #[test]
    fn identity_is_any_lm_desktop() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["productName"], "Any LM Desktop");
        assert_eq!(conf["identifier"], "io.github.kidarigun.anylm-desktop");
        assert_eq!(conf["version"], "0.1.0");
        assert_eq!(
            conf["plugins"]["updater"]["endpoints"],
            serde_json::json!(["https://github.com/kidarigun/any-lm-desktop/releases/latest/download/latest.json"])
        );
        assert!(conf["plugins"]["updater"]["pubkey"].as_str().is_some_and(|s| !s.trim().is_empty()));
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], serde_json::json!(true));
        let text = include_str!("../tauri.conf.json");
        assert!(!text.contains("EXAONE") && !text.contains("exaone"));
    }

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
        let prod = ipc_guard::app_origins(false, None, false);
        let dev = ipc_guard::app_origins(true, Some(&tauri::Url::parse("http://localhost:1420").unwrap()), false);
        let ok = |u: &str, origins: &[String]| is_app_url(&tauri::Url::parse(u).unwrap(), origins);
        assert!(ok("tauri://localhost/index.html", &prod));
        assert!(ok("http://tauri.localhost/", &prod));
        assert!(ok("http://tauri.localhost:80/", &prod), "기본 포트는 포트 없음과 같다");
        assert!(!ok("http://tauri.localhost:5173/x", &prod));
        assert!(!ok("https://tauri.localhost/", &prod), "useHttpsScheme이 아니면 https는 앱이 아니다");
        assert!(ok("https://tauri.localhost/", &ipc_guard::app_origins(false, None, true)));
        assert!(!ok("http://anylm-preview.localhost:12345/token/site/index.html", &prod));
        assert!(!ok("https://evil.example.com/", &prod));
        assert!(!ok("http://tauri.localhost.evil.com/", &prod));
        assert!(!ok("file:///C:/Windows/win.ini", &prod));
        assert!(!ok("javascript:alert(1)", &prod));
        assert!(!ok("http://localhost:1420/", &prod));
        assert!(ok("http://localhost:1420/settings", &dev));
        assert!(!ok("https://localhost:1420/", &dev));
        assert!(!ok("http://127.0.0.1:1420/", &dev));
        assert!(!ok("http://tauri.localhost/", &dev));
        assert!(!ok("http://localhost:9999/", &dev));
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
