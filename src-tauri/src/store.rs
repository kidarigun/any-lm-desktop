use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// 실제 키는 Windows 자격 증명 관리자에 둔다. 설정 파일에도, 화면(웹뷰)에도 내보내지 않는다.
    /// 화면에서 받을 때만 쓰며, 비어 있으면 "기존 키 유지"를 뜻한다.
    #[serde(default, skip_serializing)]
    pub api_key: String,
    /// 저장된 키가 있는지 (화면 표시용)
    #[serde(default)]
    pub api_key_set: bool,
    pub base_url: String,
    pub model: String,
    pub reasoning_effort: String, // "low" | "high" | "max"
    pub writing_style: String,    // "default" | "executive_report" | "technical_doc" | "creative_essay" | "code_architect"
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_key_set: false,
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
    /// 에이전트 루프에서 이 도구가 호출된 라운드 번호 (API 히스토리 재구성용)
    #[serde(default)]
    pub round: u32,
    /// run_python 실행으로 새로 생기거나 바뀐 파일 (화면에서 바로 열 수 있게)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub produced_files: Vec<String>,
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
    /// 라운드별 어시스턴트 텍스트. `content`는 이 값들을 합친 표시용 문자열이다.
    #[serde(default)]
    pub round_contents: Vec<String>,
    /// 라운드별 추론 내용. 이전 추론을 모델에 돌려보낼 때만 채운다 (`round_contents`와 같은 순서).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_reasonings: Vec<String>,
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
    /// 바이너리 파일(docx, pptx 등)의 이전 내용을 담은 백업 파일 이름 (앱 데이터 폴더의 blobs/ 아래)
    #[serde(default)]
    pub backup_blob: Option<String>,
    /// 새 내용이 바이너리라 `new_content`에 텍스트를 담지 않은 경우
    #[serde(default)]
    pub is_binary: bool,
    /// 바이너리 파일의 바뀐 뒤 내용을 담은 백업 파일 이름 (변경 보기용)
    #[serde(default)]
    pub after_blob: Option<String>,
    /// 누가 바꿨는지: "agent"(AI 도구) | "user"(캔버스 저장 등). 예전 기록은 없음.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl FileSnapshot {
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }
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
    /// 처음 실행할 때 기본 프로젝트로 쓸 폴더 (없으면 현재 폴더)
    default_workspace: Option<PathBuf>,
}

impl StoreManager {
    pub fn new(base_dir: &Path) -> Self {
        let _ = fs::create_dir_all(base_dir);
        let db_path = base_dir.join("exaone_desktop_store.json");
        Self { db_path, default_workspace: None }
    }

    pub fn with_default_workspace(mut self, dir: PathBuf) -> Self {
        self.default_workspace = Some(dir);
        self
    }

    pub fn load(&self) -> AppDatabase {
        if let Ok(raw) = fs::read_to_string(&self.db_path) {
            if let Ok(mut db) = serde_json::from_str::<AppDatabase>(&raw) {
                // 승인된 명령이 실행되던 중 앱이 종료된 경우 상태를 정리한다.
                for conv in &mut db.conversations {
                    for m in &mut conv.messages {
                        for r in &mut m.tool_records {
                            if r.status == "running" {
                                r.status = "error".to_string();
                                r.output = "실행 도중 앱이 종료되어 결과를 확인할 수 없습니다.".to_string();
                            }
                        }
                    }
                }
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
        let default_root = match &self.default_workspace {
            Some(dir) if fs::create_dir_all(dir).is_ok() => dir.to_string_lossy().to_string(),
            _ => {
                let mut cwd = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
                if cwd.ends_with("src-tauri") {
                    if let Some(parent) = cwd.parent() {
                        cwd = parent.to_path_buf();
                    }
                }
                cwd.to_string_lossy().to_string()
            }
        };

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
