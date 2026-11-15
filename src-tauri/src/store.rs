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
    /// API 키를 앱에 입력한 시각. K-EXAONE 키는 발급 30일 뒤 만료되므로 만료가 가까우면 알려 준다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_saved_at: Option<String>,
    /// 앱을 켤 때 새 버전이 있는지 확인할지 (확인만 하고, 설치는 사용자가 누를 때 한다)
    #[serde(default = "default_true")]
    pub check_updates: bool,
}

fn default_true() -> bool {
    true
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
            api_key_saved_at: None,
            check_updates: true,
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
    /// 이 프로젝트에서 묻지 않고 실행하도록 사용자가 허락한 도구 ("run_python", "run_command")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_approve: Vec<String>,
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

/// 답변 하나를 만드는 데 든 호출·토큰·시간
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStats {
    /// 서버에 보낸 요청 수 (하루 호출 한도에서 빠지는 수)
    pub api_calls: u32,
    /// 마지막 요청의 입력 토큰 수 (대화가 지금 얼마나 긴지)
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub reasoning_tokens: u64,
    /// 응답을 기다리고 받은 시간의 합 (승인을 기다린 시간은 빼고)
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// 라운드별 추론이 `reasoning_content` 안에서 차지하는 구간 [시작, 길이] (바이트).
    /// 추론을 두 벌 저장하지 않고도 이전 추론을 라운드별로 모델에 돌려보내기 위함이다.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_reasoning_spans: Vec<[usize; 2]>,
    /// 0.2.0이 쓰던 라운드별 추론 사본. 읽을 때 구간으로 바꾸고 다시 저장하지 않는다.
    #[serde(default, skip_serializing, rename = "roundReasonings")]
    pub legacy_round_reasonings: Vec<String>,
    /// 답변을 받는 도중에 저장된 상태인지 (앱이 꺼져도 여기까지는 남는다)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub incomplete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<TurnStats>,
}

impl ChatMessage {
    /// `round`번째 라운드의 추론
    pub fn round_reasoning(&self, round: usize) -> Option<&str> {
        let [start, len] = *self.round_reasoning_spans.get(round)?;
        if len == 0 {
            return None;
        }
        self.reasoning_content.as_deref()?.get(start..start + len)
    }

    /// `round`번째 라운드의 추론을 덧붙인다 (라운드는 순서대로만 늘어난다).
    pub fn push_round_reasoning(&mut self, round: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let all = self.reasoning_content.get_or_insert_with(String::new);
        if !all.is_empty() {
            all.push_str("\n\n");
        }
        let start = all.len();
        all.push_str(text);
        let end = all.len();
        if self.round_reasoning_spans.len() <= round {
            self.round_reasoning_spans.resize(round + 1, [0, 0]);
        }
        let span = &mut self.round_reasoning_spans[round];
        *span = if span[1] == 0 { [start, end - start] } else { [span[0], end - span[0]] };
    }

    /// 0.2.0 형식(라운드별 사본)을 구간 형식으로 바꾼다.
    fn migrate_round_reasonings(&mut self) {
        let old = std::mem::take(&mut self.legacy_round_reasonings);
        if old.is_empty() || !self.round_reasoning_spans.is_empty() {
            return;
        }
        let joined = old.iter().filter(|r| !r.is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
        if self.reasoning_content.as_deref() != Some(joined.as_str()) {
            return; // 모양이 다르면 라운드 정보 없이 둔다 (화면 표시는 그대로)
        }
        let mut pos = 0;
        for r in &old {
            if r.is_empty() {
                self.round_reasoning_spans.push([0, 0]);
                continue;
            }
            if pos > 0 {
                pos += 2;
            }
            self.round_reasoning_spans.push([pos, r.len()]);
            pos += r.len();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub updated_at: String,
    /// 마지막으로 메시지를 보낸 화면 ("chat" | "project" | "code"). 화면에 맞는 지침을 고르는 데 쓴다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
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

/// 되돌리기 기록을 이만큼만 둔다. 기록마다 수정 전·후 내용을 통째로 담기 때문에 그냥 두면 계속 커진다.
pub const MAX_SNAPSHOTS: usize = 300;
/// 되돌리기 기록에 담긴 글의 총량 상한 (글자 수가 아니라 바이트)
pub const MAX_SNAPSHOT_BYTES: usize = 20 * 1024 * 1024;

impl AppDatabase {
    /// 오래된 되돌리기 기록부터 지워 개수와 크기를 상한 아래로 맞춘다.
    /// 지운 기록이 쓰던 백업 파일 이름을 돌려준다 (호출한 쪽에서 파일을 지운다).
    pub fn prune_snapshots(&mut self) -> Vec<String> {
        let size = |s: &FileSnapshot| s.previous_content.len() + s.new_content.len();
        let mut total: usize = self.snapshots.iter().map(size).sum();
        let mut drop = 0;
        while drop < self.snapshots.len()
            && (self.snapshots.len() - drop > MAX_SNAPSHOTS || total > MAX_SNAPSHOT_BYTES)
        {
            total -= size(&self.snapshots[drop]);
            drop += 1;
        }
        self.snapshots
            .drain(..drop)
            .flat_map(|s| [s.backup_blob, s.after_blob])
            .flatten()
            .collect()
    }

    /// 파일에서 읽은 직후의 정리: 실행 중이던 작업, 받다 만 답변, 예전 저장 형식
    fn normalize_after_load(&mut self) {
        for conv in &mut self.conversations {
            for m in &mut conv.messages {
                m.migrate_round_reasonings();
                // 승인된 명령이 실행되던 중 앱이 종료된 경우 상태를 정리한다.
                for r in &mut m.tool_records {
                    if r.status == "running" {
                        r.status = "error".to_string();
                        r.output = "실행 도중 앱이 종료되어 결과를 확인할 수 없습니다.".to_string();
                    }
                }
                if m.incomplete {
                    m.incomplete = false;
                    let note = "_(답변을 받는 도중 앱이 종료되어 여기서 멈췄습니다. \"계속\"이라고 입력하면 이어서 작성합니다.)_";
                    if m.content.trim().is_empty() {
                        m.content = note.to_string();
                    } else {
                        m.content.push_str(&format!("\n\n{}", note));
                    }
                    if m.round_contents.is_empty() {
                        m.round_contents.push(m.content.clone());
                    } else {
                        m.round_contents.push(note.to_string());
                    }
                }
            }
        }
        for p in &mut self.projects {
            if p.root_path.ends_with("\\src-tauri") || p.root_path.ends_with("/src-tauri") {
                if let Some(parent) = Path::new(&p.root_path).parent() {
                    p.root_path = parent.to_string_lossy().to_string();
                }
            }
        }
    }
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

    fn backup_path(&self) -> PathBuf {
        self.db_path.with_extension("bak.json")
    }

    fn read_db(path: &Path) -> Option<AppDatabase> {
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str::<AppDatabase>(&raw).ok()
    }

    pub fn load(&self) -> AppDatabase {
        if let Some(mut db) = Self::read_db(&self.db_path) {
            db.normalize_after_load();
            // 잘 읽힌 파일을 백업으로 남긴다. 다음에 저장 파일이 깨져도 여기까지는 되살릴 수 있다.
            let _ = fs::copy(&self.db_path, self.backup_path());
            return db;
        }
        if self.db_path.exists() {
            // 저장 파일이 깨졌다: 덮어쓰지 않고 따로 보관한 뒤 백업에서 되살린다.
            let kept = self
                .db_path
                .with_extension(format!("corrupt-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S")));
            let _ = fs::rename(&self.db_path, kept);
            if let Some(mut db) = Self::read_db(&self.backup_path()) {
                db.normalize_after_load();
                let _ = self.save(&db);
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
            auto_approve: Vec::new(),
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

    /// 임시 파일에 다 쓴 뒤 이름을 바꿔 교체한다. 쓰는 도중 앱이 꺼져도 원래 파일은 온전하다.
    pub fn save(&self, db: &AppDatabase) -> Result<(), String> {
        use std::io::Write;
        let json = serde_json::to_vec(db).map_err(|e| e.to_string())?;
        let tmp = self.db_path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp).map_err(|e| format!("저장 파일을 만들 수 없습니다: {}", e))?;
            f.write_all(&json).map_err(|e| format!("저장하지 못했습니다: {}", e))?;
            f.sync_all().map_err(|e| format!("저장하지 못했습니다: {}", e))?;
        }
        fs::rename(&tmp, &self.db_path).map_err(|e| format!("저장 파일을 교체하지 못했습니다: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: &str, before: &str, after: &str, blob: Option<&str>) -> FileSnapshot {
        FileSnapshot {
            id: id.into(),
            project_id: "p".into(),
            relative_path: "a.md".into(),
            existed_before: true,
            previous_content: before.into(),
            new_content: after.into(),
            action: "write".into(),
            created_at: String::new(),
            backup_blob: blob.map(str::to_string),
            is_binary: false,
            after_blob: None,
            source: None,
        }
    }

    #[test]
    fn save_is_atomic_and_a_broken_file_is_restored_from_backup() {
        let dir = tempfile::tempdir().unwrap();
        let store = StoreManager::new(dir.path()).with_default_workspace(dir.path().join("ws"));
        let mut db = store.load();
        db.conversations.push(Conversation {
            id: "c1".into(),
            project_id: "general-chat".into(),
            title: "지킬 대화".into(),
            messages: vec![],
            updated_at: String::new(),
            mode: None,
        });
        store.save(&db).unwrap();
        assert!(!dir.path().join("exaone_desktop_store.json.tmp").exists(), "임시 파일은 남지 않는다");

        // 다시 열면 백업이 생긴다.
        assert_eq!(store.load().conversations.len(), 1);
        assert!(store.backup_path().is_file());

        // 저장 파일이 깨져도 빈 상태로 덮어쓰지 않고, 깨진 파일을 보관한 뒤 백업에서 되살린다.
        fs::write(dir.path().join("exaone_desktop_store.json"), "{\"settings\": {").unwrap();
        let restored = store.load();
        assert_eq!(restored.conversations[0].title, "지킬 대화");
        let kept: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt-"))
            .collect();
        assert_eq!(kept.len(), 1, "깨진 파일은 따로 보관한다");
    }

    /// 실제로 쓰던 저장 파일의 사본이 새 코드에서 그대로 읽히는지 본다.
    /// 실행: EXA_REAL_STORE=<저장 파일 경로> cargo test --lib store::tests::reads_a_real_store -- --ignored --nocapture
    #[test]
    #[ignore = "EXA_REAL_STORE에 저장 파일 경로를 줘야 한다"]
    fn reads_a_real_store_file() {
        let src = std::env::var("EXA_REAL_STORE").expect("EXA_REAL_STORE");
        let dir = tempfile::tempdir().unwrap();
        fs::copy(&src, dir.path().join("exaone_desktop_store.json")).unwrap();
        let store = StoreManager::new(dir.path());
        let db = store.load();
        let messages: usize = db.conversations.iter().map(|c| c.messages.len()).sum();
        println!("대화 {}개, 메시지 {}개, 되돌리기 기록 {}개, 프로젝트 {}개", db.conversations.len(), messages, db.snapshots.len(), db.projects.len());
        assert!(!db.conversations.is_empty(), "사본을 읽지 못하고 빈 상태가 되었다");
        store.save(&db).unwrap();
        let before = fs::metadata(&src).unwrap().len();
        let after = fs::metadata(dir.path().join("exaone_desktop_store.json")).unwrap().len();
        println!("파일 크기: {} → {} 바이트", before, after);
        assert_eq!(store.load().conversations.len(), db.conversations.len());
    }

    #[test]
    fn old_snapshots_are_pruned_by_count_and_size() {
        let mut db = AppDatabase::default();
        for i in 0..MAX_SNAPSHOTS + 5 {
            db.snapshots.push(snap(&format!("s{}", i), "a", "b", (i == 0).then_some("old.bin")));
        }
        let blobs = db.prune_snapshots();
        assert_eq!(db.snapshots.len(), MAX_SNAPSHOTS);
        assert_eq!(db.snapshots[0].id, "s5", "오래된 것부터 지운다");
        assert_eq!(blobs, vec!["old.bin".to_string()]);

        let big = "x".repeat(MAX_SNAPSHOT_BYTES / 2);
        let mut db = AppDatabase::default();
        for i in 0..3 {
            db.snapshots.push(snap(&format!("b{}", i), &big, "", None));
        }
        db.prune_snapshots();
        assert_eq!(db.snapshots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), vec!["b1", "b2"]);
    }

    #[test]
    fn round_reasoning_is_sliced_from_one_stored_copy() {
        let mut m = ChatMessage::default();
        m.push_round_reasoning(0, "첫 생각");
        m.push_round_reasoning(2, "셋째 생각");
        m.push_round_reasoning(2, "이어서");
        assert_eq!(m.reasoning_content.as_deref(), Some("첫 생각\n\n셋째 생각\n\n이어서"));
        assert_eq!(m.round_reasoning(0), Some("첫 생각"));
        assert_eq!(m.round_reasoning(1), None);
        assert_eq!(m.round_reasoning(2), Some("셋째 생각\n\n이어서"));
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json.matches("첫 생각").count(), 1, "추론은 한 벌만 저장한다");

        // 0.2.0이 저장한 형식도 읽는다.
        let old = r#"{"id":"m","role":"assistant","content":"답","reasoningContent":"가\n\n나","toolRecords":[],"createdAt":"","roundReasonings":["가","","나"]}"#;
        let mut m: ChatMessage = serde_json::from_str(old).unwrap();
        m.migrate_round_reasonings();
        assert_eq!((m.round_reasoning(0), m.round_reasoning(1), m.round_reasoning(2)), (Some("가"), None, Some("나")));
        assert!(!serde_json::to_string(&m).unwrap().contains("roundReasonings"));
    }

    #[test]
    fn half_received_answer_is_marked_when_the_app_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let store = StoreManager::new(dir.path()).with_default_workspace(dir.path().join("ws"));
        let mut db = store.load();
        db.conversations.push(Conversation {
            id: "c1".into(),
            project_id: "general-chat".into(),
            title: "t".into(),
            messages: vec![ChatMessage {
                id: "a".into(),
                role: "assistant".into(),
                content: "쓰다 만 답변".into(),
                incomplete: true,
                ..Default::default()
            }],
            updated_at: String::new(),
            mode: None,
        });
        store.save(&db).unwrap();
        let m = &store.load().conversations[0].messages[0];
        assert!(!m.incomplete);
        assert!(m.content.starts_with("쓰다 만 답변") && m.content.contains("앱이 종료되어"), "{}", m.content);
    }
}
