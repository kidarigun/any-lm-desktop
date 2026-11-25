use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 지원하는 로컬 LLM 공급자
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    #[default]
    Lmstudio,
    Ollama,
    OpenaiCompat,
}

impl LlmProvider {
    pub const ALL: [LlmProvider; 3] = [
        LlmProvider::Lmstudio,
        LlmProvider::Ollama,
        LlmProvider::OpenaiCompat,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Lmstudio => "lmstudio",
            Self::Ollama => "ollama",
            Self::OpenaiCompat => "openai_compat",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lmstudio => "LM Studio",
            Self::Ollama => "Ollama",
            Self::OpenaiCompat => "OpenAI 호환",
        }
    }

    pub fn default_server_url(self) -> &'static str {
        match self {
            Self::Lmstudio => "http://localhost:1234",
            Self::Ollama => "http://localhost:11434",
            Self::OpenaiCompat => "http://localhost:8080",
        }
    }

    pub fn secret_account(self) -> &'static str {
        match self {
            Self::Lmstudio => "lm-studio-api-token",
            Self::Ollama => "ollama-api-token",
            Self::OpenaiCompat => "openai-compat-api-token",
        }
    }

    pub fn from_id(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "lmstudio" | "lm-studio" | "lm_studio" => Some(Self::Lmstudio),
            "ollama" => Some(Self::Ollama),
            "openai_compat" | "openai-compat" | "openai" => Some(Self::OpenaiCompat),
            _ => None,
        }
    }
}

/// 공급자별 저장 프로필 (공급자를 바꿔도 주소·기본 모델·토큰 상태·모델 설정을 따로 기억한다).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderProfile {
    pub server_url: String,
    pub model: String,
    pub api_key_set: bool,
    pub model_prefs: BTreeMap<String, ModelPref>,
}

/// 모델별 사용자 선택값
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelPref {
    pub context_length: Option<u32>,
    /// "on" | "off"
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// 활성 공급자의 API 토큰(선택). Windows 자격 증명 관리자에 둔다. 설정 파일에도, 화면(웹뷰)에도 내보내지 않는다.
    /// 화면에서 받을 때만 쓰며, 비어 있으면 "기존 토큰 유지"를 뜻한다.
    #[serde(skip_serializing)]
    pub api_key: String,
    /// 저장된 토큰이 있는지 (화면 표시용)
    pub api_key_set: bool,
    /// 현재 활성 LLM 공급자
    pub provider: LlmProvider,
    /// 활성 공급자 서버 주소 (localhost / 127.0.0.1 / [::1] 만 허용)
    pub server_url: String,
    /// 기본 모델 key. 비어 있으면 아직 고르지 않은 것.
    pub model: String,
    pub writing_style: String, // "default" | "executive_report" | "technical_doc" | "creative_essay" | "code_architect"
    /// 컨텍스트가 차면 자동으로 요약 압축한다.
    pub auto_compact: bool,
    pub model_prefs: BTreeMap<String, ModelPref>,
    /// 공급자별 설정 프로필. 최상위 server_url/model/api_key_set/model_prefs와 활성 공급자를 동기화한다.
    pub providers: BTreeMap<LlmProvider, ProviderProfile>,
    /// 요청에 맞는 Skill을 앱이 골라 미리 넣는다(결정 0007). 끄면 모델이 load_skill을 직접 불러야 한다.
    pub skill_auto_load: bool,
    /// 작업 패널 터미널의 셸: "powershell"(기본) | "pwsh" | "cmd" (결정 0008)
    pub terminal_shell: String,
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
            provider: LlmProvider::Lmstudio,
            server_url: LlmProvider::Lmstudio.default_server_url().to_string(),
            model: String::new(),
            writing_style: "executive_report".to_string(),
            auto_compact: true,
            model_prefs: BTreeMap::new(),
            providers: BTreeMap::new(),
            skill_auto_load: true,
            terminal_shell: "powershell".to_string(),
            check_updates: true,
        }
    }
}

impl AppSettings {
    /// 모든 공급자 프로필이 존재하고 기본 서버 주소를 갖도록 보장한다.
    /// 예전 저장 파일(providers 없음)에서 읽은 최상위 값은 활성 공급자 프로필로 이관한다.
    pub fn ensure_provider_profiles(&mut self) {
        let had_active = self.providers.contains_key(&self.provider);
        for p in LlmProvider::ALL {
            let entry = self.providers.entry(p).or_insert_with(|| ProviderProfile {
                server_url: p.default_server_url().to_string(),
                ..Default::default()
            });
            if entry.server_url.trim().is_empty() {
                entry.server_url = p.default_server_url().to_string();
            }
        }
        if !had_active {
            if let Some(active) = self.providers.get_mut(&self.provider) {
                if !self.server_url.trim().is_empty() {
                    active.server_url = self.server_url.clone();
                }
                active.model = self.model.clone();
                active.api_key_set = self.api_key_set;
                active.model_prefs = self.model_prefs.clone();
            }
        }
        if self.server_url.trim().is_empty() {
            self.server_url = self.providers[&self.provider].server_url.clone();
        }
    }

    /// 최상위 활성 필드(server_url/model/api_key_set/model_prefs)를 활성 공급자 프로필에 저장한다.
    pub fn sync_active_to_profile(&mut self) {
        self.ensure_provider_profiles();
        if self.server_url.trim().is_empty() {
            self.server_url = self.provider.default_server_url().to_string();
        }
        if let Some(active) = self.providers.get_mut(&self.provider) {
            active.server_url = self.server_url.clone();
            active.model = self.model.clone();
            active.api_key_set = self.api_key_set;
            active.model_prefs = self.model_prefs.clone();
        }
    }

    /// 활성 공급자 프로필 값을 최상위 필드로 복사한다.
    pub fn sync_profile_to_active(&mut self) {
        self.ensure_provider_profiles();
        if let Some(active) = self.providers.get(&self.provider).cloned() {
            self.server_url = active.server_url;
            self.model = active.model;
            self.api_key_set = active.api_key_set;
            self.model_prefs = active.model_prefs;
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
    /// 프로젝트 메모리: 모델(remember 도구)과 사용자가 함께 쓰는 짧은 사실 목록. 오래된 것이 앞.
    #[serde(default)]
    pub memory: Vec<MemoryItem>,
    /// 지식 폴더(절대 경로). 앱은 읽기만 하고, 색인은 앱 데이터 폴더 `knowledge/<id>.json`에 둔다.
    #[serde(default)]
    pub knowledge_folders: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dev_servers: Vec<crate::dev_servers::DevServerConfig>,
}

/// 프로젝트 메모리 항목 하나
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub updated_at: String,
    /// "model" | "user"
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// 모델이 부른 것이 아니라 앱이 스스로 한 일(Skill 자동 불러오기)을 화면에 보여 주는 기록.
    /// 다음 요청의 대화 기록(도구 호출)으로는 보내지 않는다.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub automatic: bool,
}

/// 답변 하나를 만드는 데 든 호출·토큰·시간
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStats {
    /// 이번 답변에 서버로 보낸 요청 수.
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
    /// 이 답변을 만든 모델 key (없으면 생략)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_key: Option<String>,
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

/// 대화 중간에 이전 메시지를 요약·대체한 컨텍스트 압축 정보.
/// 원본 `messages`는 화면 표시·되돌리기용으로 유지하고, 모델 요청 시 `messages[..covered_messages]`를 `summary`로 대체한다.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompaction {
    pub covered_messages: usize,
    pub created_at: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_chars: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// 이전 대화 컨텍스트 압축 상태 (없으면 전체 대화 전송)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<ContextCompaction>,
    /// 이 대화에서 쓸 모델 key. 없으면 설정의 기본 모델을 쓴다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_key: Option<String>,
    /// `model_key`를 고른 공급자. 지금 공급자와 다르면 `model_key`는 쓰지 않는다(다른 서버의 모델 이름이다).
    /// 이 항목이 없는 예전 대화는 LM Studio에서 고른 것으로 본다(`model_key_for`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<LlmProvider>,
}

impl Conversation {
    /// 지금 공급자에서 쓸 수 있는 대화 모델 key. 다른 공급자에서 고른 것이면 `None`.
    pub fn model_key_for(&self, provider: LlmProvider) -> Option<&str> {
        let owner = self.model_provider.unwrap_or(LlmProvider::Lmstudio);
        self.model_key.as_deref().filter(|_| owner == provider)
    }
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
        self.settings.ensure_provider_profiles();
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

const STORE_NAME: &str = "anylm_desktop_store.json";
// 이전 Any LM 설치에서 쓰던 파일명. 같은 앱 데이터 폴더 안에서만 읽는다.
const LEGACY_STORE_NAME: &str = "exaone_desktop_store.json";

impl StoreManager {
    pub fn new(base_dir: &Path) -> Self {
        let _ = fs::create_dir_all(base_dir);
        let db_path = base_dir.join(STORE_NAME);
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
        if let Some(parent) = self.db_path.parent() {
            let legacy = parent.join(LEGACY_STORE_NAME);
            let read_previous = |path: &Path| {
                path.symlink_metadata().ok().filter(|m| m.is_file())?;
                Self::read_db(path)
            };
            if let Some(mut db) = read_previous(&legacy).or_else(|| read_previous(&legacy.with_extension("bak.json"))) {
                db.normalize_after_load();
                if let Err(e) = self.save(&db) {
                    eprintln!("Any LM 저장 파일을 새 이름으로 저장하지 못했습니다: {e}");
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
            name: "기본 워크스페이스".to_string(),
            root_path: default_root,
            instructions: "당신은 최고 수준의 한국어 글쓰기·보고서·문서 작성 및 로컬 코딩 에이전트입니다. 사용자의 프로젝트 폴더 내 파일들을 직접 읽고, 작성하고, 수정하여 완성도 높은 결과물을 만드세요.".to_string(),
            created_at: chrono::Local::now().to_rfc3339(),
            auto_approve: Vec::new(),
            memory: Vec::new(),
            knowledge_folders: Vec::new(),
            dev_servers: Vec::new(),
        };

        let mut settings = AppSettings::default();
        settings.ensure_provider_profiles();
        let db = AppDatabase {
            settings,
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
    #[test]
    fn default_settings_point_to_lm_studio() {
        let s = AppSettings::default();
        assert_eq!(s.server_url, "http://localhost:1234");
        assert_eq!(s.model, "");
        assert!(s.auto_compact);
        assert!(s.model_prefs.is_empty());
    }

    #[test]
    fn api_key_is_never_serialized() {
        let s = AppSettings { api_key: "secret-token".into(), api_key_set: true, ..AppSettings::default() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("secret-token"));
        assert!(!json.contains("apiKey\""));
        assert!(json.contains("\"apiKeySet\":true"));
        // 화면에서 보낸 토큰은 받을 수 있어야 한다.
        let back: AppSettings =
            serde_json::from_str(r#"{"apiKey":"new","serverUrl":"http://localhost:1234","model":"m"}"#).unwrap();
        assert_eq!(back.api_key, "new");
    }

    #[test]
    fn old_settings_file_loads_with_defaults() {
        let back: AppSettings = serde_json::from_str(
            r#"{"baseUrl":"https://api.exaone.ai/v1/chat/completions","model":"k-exaone_v2.5","reasoningEffort":"high","writingStyle":"default","checkUpdates":true,"apiKeySavedAt":"x"}"#,
        )
        .unwrap();
        assert_eq!(back.server_url, "http://localhost:1234");
        assert!(back.auto_compact);
        assert_eq!(back.writing_style, "default");
        assert!(back.skill_auto_load, "예전 설정 파일에는 없으니 기본으로 켠다");
        assert!(back.check_updates, "예전 설정 파일에는 없으니 기본으로 켠다");
        let off: AppSettings = serde_json::from_str(r#"{"checkUpdates":false}"#).unwrap();
        assert!(!off.check_updates);
    }

    #[test]
    fn skill_auto_load_setting_and_automatic_records_round_trip() {
        let s: AppSettings = serde_json::from_str(r#"{"skillAutoLoad":false}"#).unwrap();
        assert!(!s.skill_auto_load);
        assert!(serde_json::to_string(&AppSettings::default()).unwrap().contains("\"skillAutoLoad\":true"));
        // 자동 기록 표시는 저장했다가 다시 열어도 남고, 보통 기록에는 붙지 않는다.
        let r = ToolExecutionRecord { id: "a".into(), tool_name: "load_skill".into(), automatic: true, ..Default::default() };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"automatic\":true"));
        assert!(serde_json::from_str::<ToolExecutionRecord>(&json).unwrap().automatic);
        let plain = serde_json::to_string(&ToolExecutionRecord::default()).unwrap();
        assert!(!plain.contains("automatic"));
    }

    #[test]
    fn model_pref_uses_camel_case() {
        let mut s = AppSettings::default();
        s.model_prefs.insert("qwen".into(), ModelPref { context_length: Some(32768), reasoning: Some("off".into()) });
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""modelPrefs":{"qwen":{"contextLength":32768,"reasoning":"off"}}"#));
    }

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
            ..Default::default()
        });
        store.save(&db).unwrap();
        assert!(!store.db_path.with_extension("json.tmp").exists(), "임시 파일은 남지 않는다");

        // 다시 열면 백업이 생긴다.
        assert_eq!(store.load().conversations.len(), 1);
        assert!(store.backup_path().is_file());

        // 저장 파일이 깨져도 빈 상태로 덮어쓰지 않고, 깨진 파일을 보관한 뒤 백업에서 되살린다.
        fs::write(&store.db_path, "{\"settings\": {").unwrap();
        let restored = store.load();
        assert_eq!(restored.conversations[0].title, "지킬 대화");
        let kept: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt-"))
            .collect();
        assert_eq!(kept.len(), 1, "깨진 파일은 따로 보관한다");
    }

    #[test]
    fn anylm_loads_its_old_store_and_writes_the_new_filename() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("exaone_desktop_store.json");
        let old_data = br#"{"settings":{},"projects":[],"conversations":[{"id":"saved","projectId":"general-chat","title":"old chat","messages":[],"updatedAt":"2026-11-24"}],"snapshots":[]}"#;
        fs::write(&old, old_data).unwrap();
        let store = StoreManager::new(dir.path());
        let mut db = store.load();
        assert_eq!(db.conversations[0].id, "saved");
        assert!(dir.path().join("anylm_desktop_store.json").is_file());
        assert_eq!(fs::read(&old).unwrap(), old_data);

        db.conversations[0].title = "updated chat".to_string();
        store.save(&db).unwrap();
        assert_eq!(store.load().conversations[0].title, "updated chat");
        assert_eq!(fs::read(&old).unwrap(), old_data);
    }

    /// 실제로 쓰던 저장 파일의 사본이 새 코드에서 그대로 읽히는지 본다.
    /// 실행: ANYLM_REAL_STORE=<저장 파일 경로> cargo test --lib store::tests::reads_a_real_store -- --ignored --nocapture
    #[test]
    #[ignore = "ANYLM_REAL_STORE에 저장 파일 경로를 줘야 한다"]
    fn reads_a_real_store_file() {
        let src = std::env::var("ANYLM_REAL_STORE").expect("ANYLM_REAL_STORE");
        let dir = tempfile::tempdir().unwrap();
        fs::copy(&src, dir.path().join(LEGACY_STORE_NAME)).unwrap();
        let store = StoreManager::new(dir.path());
        let db = store.load();
        let messages: usize = db.conversations.iter().map(|c| c.messages.len()).sum();
        println!("대화 {}개, 메시지 {}개, 되돌리기 기록 {}개, 프로젝트 {}개", db.conversations.len(), messages, db.snapshots.len(), db.projects.len());
        assert!(!db.conversations.is_empty(), "사본을 읽지 못하고 빈 상태가 되었다");
        store.save(&db).unwrap();
        let before = fs::metadata(&src).unwrap().len();
        let after = fs::metadata(&store.db_path).unwrap().len();
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
            ..Default::default()
        });
        store.save(&db).unwrap();
        let m = &store.load().conversations[0].messages[0];
        assert!(!m.incomplete);
        assert!(m.content.starts_with("쓰다 만 답변") && m.content.contains("앱이 종료되어"), "{}", m.content);
    }

    #[test]
    fn legacy_settings_migrate_into_provider_profiles_and_round_trip() {
        let mut s: AppSettings = serde_json::from_str(
            r#"{"serverUrl":"http://127.0.0.1:2345","model":"qwen3.5-9b","apiKeySet":true,"modelPrefs":{"qwen3.5-9b":{"contextLength":16384,"reasoning":"off"}}}"#,
        )
        .unwrap();
        assert_eq!(s.provider, LlmProvider::Lmstudio);
        s.ensure_provider_profiles();

        let lm = &s.providers[&LlmProvider::Lmstudio];
        assert_eq!(lm.server_url, "http://127.0.0.1:2345");
        assert_eq!(lm.model, "qwen3.5-9b");
        assert!(lm.api_key_set);
        assert_eq!(lm.model_prefs["qwen3.5-9b"].context_length, Some(16384));
        assert_eq!(s.providers[&LlmProvider::Ollama].server_url, "http://localhost:11434");
        assert_eq!(s.providers[&LlmProvider::OpenaiCompat].server_url, "http://localhost:8080");

        // Ollama로 전환해 값을 설정한 뒤 다시 LM Studio로 돌아와도 기존 값이 남는다.
        s.provider = LlmProvider::Ollama;
        s.sync_profile_to_active();
        assert_eq!(s.server_url, "http://localhost:11434");
        assert_eq!(s.model, "");
        assert!(!s.api_key_set);
        s.model = "qwen3:8b".to_string();
        s.model_prefs.insert("qwen3:8b".into(), ModelPref { context_length: Some(32768), reasoning: None });
        s.sync_active_to_profile();

        s.provider = LlmProvider::Lmstudio;
        s.sync_profile_to_active();
        assert_eq!(s.server_url, "http://127.0.0.1:2345");
        assert_eq!(s.model, "qwen3.5-9b");
        assert!(s.api_key_set);
        assert_eq!(s.providers[&LlmProvider::Ollama].model, "qwen3:8b");
    }
}
