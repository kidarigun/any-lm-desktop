use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub reasoning_effort: String, // "low" | "high" | "max"
    pub writing_style: String,    // "default" | "executive_report" | "technical_doc" | "creative_essay" | "code_architect"
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.exaone.ai/v1/chat/completions".to_string(),
            model: "k-exaone_v2.5".to_string(),
            reasoning_effort: "high".to_string(),
            writing_style: "executive_report".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub instructions: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionRecord {
    pub id: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub status: String, // "completed" | "pending_approval" | "rejected" | "error"
    pub output: String,
    pub file_path: Option<String>,
    pub snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String, // "user" | "assistant" | "system" | "tool"
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_records: Vec<ToolExecutionRecord>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSnapshot {
    pub id: String,
    pub project_id: String,
    pub relative_path: String,
    pub existed_before: bool,
    pub previous_content: String,
    pub new_content: String,
    pub action: String, // "write" | "modify" | "delete"
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppDatabase {
    pub settings: AppSettings,
    pub projects: Vec<Project>,
    pub conversations: Vec<Conversation>,
    pub snapshots: Vec<FileSnapshot>,
}

pub struct StoreManager {
    db_path: PathBuf,
}

impl StoreManager {
    pub fn new(base_dir: &Path) -> Self {
        let _ = fs::create_dir_all(base_dir);
        let db_path = base_dir.join("exaone_desktop_store.json");
        Self { db_path }
    }

    pub fn load(&self) -> AppDatabase {
        if let Ok(raw) = fs::read_to_string(&self.db_path) {
            if let Ok(mut db) = serde_json::from_str::<AppDatabase>(&raw) {
                for p in &mut db.projects {
                    if p.root_path.ends_with("\\src-tauri") || p.root_path.ends_with("/src-tauri") {
                        if let Some(parent) = Path::new(&p.root_path).parent() {
                            p.root_path = parent.to_string_lossy().to_string();
                        }
                    }
                }
                return db;
            }
        }
        let mut cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("c:\\DOCS\\EXAONE"));
        if cwd.ends_with("src-tauri") {
            if let Some(parent) = cwd.parent() {
                cwd = parent.to_path_buf();
            }
        }
        let default_root = cwd.to_string_lossy().to_string();

        let default_project = Project {
            id: "proj-default".to_string(),
            name: "EXAONE 기본 워크스페이스".to_string(),
            root_path: default_root,
            instructions: "당신은 LG AI연구원의 K-EXAONE 2.5 모델을 기반으로 동작하는 최고 수준의 한국어 글쓰기·보고서·문서 작성 및 로컬 코딩 에이전트입니다. 사용자의 프로젝트 폴더 내 파일들을 직접 읽고, 작성하고, 수정하여 완성도 높은 결과물을 만드세요.".to_string(),
            created_at: chrono::Local::now().to_rfc3339(),
        };

        let db = AppDatabase {
            settings: AppSettings::default(),
            projects: vec![default_project],
            conversations: Vec::new(),
            snapshots: Vec::new(),
        };
        let _ = self.save(&db);
        db
    }

    pub fn save(&self, db: &AppDatabase) -> Result<(), String> {
        let json = serde_json::to_string_pretty(db).map_err(|e| e.to_string())?;
        fs::write(&self.db_path, json).map_err(|e| e.to_string())
    }
}
