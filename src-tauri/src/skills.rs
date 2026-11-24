//! Skill(작업 지침 묶음) 찾기·읽기·설치.
//!
//! Skill은 SKILL.md(이름·설명 머리말 + 본문 지침)와 함께 쓰는 파일이 든 폴더다 (Agent Skills 형식).
//! - 모든 대화에서 쓰는 Skill: 앱 데이터 폴더의 `skills\<폴더>\`
//! - 한 프로젝트에서만 쓰는 Skill: 프로젝트의 `.skills\<폴더>\` 또는 `.claude\skills\<폴더>\`
//!
//! 모델에는 이름과 설명만 먼저 알려 주고, 필요할 때 `load_skill`로 본문을 읽게 한다.
//! Skill 폴더는 읽기 전용으로만 열어 주며, 안의 스크립트는 `run_python`(사용자 승인)으로만 실행된다.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 프로젝트 안에서 Skill을 찾는 폴더 (프로젝트 루트 기준)
pub const PROJECT_SKILL_DIRS: [&str; 2] = [".skills", ".claude/skills"];
const SKILL_FILE: &str = "SKILL.md";
const STATE_FILE: &str = "_state.json";
const MAX_NAME_CHARS: usize = 64;
const MAX_DESCRIPTION_CHARS: usize = 1024;
const MAX_SKILL_MD_BYTES: u64 = 512 * 1024;
const MAX_READ_BYTES: u64 = 1024 * 1024;
const MAX_LISTED_FILES: usize = 200;
const MAX_LIST_DEPTH: usize = 4;
/// 설치할 때 받아들이는 최대 크기·파일 수
const MAX_INSTALL_BYTES: u64 = 50 * 1024 * 1024;
const MAX_INSTALL_FILES: usize = 2000;
/// 설치할 때 옮기지 않는 폴더
const SKIP_DIRS: [&str; 4] = [".git", "__pycache__", "node_modules", "__MACOSX"];

static USER_DIR: OnceLock<PathBuf> = OnceLock::new();
static STATE_LOCK: Mutex<()> = Mutex::new(());

/// 앱 시작 시 한 번 호출해 모든 대화용 Skill 폴더를 정한다.
pub fn set_user_dir(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = USER_DIR.set(dir);
}

pub fn user_dir() -> PathBuf {
    USER_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("anylm-desktop-skills"))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    /// 켜고 끄기·삭제에 쓰는 고유 ID (`user:<폴더>` 또는 `project:<프로젝트ID>:<상대 경로>`)
    pub id: String,
    pub name: String,
    pub description: String,
    /// "user"(모든 대화) 또는 "project"(이 프로젝트)
    pub scope: String,
    /// 표시용 위치 (사용자 Skill은 폴더 이름, 프로젝트 Skill은 프로젝트 기준 상대 경로)
    pub location: String,
    pub enabled: bool,
    /// SKILL.md에 문제가 있으면 그 내용 (이 경우 모델에 알려 주지 않는다)
    pub error: Option<String>,
    /// 같은 이름의 다른 Skill에 가려졌는지 (프로젝트 Skill이 사용자 Skill보다 우선)
    pub shadowed: bool,
    /// SKILL.md 말고 함께 든 파일 (Skill 폴더 기준 상대 경로)
    pub files: Vec<String>,
    #[serde(skip)]
    pub dir: PathBuf,
}

// ---------------------------------------------------------------------------
// SKILL.md 읽기
// ---------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
pub struct ParsedSkill {
    pub name: Option<String>,
    pub description: Option<String>,
    /// 머리말을 뺀 본문 지침
    pub body: String,
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 {
        if v.starts_with('"') && v.ends_with('"') {
            return v[1..v.len() - 1]
                .replace("\\n", "\n")
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
        }
        if v.starts_with('\'') && v.ends_with('\'') {
            return v[1..v.len() - 1].replace("''", "'");
        }
    }
    v.to_string()
}

/// SKILL.md의 YAML 머리말(`---`로 감싼 부분)에서 name·description을 읽는다.
/// 필요한 키만 읽는 간단한 해석기로, 따옴표 값과 여러 줄 값(`>`, `|`)을 지원한다.
pub fn parse_skill_md(text: &str) -> ParsedSkill {
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return ParsedSkill { body: text.trim().to_string(), ..Default::default() };
    }
    let Some(end) = lines.iter().skip(1).position(|l| l.trim_end() == "---").map(|i| i + 1) else {
        return ParsedSkill { body: text.trim().to_string(), ..Default::default() };
    };

    let mut out = ParsedSkill { body: lines[end + 1..].join("\n").trim().to_string(), ..Default::default() };
    let header = &lines[1..end];
    let mut i = 0;
    while i < header.len() {
        let line = header[i];
        i += 1;
        if line.starts_with(char::is_whitespace) || line.trim_start().starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim();
        let value = value.trim();
        // 여러 줄 값: 다음의 들여쓴 줄들을 모은다.
        let mut block: Vec<&str> = Vec::new();
        while i < header.len() && (header[i].trim().is_empty() || header[i].starts_with(char::is_whitespace)) {
            block.push(header[i].trim());
            i += 1;
        }
        let joined = if matches!(value, "|" | "|-" | "|+") {
            block.join("\n").trim().to_string()
        } else if value.is_empty() || matches!(value, ">" | ">-" | ">+") {
            block.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ")
        } else {
            unquote(value)
        };
        match key {
            "name" if !joined.is_empty() => out.name = Some(joined),
            "description" if !joined.is_empty() => out.description = Some(joined),
            _ => {}
        }
    }
    out
}

fn read_skill_md(dir: &Path) -> Result<String, String> {
    let path = skill_file_in(dir).ok_or_else(|| "SKILL.md가 없습니다.".to_string())?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_SKILL_MD_BYTES {
        return Err(format!("SKILL.md가 너무 큽니다 ({}KB 넘음).", MAX_SKILL_MD_BYTES / 1024));
    }
    let bytes = fs::read(&path).map_err(|e| format!("SKILL.md를 읽지 못했습니다: {}", e))?;
    String::from_utf8(bytes).map_err(|_| "SKILL.md가 UTF-8 텍스트가 아닙니다.".to_string())
}

/// 폴더 안의 SKILL.md (대소문자 구분 없이)
fn skill_file_in(dir: &Path) -> Option<PathBuf> {
    let exact = dir.join(SKILL_FILE);
    if exact.is_file() {
        return Some(exact);
    }
    fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        (p.is_file() && e.file_name().to_string_lossy().eq_ignore_ascii_case(SKILL_FILE)).then_some(p)
    })
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(true) || is_junction(path)
}

#[cfg(windows)]
fn is_junction(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(true)
}

#[cfg(not(windows))]
fn is_junction(_: &Path) -> bool {
    false
}

fn list_files(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if out.len() >= MAX_LISTED_FILES {
                return;
            }
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if is_link(&p) || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            if p.is_dir() {
                if depth < MAX_LIST_DEPTH {
                    walk(base, &p, depth + 1, out);
                }
            } else if let Ok(rel) = p.strip_prefix(base) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if !rel.eq_ignore_ascii_case(SKILL_FILE) {
                    out.push(rel);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, 0, &mut out);
    out
}

fn valid_name(name: &str) -> Result<(), String> {
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("이름이 {}자를 넘습니다.", MAX_NAME_CHARS));
    }
    if name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | '"' | '`')) {
        return Err("이름에 쓸 수 없는 문자가 있습니다.".to_string());
    }
    Ok(())
}

/// 폴더 하나를 Skill로 읽는다. SKILL.md가 없으면 None.
fn inspect(dir: &Path, id: String, scope: &str, location: String, disabled: &HashSet<String>) -> Option<SkillInfo> {
    skill_file_in(dir)?;
    let folder = dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut info = SkillInfo {
        enabled: !disabled.contains(&id),
        id,
        name: folder.clone(),
        description: String::new(),
        scope: scope.to_string(),
        location,
        error: None,
        shadowed: false,
        files: list_files(dir),
        dir: dir.to_path_buf(),
    };
    match read_skill_md(dir) {
        Ok(text) => {
            let parsed = parse_skill_md(&text);
            if let Some(n) = parsed.name {
                info.name = n;
            }
            match parsed.description {
                Some(d) => info.description = d.chars().take(MAX_DESCRIPTION_CHARS).collect(),
                None => {
                    info.error = Some(
                        "SKILL.md 머리말에 description(언제 쓰는 Skill인지)이 없어 모델에게 알려 줄 수 없습니다."
                            .to_string(),
                    )
                }
            }
            if let Err(e) = valid_name(&info.name) {
                info.error = Some(e);
            }
        }
        Err(e) => info.error = Some(e),
    }
    Some(info)
}

fn scan(base: &Path, scope: &str, id_prefix: &str, loc_prefix: &str, disabled: &HashSet<String>) -> Vec<SkillInfo> {
    let Ok(rd) = fs::read_dir(base) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !is_link(p))
        .filter(|p| {
            let n = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str())
        })
        .collect();
    dirs.sort();
    dirs.iter()
        .filter_map(|d| {
            let folder = d.file_name()?.to_string_lossy().to_string();
            inspect(
                d,
                format!("{}{}", id_prefix, folder),
                scope,
                format!("{}{}", loc_prefix, folder),
                disabled,
            )
        })
        .collect()
}

/// 설치된 모든 Skill (꺼진 것·문제 있는 것 포함). 프로젝트 Skill을 먼저 둔다.
/// `project`: (프로젝트 ID, 루트 경로)
pub fn discover(project: Option<(&str, &Path)>) -> Vec<SkillInfo> {
    let disabled = load_state().disabled;
    let mut all = Vec::new();
    if let Some((pid, root)) = project {
        for rel in PROJECT_SKILL_DIRS {
            let base = root.join(rel);
            if base.is_dir() && !is_link(&base) {
                all.extend(scan(&base, "project", &format!("project:{}:{}/", pid, rel), &format!("{}/", rel), &disabled));
            }
        }
    }
    all.extend(scan(&user_dir(), "user", "user:", "", &disabled));

    // 같은 이름은 앞의 것(프로젝트 → 사용자 순)만 쓴다.
    let mut seen: HashSet<String> = HashSet::new();
    for s in all.iter_mut() {
        if s.error.is_some() || !s.enabled {
            continue;
        }
        if !seen.insert(s.name.to_lowercase()) {
            s.shadowed = true;
        }
    }
    all
}

/// 모델에 알려 줄 Skill (켜져 있고, 문제없고, 가려지지 않은 것)
pub fn available(project: Option<(&str, &Path)>) -> Vec<SkillInfo> {
    discover(project)
        .into_iter()
        .filter(|s| s.enabled && s.error.is_none() && !s.shadowed)
        .collect()
}

pub fn find_available(project: Option<(&str, &Path)>, name: &str) -> Option<SkillInfo> {
    let want = name.trim().to_lowercase();
    available(project).into_iter().find(|s| s.name.to_lowercase() == want)
}

fn unknown_skill(project: Option<(&str, &Path)>, name: &str) -> String {
    let names: Vec<String> = available(project).into_iter().map(|s| s.name).collect();
    if names.is_empty() {
        format!("'{}' Skill이 없습니다. 지금 쓸 수 있는 Skill이 없습니다.", name)
    } else {
        format!("'{}' Skill이 없습니다. 쓸 수 있는 Skill: {}", name, names.join(", "))
    }
}

// ---------------------------------------------------------------------------
// 모델에 주는 내용
// ---------------------------------------------------------------------------

/// 시스템 프롬프트에 붙일 Skill 목록. 쓸 수 있는 Skill이 없으면 None.
pub fn prompt_section(skills: &[SkillInfo]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let mut out = String::from(
        "\n\n[Skill]\n사용자가 설치한 작업 지침(Skill)이 있습니다. 요청이 아래 Skill의 설명에 해당하면, 답하거나 작업하기 전에 먼저 load_skill로 그 Skill의 지침을 읽고 그대로 따르세요. 해당하지 않으면 쓰지 마세요.\n질문에 맞는 Skill이 [Skill: …] 로 이미 들어와 있으면 load_skill을 다시 부르지 말고 그 지침을 따르세요.\n",
    );
    for s in skills {
        out.push_str(&format!("- {}: {}\n", s.name, s.description.replace('\n', " ")));
    }
    Some(out)
}

/// `load_skill` 결과: 본문 지침과 함께 든 파일 안내
pub fn load_for_model(skill: &SkillInfo, can_run_python: bool) -> Result<String, String> {
    let text = read_skill_md(&skill.dir)?;
    let parsed = parse_skill_md(&text);
    let mut out = format!("# Skill: {}\n", skill.name);
    if !skill.files.is_empty() {
        out.push_str(&format!(
            "이 Skill 폴더에 든 파일: {}\n- 지침이 가리키는 파일은 read_skill_file(skill=\"{}\", path=...)로 읽으세요. read_file로는 읽을 수 없습니다.\n",
            skill.files.join(", "),
            skill.name
        ));
        if can_run_python && skill.files.iter().any(|f| f.to_lowercase().ends_with(".py")) {
            out.push_str(&format!(
                "- 이 Skill의 .py 스크립트는 run_python(skill=\"{}\", relative_path=\"scripts/...\")으로 실행하세요. 실행 위치는 프로젝트 루트라서, 스크립트에 넘기는 파일 경로와 결과 파일은 프로젝트 기준입니다.\n",
                skill.name
            ));
        }
    }
    out.push_str("\n---\n\n");
    out.push_str(&parsed.body);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Skill 자동 불러오기 (결정 0007)
// ---------------------------------------------------------------------------

/// 설명으로 고를 때 맞아야 하는 서로 다른(의미 있는) 토큰 수의 최소값
pub const SKILL_AUTO_MIN_HITS: usize = 2;
/// 설명으로 고를 때 필요한 비율: (Skill 이름·설명에 든 요청 토큰 수) / (요청의 의미 있는 토큰 수)
pub const SKILL_AUTO_MIN_COVERAGE: f64 = 0.25;
/// 맞은 토큰의 IDF 합이 이 값 이상이어야 고른다. Skill이 하나뿐이어도 IDF는 ln(1+2)=1.10이라 맞은 토큰이 둘은 있어야 한다.
pub const SKILL_AUTO_MIN_WEIGHT: f64 = 2.0;
/// 1등의 IDF 합이 2등보다 이만큼은 앞서야 고른다(비슷하면 고르지 않는다).
pub const SKILL_AUTO_MIN_MARGIN: f64 = 0.5;
/// IDF를 셀 때 설치된 Skill 수에 더하는 "아무 Skill에도 해당하지 않는 일반 요청" 가상 문서 수.
/// Skill이 하나뿐일 때도 모든 토큰의 IDF가 같아지지 않고, 여러 Skill에 흔한 토큰은 덜 친다.
pub const SKILL_AUTO_GENERIC_DOCS: f64 = 1.0;
/// 설명으로 고를 때, 맞은 토큰 중 흔한 낱말(`GENERIC_TOKENS`)이 아닌 것이 Skill 이름 토큰이 아니면 이만큼은 있어야 한다.
/// Skill이 하나뿐이면 IDF가 모두 ln 3이라 `SKILL_AUTO_MIN_WEIGHT`만으로는 "아무 낱말 둘"과 같아서다("회사 업무 매뉴얼" → weekly-report 방지).
pub const SKILL_AUTO_MIN_DISTINCTIVE: usize = 2;
/// 앞 턴에 자동으로 불러온 Skill을 다음 요청에도 이어서 넣는 최대 턴 수(다른 Skill을 고르거나 설정을 끄면 멈춤)
pub const SKILL_STICKY_TURNS: usize = 3;
/// 이어 쓰기를 따져 보는 요청 길이: 의미 있는 낱말(띄어쓰기 단위)이 이만큼 이상인데 Skill과 겹치는 토큰이 하나도 없으면 다른 일로 보고 이어 넣지 않는다.
/// "계속"·"날짜만 바꿔 줘"·"3번 섹션 다시 써 줘" 같은 짧은 후속 요청은 늘 이어 넣는다.
pub const SKILL_STICKY_UNRELATED_MIN_WORDS: usize = 3;
/// 자동으로 넣는 Skill 지침의 최대 글자 수와 컨텍스트(글자) 대비 비율
const SKILL_AUTO_BUDGET_CAP: usize = 10_000;
const SKILL_AUTO_BUDGET_RATIO: f64 = 0.20;
/// 머리 줄 `[Skill: <이름> — ...]`의 이름 뒤 부분
pub const AUTO_HEADER_TAIL: &str = "앱이 이 요청에 맞는 Skill로 판단해 미리 불러왔습니다. 아래 지침을 그대로 따르세요. 맞지 않으면 무시하세요.";
/// 이어 쓰기(Sticky)로 넣을 때 머리 줄의 이름 뒤 부분: 이번 요청과 맞는지 모델이 판단하게 한다.
pub const STICKY_HEADER_TAIL: &str = "앞 요청에서 쓰던 Skill입니다. 이번 요청이 그 작업을 이어 가는 것일 때만 따르고, 다른 일이면 무시하세요.";
/// 자동으로 불러온 Skill을 모델이 같은 턴에 다시 load_skill로 부를 때(첫 번째만) 돌려주는 답.
/// 저장되어 다음 턴에도 보이므로 "위에 있다"고 하지 않는다.
pub const ALREADY_LOADED: &str = "앱이 이 턴에 미리 넣었던 Skill입니다. 지침이 필요하면 load_skill로 다시 읽으세요.";

/// 어느 Skill에나 흔한 요청 낱말·조사·어미 묶음(한글 2글자)과 영어 일반 낱말. 맞은 토큰으로 치지 않는다.
const STOPWORDS: &[&str] = &[
    // 한글: 일반 동사·명사
    "사용", "용자", "자가", "자의", "자를", "요청", "청할", "청하", "청을", "작성", "성한", "성해", "성을", "정리", "리한", "리해", "리를",
    "만들", "만든", "든다", "들어", "드는", "이번", "번주", "으로", "한다", "쓴다", "하는", "해주", "주세", "세요", "해서", "하고", "에서",
    "에게", "니다", "합니", "습니", "있는", "없는", "경우", "같은", "대한", "관련", "위한", "위해", "하면", "하기", "할때", "때는", "다음",
    "그리", "리고", "또는", "이나", "거나", "하거", "주는", "주고", "알려", "려줘", "보여", "여줘", "해줘", "도와", "와줘", "필요", "요한",
    // 영어
    "the", "to", "a", "an", "of", "for", "and", "or", "use", "uses", "using", "used", "this", "that", "these", "those", "when",
    "whenever", "user", "users", "skill", "skills", "wants", "want", "do", "does", "anything", "something", "with", "how", "i",
    "is", "are", "be", "it", "its", "in", "on", "at", "by", "from", "as", "can", "you", "your", "me", "my", "please", "what",
    "which", "any", "all", "into", "about", "include", "includes", "including", "like", "such", "should", "will", "would",
    "also", "if", "then", "than", "so", "not", "no", "get", "make", "create", "help", "need", "needs", "read", "write", "edit",
];

/// 맞은 토큰으로는 세지만 "이 Skill다운" 토큰으로는 치지 않는 흔한 낱말(여러 일에 두루 쓰인다).
const GENERIC_TOKENS: &[&str] = &[
    "회사", "업무", "파일", "내용", "자료", "문서", "정보", "데이터", "목록", "코드", "작업",
    "text", "texts", "file", "files", "data", "table", "tables", "add", "document", "documents", "doc", "docs", "code",
    "list", "lists", "content", "contents", "new", "work", "page", "pages", "format", "information", "info", "function",
    "functions", "project", "folder", "folders",
];

fn is_generic(token: &str) -> bool {
    GENERIC_TOKENS.contains(&token)
}

/// Skill 이름을 토큰으로(`weekly-report` → weekly, report)
fn name_tokens(name: &str) -> BTreeSet<String> {
    meaningful_tokens(&name.replace(['-', '_'], " "))
}

/// Skill을 고른 이유
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectReason {
    /// 요청에 `/<이름>`이나 이름이 그대로 들어 있다.
    Explicit,
    /// 요청 내용이 Skill 이름·설명과 맞는다.
    Implicit,
    /// 이번 요청으로는 고르지 못했지만 바로 앞 답변에서 자동으로 불러온 Skill을 이어 쓴다("계속" 등).
    Sticky,
}

impl SelectReason {
    fn as_str(self) -> &'static str {
        match self {
            SelectReason::Explicit => "explicit",
            SelectReason::Implicit => "implicit",
            SelectReason::Sticky => "sticky",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "explicit" => SelectReason::Explicit,
            "sticky" => SelectReason::Sticky,
            _ => SelectReason::Implicit,
        }
    }
}

/// 이번 턴에 앱이 미리 불러온 Skill
#[derive(Debug, Clone, PartialEq)]
pub struct AutoSkill {
    pub name: String,
    pub reason: SelectReason,
    /// 모델에 넣을 글(머리 줄 + 지침). 예산을 넘으면 잘려 있다.
    pub text: String,
    /// 예산 때문에 지침을 잘랐는지 (그러면 load_skill로 전체를 다시 읽을 수 있다)
    pub truncated: bool,
}

fn usable(s: &SkillInfo) -> bool {
    s.enabled && s.error.is_none() && !s.shadowed
}

/// 이름의 앞뒤 경계로 보지 않는 글자. 영문 이름은 영문·숫자·`-`·`_`(그래서 "weekly-report로"처럼 한글 조사가 붙어도 맞는다),
/// 한글 등이 든 이름은 모든 글자(그래서 "요약"이 "요약하지" 안에서는 맞지 않는다).
fn is_name_char(c: char, ascii_name: bool) -> bool {
    c == '-' || c == '_' || if ascii_name { c.is_ascii_alphanumeric() } else { c.is_alphanumeric() }
}

/// `name`이 `text` 안에 낱말 하나로(앞뒤가 이름 글자가 아님) 들어 있는지. 둘 다 소문자로 준다.
fn contains_whole_name(text: &str, name: &str) -> bool {
    let core = name.trim_start_matches('/');
    if core.is_empty() {
        return false;
    }
    let ascii = core.is_ascii();
    text.match_indices(name).any(|(i, m)| {
        let before = text[..i].chars().next_back();
        let after = text[i + m.len()..].chars().next();
        !before.is_some_and(|c| is_name_char(c, ascii)) && !after.is_some_and(|c| is_name_char(c, ascii))
    })
}

/// 이름만(슬래시 없이) 써도 부른 것으로 볼 이름인지: 영문 이름이고 `-`가 있거나 4글자 이상. 한글 이름은 `/이름`으로만 부른다.
fn bare_name_ok(name: &str) -> bool {
    name.is_ascii() && (name.contains('-') || name.chars().count() >= 4)
}

/// 고르기에 쓰는 토큰: `knowledge::tokenize`에서 한 글자 토큰과 흔한 낱말(STOPWORDS)을 뺀 것
fn meaningful_tokens(text: &str) -> BTreeSet<String> {
    crate::knowledge::tokenize(text)
        .into_iter()
        .filter(|t| t.chars().count() >= 2 && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// 사용자 메시지에 맞는 Skill을 하나 고른다(없으면 None). 켜져 있고 문제없고 가려지지 않은 Skill만 본다.
/// - 이름을 부르면(`/weekly-report`, 또는 `bare_name_ok`인 이름 그대로) 그것(여럿이면 긴 이름).
/// - 아니면 의미 있는 요청 토큰이 이름·설명에 얼마나 들어 있는지와, 설치된 Skill 사이의 IDF로 고른다.
pub fn select_for_message(skills: &[SkillInfo], message: &str) -> Option<(SkillInfo, SelectReason)> {
    let candidates: Vec<&SkillInfo> = skills.iter().filter(|s| usable(s)).collect();
    let lower = message.to_lowercase();
    if let Some(s) = candidates
        .iter()
        .filter(|s| {
            let name = s.name.trim().to_lowercase();
            contains_whole_name(&lower, &format!("/{}", name)) || (bare_name_ok(&name) && contains_whole_name(&lower, &name))
        })
        .max_by_key(|s| s.name.chars().count())
    {
        return Some(((*s).clone(), SelectReason::Explicit));
    }

    let query = meaningful_tokens(message);
    if query.is_empty() || candidates.is_empty() {
        return None;
    }
    let owns: Vec<BTreeSet<String>> = candidates.iter().map(|s| meaningful_tokens(&format!("{} {}", s.name, s.description))).collect();
    let docs = candidates.len() as f64 + SKILL_AUTO_GENERIC_DOCS;
    let idf = |t: &String| {
        let df = owns.iter().filter(|o| o.contains(t)).count().max(1) as f64;
        (1.0 + docs / df).ln()
    };
    // (IDF 합, 맞은 토큰, Skill)
    let mut scored: Vec<(f64, Vec<&str>, &SkillInfo)> = candidates
        .iter()
        .zip(owns.iter())
        .map(|(s, own)| {
            let hits: Vec<&str> = query.iter().filter(|t| own.contains(*t)).map(String::as_str).collect();
            (hits.iter().map(|t| idf(&t.to_string())).sum(), hits, *s)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let runner_up = scored.get(1).map_or(0.0, |r| r.0);
    let (weight, hits, skill) = scored.into_iter().next()?;
    let coverage = hits.len() as f64 / query.len() as f64;
    // 흔한 낱말만 맞으면 고르지 않는다: 이름 토큰이 하나 맞거나, 흔하지 않은 토큰이 둘 이상 맞아야 한다.
    let own_name = name_tokens(&skill.name);
    let distinctive: Vec<&str> = hits.iter().copied().filter(|t| !is_generic(t)).collect();
    let specific = distinctive.iter().any(|t| own_name.contains(*t)) || distinctive.len() >= SKILL_AUTO_MIN_DISTINCTIVE;
    (hits.len() >= SKILL_AUTO_MIN_HITS
        && coverage >= SKILL_AUTO_MIN_COVERAGE
        && weight >= SKILL_AUTO_MIN_WEIGHT
        && weight - runner_up >= SKILL_AUTO_MIN_MARGIN
        && specific)
        .then(|| (skill.clone(), SelectReason::Implicit))
}

/// 자동으로 넣는 Skill 지침의 글자 예산 = min(10,000, 컨텍스트 × 1.5 × 0.20)
pub fn auto_skill_budget(context_length: u32) -> usize {
    let ctx_chars = context_length as f64 * crate::context_budget::CHARS_PER_TOKEN;
    ((ctx_chars * SKILL_AUTO_BUDGET_RATIO) as usize).min(SKILL_AUTO_BUDGET_CAP)
}

/// 고른 Skill의 지침(`load_for_model`과 같은 글)에 머리 줄을 붙이고 예산에 맞춘다.
pub fn build_auto_skill(skill: &SkillInfo, reason: SelectReason, context_length: u32, can_run_python: bool) -> Result<AutoSkill, String> {
    let tail = if reason == SelectReason::Sticky { STICKY_HEADER_TAIL } else { AUTO_HEADER_TAIL };
    let header = format!("[Skill: {} — {}]\n\n", skill.name, tail);
    let body = load_for_model(skill, can_run_python)?;
    let budget = auto_skill_budget(context_length);
    let full = format!("{}{}", header, body);
    if full.chars().count() <= budget {
        return Ok(AutoSkill { name: skill.name.clone(), reason, text: full, truncated: false });
    }
    let note = format!(
        "\n\n…(지침이 길어 여기까지만 넣었습니다. 나머지는 read_skill_file(skill=\"{0}\", path=\"SKILL.md\") 또는 load_skill(name=\"{0}\")로 읽으세요.)",
        skill.name
    );
    let keep = budget.saturating_sub(note.chars().count());
    let text: String = full.chars().take(keep).collect::<String>() + &note;
    Ok(AutoSkill { name: skill.name.clone(), reason, text, truncated: true })
}

/// 답변의 자동 불러오기 기록에서 (Skill 이름, 이유)
pub fn auto_record_skill(r: &crate::store::ToolExecutionRecord) -> Option<(String, SelectReason)> {
    if !r.automatic || r.tool_name != "load_skill" {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&r.arguments_json).ok()?;
    let name = v.get("name")?.as_str()?.to_string();
    Some((name, SelectReason::parse(v.get("reason").and_then(|x| x.as_str()).unwrap_or(""))))
}

fn message_auto_skill(m: &crate::store::ChatMessage) -> Option<(String, SelectReason)> {
    m.tool_records.iter().find_map(auto_record_skill)
}

/// 이어 쓸 Skill: 바로 앞 답변이 자동으로 불러온 Skill이 있고, 그 Skill을 이어 쓴(Sticky) 턴이 `SKILL_STICKY_TURNS`보다 적으면 그 이름.
/// `history`는 이번 사용자 메시지를 넣기 전이든 뒤든 된다(끝의 사용자 메시지는 건너뛴다).
pub fn sticky_from_history(history: &[crate::store::ChatMessage]) -> Option<String> {
    let mut answers = history.iter().rev().filter(|m| m.role == "assistant");
    let (name, reason) = message_auto_skill(answers.next()?)?;
    if reason != SelectReason::Sticky {
        // 바로 앞에서 새로 고른 Skill: 그 앞의 이어 쓰기 기록은 세지 않는다.
        return Some(name);
    }
    let mut streak = 1;
    for m in answers {
        match message_auto_skill(m) {
            Some((n, SelectReason::Sticky)) if n.eq_ignore_ascii_case(&name) => streak += 1,
            _ => break,
        }
    }
    (streak < SKILL_STICKY_TURNS).then_some(name)
}

/// 앞 Skill을 이번 요청에 이어 넣어도 되는지. 짧은 후속 요청(의미 있는 낱말 `SKILL_STICKY_UNRELATED_MIN_WORDS`개 미만)은 늘 그렇다.
/// 그보다 길면 요청 토큰 중 흔하지 않은 것이 Skill 이름·설명·SKILL.md 본문에 하나라도 있어야 한다("파이썬으로 CSV 합치는 코드 짜 줘"는 주간 보고 Skill을 이어 넣지 않는다).
pub fn sticky_fits(skill: &SkillInfo, message: &str) -> bool {
    let words = message.split_whitespace().filter(|w| !meaningful_tokens(w).is_empty()).count();
    if words < SKILL_STICKY_UNRELATED_MIN_WORDS {
        return true;
    }
    let body = read_skill_md(&skill.dir).map(|t| parse_skill_md(&t).body).unwrap_or_default();
    let own = meaningful_tokens(&format!("{} {} {}", skill.name.replace(['-', '_'], " "), skill.description, body));
    meaningful_tokens(message).iter().any(|t| !is_generic(t) && own.contains(t))
}

/// 턴 시작 때 부른다: 켜져 있으면 쓸 수 있는 Skill 중 이번 메시지에 맞는 것을 골라 지침을 만든다.
/// 맞는 것이 없으면 바로 앞 답변에서 자동으로 불러온 Skill을 이어 쓴다(`sticky_from_history`).
/// `message`는 첨부 파일 내용을 뺀 사용자 글, `history`는 대화 기록. 프로젝트가 있으면 Skill 스크립트 실행 안내도 넣는다.
pub fn auto_for_turn(
    project: Option<(&str, &Path)>,
    message: &str,
    history: &[crate::store::ChatMessage],
    context_length: u32,
    enabled: bool,
) -> Option<AutoSkill> {
    if !enabled || message.trim().is_empty() {
        return None;
    }
    let skills = available(project);
    let (skill, reason) = select_for_message(&skills, message).or_else(|| {
        let name = sticky_from_history(history)?;
        let s = skills.iter().find(|s| s.name.eq_ignore_ascii_case(&name))?.clone();
        sticky_fits(&s, message).then_some((s, SelectReason::Sticky))
    })?;
    build_auto_skill(&skill, reason, context_length, project.is_some()).ok()
}

/// 승인 뒤 이어가기: 멈췄던 답변의 자동 불러오기 기록에 적힌 Skill을 그대로 다시 넣는다(다시 고르지 않는다).
pub fn auto_for_resume(project: Option<(&str, &Path)>, paused: &crate::store::ChatMessage, context_length: u32) -> Option<AutoSkill> {
    let (name, reason) = message_auto_skill(paused)?;
    let skill = find_available(project, &name)?;
    build_auto_skill(&skill, reason, context_length, project.is_some()).ok()
}

/// 자동으로 불러온 Skill을 모델이 다시 load_skill로 부르면 짧은 답을 준다(지침을 잘라 넣었으면 None: 전체를 읽게 둔다).
/// 한 턴에 한 번만 쓴다(그 뒤에 또 부르면 전문을 준다) — 부르는 쪽이 지킨다.
pub fn already_loaded_reply(auto: Option<&AutoSkill>, tool: &str, args: &serde_json::Value) -> Option<String> {
    let auto = auto.filter(|a| !a.truncated)?;
    let want = args.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
    (tool == "load_skill" && want.eq_ignore_ascii_case(&auto.name)).then(|| ALREADY_LOADED.to_string())
}

impl AutoSkill {
    /// 화면에 "Skill 자동 불러옴" 카드로 보일 기록(`automatic`이라 다음 요청의 도구 기록으로는 보내지 않는다).
    pub fn tool_record(&self) -> crate::store::ToolExecutionRecord {
        let why = match self.reason {
            SelectReason::Explicit => "요청에 Skill 이름이 있어",
            SelectReason::Implicit => "요청 내용이 Skill 설명과 맞아",
            SelectReason::Sticky => "앞 요청에서 쓰던 Skill이라 이어서",
        };
        let cut = if self.truncated { ", 길어서 앞부분만" } else { "" };
        crate::store::ToolExecutionRecord {
            id: format!("auto-skill-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            tool_name: "load_skill".to_string(),
            arguments_json: serde_json::json!({ "name": self.name, "reason": self.reason.as_str() }).to_string(),
            status: "completed".to_string(),
            output: format!("Skill 자동 불러옴: {} ({} 앱이 미리 넣음{})", self.name, why, cut),
            automatic: true,
            ..Default::default()
        }
    }

    /// 이 Skill을 자동으로 불러온 기록인지 (승인 뒤 이어갈 때 같은 Skill을 다시 넣을지 판단)
    pub fn matches_record(&self, r: &crate::store::ToolExecutionRecord) -> bool {
        auto_record_skill(r).is_some_and(|(n, _)| n.eq_ignore_ascii_case(&self.name))
    }
}

/// Skill 폴더 안의 파일을 읽는다 (Skill 폴더 밖으로는 나갈 수 없다).
pub fn read_file_for_model(skill: &SkillInfo, rel: &str) -> Result<String, String> {
    let path = resolve_in_skill(skill, rel)?;
    if !path.is_file() {
        return Err(format!("'{}' Skill에 '{}' 파일이 없습니다. 파일 목록: {}", skill.name, rel, skill.files.join(", ")));
    }
    let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if len > MAX_READ_BYTES {
        return Err(format!("파일이 너무 큽니다 ({}KB 넘음).", MAX_READ_BYTES / 1024));
    }
    let bytes = fs::read(&path).map_err(|e| format!("파일을 읽지 못했습니다: {}", e))?;
    String::from_utf8(bytes).map_err(|_| format!("'{}'은(는) 텍스트 파일이 아니라 읽을 수 없습니다.", rel))
}

/// Skill 폴더 기준 상대 경로를 실제 경로로 바꾼다. 폴더 밖·링크는 막는다.
pub fn resolve_in_skill(skill: &SkillInfo, rel: &str) -> Result<PathBuf, String> {
    let clean = rel.trim().replace('\\', "/");
    let clean = clean.trim_start_matches("./");
    let p = Path::new(clean);
    if clean.is_empty()
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("Skill 폴더 밖의 경로('{}')는 쓸 수 없습니다.", rel));
    }
    let mut cur = skill.dir.clone();
    for c in p.components() {
        cur.push(c);
        if cur.exists() && is_link(&cur) {
            return Err(format!("링크 경로('{}')는 쓸 수 없습니다.", rel));
        }
    }
    Ok(cur)
}

/// 모델이 부르는 Skill 도구 실행. (도구 이름이 Skill 도구가 아니면 None)
pub fn run_tool(name: &str, args: &serde_json::Value, project: Option<(&str, &Path)>) -> Option<Result<String, String>> {
    let arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match name {
        "load_skill" => {
            let skill_name = arg("name");
            Some(match find_available(project, &skill_name) {
                Some(s) => load_for_model(&s, project.is_some()),
                None => Err(unknown_skill(project, &skill_name)),
            })
        }
        "read_skill_file" => {
            let skill_name = arg("skill");
            Some(match find_available(project, &skill_name) {
                Some(s) => read_file_for_model(&s, &arg("path")),
                None => Err(unknown_skill(project, &skill_name)),
            })
        }
        _ => None,
    }
}

pub fn tools_schema() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "load_skill",
                "description": "설치된 Skill(작업 지침)의 본문을 읽습니다. 사용자의 요청이 시스템 안내의 Skill 목록 중 하나에 해당하면, 작업하기 전에 먼저 호출하세요.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Skill 이름 (목록에 적힌 그대로)" }
                    },
                    "required": ["name"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_skill_file",
                "description": "Skill 폴더에 함께 든 참고 파일(서식 예시, 참고 문서, 스크립트 등)을 읽습니다. load_skill 결과에 나온 파일만 읽을 수 있습니다.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string", "description": "Skill 이름" },
                        "path": { "type": "string", "description": "Skill 폴더 기준 상대 경로 (예: 'reference/style.md')" }
                    },
                    "required": ["skill", "path"]
                }
            }
        }),
    ]
}

// ---------------------------------------------------------------------------
// 켜기·끄기
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct SkillState {
    #[serde(default)]
    disabled: HashSet<String>,
}

fn state_path() -> PathBuf {
    user_dir().join(STATE_FILE)
}

fn load_state() -> SkillState {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let _guard = STATE_LOCK.lock().map_err(|e| e.to_string())?;
    let mut state = load_state();
    if enabled {
        state.disabled.remove(id);
    } else {
        state.disabled.insert(id.to_string());
    }
    let sorted: BTreeSet<&String> = state.disabled.iter().collect();
    let json = serde_json::to_string_pretty(&serde_json::json!({ "disabled": sorted })).map_err(|e| e.to_string())?;
    let _ = fs::create_dir_all(user_dir());
    fs::write(state_path(), json).map_err(|e| format!("Skill 설정을 저장하지 못했습니다: {}", e))
}

// ---------------------------------------------------------------------------
// 설치·삭제·새로 만들기 (모든 대화용 Skill 폴더)
// ---------------------------------------------------------------------------

/// 같은 폴더가 이미 있을 때 돌려주는 오류의 앞부분 (화면에서 덮어쓸지 묻는다)
pub const ALREADY_EXISTS: &str = "ALREADY_EXISTS:";

/// Skill 이름을 Windows 폴더 이름으로 쓸 수 있게 다듬는다.
pub fn folder_name_for(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '-' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().chars().take(MAX_NAME_CHARS).collect::<String>();
    if cleaned.is_empty() || cleaned.starts_with('.') || cleaned.starts_with('_') {
        return format!("skill-{}", cleaned.trim_start_matches(['.', '_']));
    }
    if RESERVED.contains(&cleaned.to_ascii_uppercase().as_str()) {
        return format!("{}-skill", cleaned);
    }
    cleaned
}

fn info_for_user_folder(folder: &str) -> Result<SkillInfo, String> {
    let disabled = load_state().disabled;
    inspect(&user_dir().join(folder), format!("user:{}", folder), "user", folder.to_string(), &disabled)
        .ok_or_else(|| "설치한 폴더에서 SKILL.md를 찾지 못했습니다.".to_string())
}

/// 임시 폴더에 풀어 둔 Skill을 사용자 Skill 폴더로 옮긴다.
fn finish_install(staged: &Path, fallback_name: &str, overwrite: bool) -> Result<SkillInfo, String> {
    let text = read_skill_md(staged)?;
    let parsed = parse_skill_md(&text);
    let name = parsed.name.unwrap_or_else(|| fallback_name.to_string());
    valid_name(&name)?;
    if parsed.description.is_none() {
        return Err("SKILL.md 머리말에 description(언제 쓰는 Skill인지)이 없습니다. Skill 형식이 맞는지 확인하세요.".to_string());
    }
    let folder = folder_name_for(&name);
    let target = user_dir().join(&folder);
    if target.exists() {
        if !overwrite {
            return Err(format!("{}{}", ALREADY_EXISTS, name));
        }
        trash::delete(&target).map_err(|e| format!("기존 Skill을 휴지통으로 옮기지 못했습니다: {}", e))?;
    }
    fs::rename(staged, &target).map_err(|e| format!("Skill을 설치하지 못했습니다: {}", e))?;
    info_for_user_folder(&folder)
}

fn staging_dir() -> Result<PathBuf, String> {
    let dir = user_dir().join(format!(".installing-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&dir).map_err(|e| format!("임시 폴더를 만들지 못했습니다: {}", e))?;
    Ok(dir)
}

/// 실패하면 임시 폴더를 지운다.
fn with_staging(f: impl FnOnce(&Path) -> Result<SkillInfo, String>) -> Result<SkillInfo, String> {
    let staged = staging_dir()?;
    let res = f(&staged);
    if staged.exists() {
        let _ = fs::remove_dir_all(&staged);
    }
    res
}

/// 폴더째 복사해 설치한다. 폴더 안(또는 바로 아래 한 단계)에 SKILL.md가 있어야 한다.
pub fn install_from_folder(src: &Path, overwrite: bool) -> Result<SkillInfo, String> {
    if !src.is_dir() || is_link(src) {
        return Err("Skill 폴더를 찾을 수 없습니다.".to_string());
    }
    let src = fs::canonicalize(src).map_err(|e| e.to_string())?;
    let users = fs::canonicalize(user_dir()).unwrap_or_else(|_| user_dir());
    if src.starts_with(&users) {
        return Err("이미 Skill 폴더 안에 있는 폴더입니다.".to_string());
    }
    let root = if skill_file_in(&src).is_some() {
        src.clone()
    } else {
        // 폴더를 한 겹 더 감싼 경우 (예: 내려받은 폴더 안에 Skill 폴더 하나)
        let inner: Vec<PathBuf> = fs::read_dir(&src)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !is_link(p) && skill_file_in(p).is_some())
            .collect();
        match inner.as_slice() {
            [one] => one.clone(),
            [] => return Err("고른 폴더에 SKILL.md가 없습니다. SKILL.md가 든 Skill 폴더를 고르세요.".to_string()),
            _ => return Err("고른 폴더에 Skill이 여러 개 있습니다. Skill 폴더 하나를 고르세요.".to_string()),
        }
    };
    let fallback = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    with_staging(|staged| {
        let mut budget = (0u64, 0usize);
        copy_tree(&root, staged, &mut budget)?;
        finish_install(staged, &fallback, overwrite)
    })
}

fn copy_tree(from: &Path, to: &Path, budget: &mut (u64, usize)) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        let name = e.file_name();
        if is_link(&p) || SKIP_DIRS.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        if p.is_dir() {
            copy_tree(&p, &to.join(&name), budget)?;
        } else if p.is_file() {
            let len = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            budget.0 += len;
            budget.1 += 1;
            check_budget(budget)?;
            fs::copy(&p, to.join(&name)).map_err(|e| format!("파일 복사 실패 ({}): {}", name.to_string_lossy(), e))?;
        }
    }
    Ok(())
}

fn check_budget(budget: &(u64, usize)) -> Result<(), String> {
    if budget.0 > MAX_INSTALL_BYTES {
        return Err(format!("Skill이 너무 큽니다 ({}MB 넘음).", MAX_INSTALL_BYTES / 1024 / 1024));
    }
    if budget.1 > MAX_INSTALL_FILES {
        return Err(format!("Skill에 파일이 너무 많습니다 ({}개 넘음).", MAX_INSTALL_FILES));
    }
    Ok(())
}

/// .zip / .skill 파일을 풀어 설치한다. SKILL.md가 가장 얕은 곳에 있는 폴더를 Skill로 본다.
pub fn install_from_zip(zip_path: &Path, overwrite: bool) -> Result<SkillInfo, String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("파일을 열 수 없습니다: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "올바른 zip 파일이 아닙니다.".to_string())?;

    // SKILL.md 위치 찾기
    let mut best: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(i) else { continue };
        let Some(path) = entry.enclosed_name() else { continue };
        if entry.is_dir() || path.components().any(|c| c.as_os_str() == "__MACOSX") {
            continue;
        }
        if path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(SKILL_FILE))
            && best.as_ref().map_or(true, |b| path.components().count() < b.components().count())
        {
            best = Some(path);
        }
    }
    let skill_md = best.ok_or_else(|| "zip 파일 안에 SKILL.md가 없습니다.".to_string())?;
    let prefix = skill_md.parent().map(Path::to_path_buf).unwrap_or_default();
    let fallback = prefix
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .or_else(|| zip_path.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "skill".to_string());

    with_staging(|staged| {
        let mut budget = (0u64, 0usize);
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| format!("zip 읽기 오류: {}", e))?;
            let Some(path) = entry.enclosed_name() else { continue };
            let Ok(rel) = path.strip_prefix(&prefix) else { continue };
            if rel.as_os_str().is_empty()
                || rel.components().any(|c| SKIP_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
            {
                continue;
            }
            let out = staged.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                continue;
            }
            if entry.is_symlink() {
                continue;
            }
            budget.0 += entry.size();
            budget.1 += 1;
            check_budget(&budget)?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            // 선언된 크기보다 많이 풀리지 않게 읽는 양을 제한한다.
            let mut buf = Vec::new();
            (&mut entry)
                .take(MAX_INSTALL_BYTES + 1)
                .read_to_end(&mut buf)
                .map_err(|e| format!("zip 풀기 오류: {}", e))?;
            if buf.len() as u64 > entry.size() {
                return Err("zip 파일이 손상되었습니다.".to_string());
            }
            fs::write(&out, buf).map_err(|e| e.to_string())?;
        }
        finish_install(staged, &fallback, overwrite)
    })
}

/// 사용자 Skill을 휴지통으로 옮긴다. 프로젝트 Skill은 프로젝트 파일이라 여기서 지우지 않는다.
pub fn delete_user_skill(id: &str) -> Result<(), String> {
    let folder = id
        .strip_prefix("user:")
        .ok_or_else(|| "프로젝트 Skill은 파일 탐색기에서 폴더를 지우세요.".to_string())?;
    if folder.is_empty() || folder.contains(['/', '\\']) || folder.starts_with('.') || folder == STATE_FILE {
        return Err("잘못된 Skill입니다.".to_string());
    }
    let dir = user_dir().join(folder);
    if !dir.is_dir() || is_link(&dir) {
        return Err("Skill 폴더를 찾을 수 없습니다.".to_string());
    }
    trash::delete(&dir).map_err(|e| format!("휴지통으로 옮기지 못했습니다: {}", e))?;
    let _ = set_enabled(id, true);
    Ok(())
}

/// 새 사용자 Skill의 틀을 만든다.
pub fn create_user_skill(name: &str) -> Result<SkillInfo, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Skill 이름을 입력하세요.".to_string());
    }
    valid_name(name)?;
    let folder = folder_name_for(name);
    let dir = user_dir().join(&folder);
    if dir.exists() {
        return Err(format!("'{}' 폴더가 이미 있습니다.", folder));
    }
    fs::create_dir_all(&dir).map_err(|e| format!("폴더를 만들지 못했습니다: {}", e))?;
    let yaml_name = if name.contains([':', '#', '\'', '"']) || name.starts_with(['-', '[', '{', '&', '*', '!', '|', '>', '%', '@']) {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        name.to_string()
    };
    let template = format!(
        "---\nname: {yaml_name}\ndescription: 이 Skill이 무엇을 하는지, 어떤 요청일 때 써야 하는지 한두 문장으로 적으세요. 모델은 이 설명을 보고 Skill을 쓸지 정합니다.\n---\n\n# {name}\n\n## 언제 쓰나요\n- (예: 주간 업무 보고서를 써 달라고 할 때)\n\n## 지침\n1. (모델이 따라야 할 순서와 규칙을 적으세요)\n2. \n\n## 결과물 형식\n- (예: 제목, 요약 3줄, 세부 항목 표)\n"
    );
    fs::write(dir.join(SKILL_FILE), template).map_err(|e| format!("SKILL.md를 만들지 못했습니다: {}", e))?;
    info_for_user_folder(&folder)
}

/// 화면에서 고른 Skill의 실제 폴더 (ID로 찾는다)
pub fn dir_of(id: &str, project: Option<(&str, &Path)>) -> Option<PathBuf> {
    discover(project).into_iter().find(|s| s.id == id).map(|s| s.dir)
}

/// SKILL.md 원문 (미리보기용)
pub fn skill_md_text(id: &str, project: Option<(&str, &Path)>) -> Result<String, String> {
    let dir = dir_of(id, project).ok_or_else(|| "Skill을 찾을 수 없습니다.".to_string())?;
    read_skill_md(&dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_front_matter_variants() {
        let p = parse_skill_md("\u{feff}---\nname: weekly-report\ndescription: \"주간 보고서: 형식\"\nlicense: MIT\n---\n\n# 본문\n지침");
        assert_eq!(p.name.as_deref(), Some("weekly-report"));
        assert_eq!(p.description.as_deref(), Some("주간 보고서: 형식"));
        assert_eq!(p.body, "# 본문\n지침");

        let p = parse_skill_md("---\nname: x\ndescription: >\n  첫 줄\n  둘째 줄\nmetadata:\n  version: 1\n---\nbody");
        assert_eq!(p.description.as_deref(), Some("첫 줄 둘째 줄"));
        let p = parse_skill_md("---\r\nname: 'a''b'\r\ndescription: |\r\n  하나\r\n  둘\r\n---\r\nbody");
        assert_eq!(p.name.as_deref(), Some("a'b"));
        assert_eq!(p.description.as_deref(), Some("하나\n둘"));

        let p = parse_skill_md("# 머리말 없음\n내용");
        assert_eq!(p.name, None);
        assert_eq!(p.body, "# 머리말 없음\n내용");
    }

    #[test]
    fn folder_names_are_safe_on_windows() {
        assert_eq!(folder_name_for("보고서 작성"), "보고서 작성");
        assert_eq!(folder_name_for("a/b:c"), "a-b-c");
        assert_eq!(folder_name_for("CON"), "CON-skill");
        assert_eq!(folder_name_for(".hidden"), "hidden");
        assert_eq!(folder_name_for("_state.json"), "skill-state.json");
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    // 사용자 Skill 폴더는 프로세스 전체에서 하나라, 그 폴더를 쓰는 검사는 한 테스트에 모은다.
    #[test]
    fn discover_install_and_tools() {
        let home = tempfile::tempdir().unwrap();
        set_user_dir(home.path().join("skills"));
        let users = user_dir();
        let proj = tempfile::tempdir().unwrap();
        let root = proj.path();

        write(&users.join("report/SKILL.md"), "---\nname: report\ndescription: 보고서 형식\n---\n보고서 지침");
        write(&users.join("report/reference/style.md"), "스타일 안내");
        write(&users.join("broken/SKILL.md"), "---\nname: broken\n---\n설명 없음");
        write(&users.join("not-a-skill/readme.txt"), "x");
        write(&root.join(".skills/report/SKILL.md"), "---\nname: report\ndescription: 프로젝트용 보고서\n---\n프로젝트 지침");
        write(&root.join(".skills/report/scripts/make.py"), "print(1)");

        let all = discover(Some(("p1", root)));
        assert_eq!(all.len(), 3);
        let broken = all.iter().find(|s| s.name == "broken").unwrap();
        assert!(broken.error.is_some());
        let user_report = all.iter().find(|s| s.id == "user:report").unwrap();
        assert!(user_report.shadowed, "프로젝트 Skill이 같은 이름의 사용자 Skill을 가린다");
        assert_eq!(user_report.files, vec!["reference/style.md"]);

        let avail = available(Some(("p1", root)));
        assert_eq!(avail.len(), 1);
        assert_eq!(avail[0].id, "project:p1:.skills/report");
        let section = prompt_section(&avail).unwrap();
        assert!(section.contains("- report: 프로젝트용 보고서"));

        // 프로젝트가 없으면 사용자 Skill만
        let chat = available(None);
        assert_eq!(chat.len(), 1);
        assert_eq!(chat[0].id, "user:report");

        // 도구
        let out = run_tool("load_skill", &serde_json::json!({"name": "REPORT"}), Some(("p1", root))).unwrap().unwrap();
        assert!(out.contains("프로젝트 지침"));
        assert!(out.contains("run_python(skill=\"report\""));
        let out = run_tool("load_skill", &serde_json::json!({"name": "report"}), None).unwrap().unwrap();
        assert!(out.contains("보고서 지침") && !out.contains("run_python"));
        let out = run_tool("read_skill_file", &serde_json::json!({"skill": "report", "path": "reference/style.md"}), None).unwrap();
        assert_eq!(out.unwrap(), "스타일 안내");
        let out = run_tool("read_skill_file", &serde_json::json!({"skill": "report", "path": "../broken/SKILL.md"}), None).unwrap();
        assert!(out.is_err());
        let out = run_tool("load_skill", &serde_json::json!({"name": "없음"}), None).unwrap();
        assert!(out.unwrap_err().contains("쓸 수 있는 Skill: report"));
        assert!(run_tool("read_file", &serde_json::json!({}), None).is_none());

        // 끄기
        set_enabled("project:p1:.skills/report", false).unwrap();
        let avail = available(Some(("p1", root)));
        assert_eq!(avail.len(), 1);
        assert_eq!(avail[0].id, "user:report", "프로젝트 Skill을 끄면 사용자 Skill이 쓰인다");
        set_enabled("project:p1:.skills/report", true).unwrap();

        // 폴더에서 설치 (한 겹 감싼 폴더도)
        let src = tempfile::tempdir().unwrap();
        write(&src.path().join("wrap/메일 작성/SKILL.md"), "---\nname: 메일 작성\ndescription: 메일\n---\n본문");
        write(&src.path().join("wrap/메일 작성/.git/config"), "x");
        let info = install_from_folder(&src.path().join("wrap"), false).unwrap();
        assert_eq!(info.id, "user:메일 작성");
        assert!(!users.join("메일 작성/.git").exists());
        let again = install_from_folder(&src.path().join("wrap"), false).unwrap_err();
        assert_eq!(again, format!("{}메일 작성", ALREADY_EXISTS));

        // zip에서 설치 (폴더 밖으로 나가는 항목은 무시)
        let zip_path = src.path().join("pack.skill");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opt = zip::write::SimpleFileOptions::default();
            z.start_file("pdf-tool/SKILL.md", opt).unwrap();
            z.write_all("---\nname: pdf-tool\ndescription: PDF 다루기\n---\nPDF 지침".as_bytes()).unwrap();
            z.start_file("pdf-tool/scripts/run.py", opt).unwrap();
            z.write_all(b"print('hi')").unwrap();
            z.start_file("../evil.txt", opt).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        let info = install_from_zip(&zip_path, false).unwrap();
        assert_eq!(info.name, "pdf-tool");
        assert_eq!(info.files, vec!["scripts/run.py"]);
        assert!(!users.join("evil.txt").exists() && !home.path().join("evil.txt").exists());
        // 임시 폴더가 남지 않는다
        assert!(!fs::read_dir(&users).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with(".installing")));

        // 새로 만들기
        let created = create_user_skill("회의록: 정리").unwrap();
        assert_eq!(created.name, "회의록: 정리");
        assert!(created.error.is_none());
        assert!(create_user_skill("회의록: 정리").is_err());

        // 삭제는 사용자 Skill만
        assert!(delete_user_skill("project:p1:.skills/report").is_err());
        assert!(delete_user_skill("user:../x").is_err());
    }
    // -----------------------------------------------------------------------
    // Skill 자동 불러오기
    // -----------------------------------------------------------------------

    const WEEKLY_DESC: &str = "주간 업무 보고서를 회사 양식으로 작성한다. 사용자가 주간 보고, 주간 업무 보고서, 이번 주 업무 정리를 요청할 때 쓴다.";

    fn info(name: &str, description: &str) -> SkillInfo {
        SkillInfo {
            id: format!("project:p:.skills/{}", name),
            name: name.to_string(),
            description: description.to_string(),
            scope: "project".into(),
            location: format!(".skills/{}", name),
            enabled: true,
            error: None,
            shadowed: false,
            files: Vec::new(),
            dir: PathBuf::new(),
        }
    }

    fn sample_skills() -> Vec<SkillInfo> {
        vec![
            info("weekly-report", WEEKLY_DESC),
            info("meeting-minutes", "회의록을 정해진 양식으로 정리한다. 회의 내용 정리, 회의록 작성을 요청할 때 쓴다."),
            info("pptx-design", "발표 자료(PPTX) 슬라이드를 디자인 규칙에 맞게 만든다."),
        ]
    }

    fn picked(skills: &[SkillInfo], msg: &str) -> Option<(String, SelectReason)> {
        select_for_message(skills, msg).map(|(s, r)| (s.name, r))
    }

    #[test]
    fn explicit_name_or_slash_selects_the_skill() {
        let skills = sample_skills();
        assert_eq!(picked(&skills, "/weekly-report 써 줘"), Some(("weekly-report".into(), SelectReason::Explicit)));
        assert_eq!(picked(&skills, "Weekly-Report로 정리해 줘"), Some(("weekly-report".into(), SelectReason::Explicit)));
        assert_eq!(picked(&skills, "이건 meeting-minutes 양식으로"), Some(("meeting-minutes".into(), SelectReason::Explicit)));
        // 이름의 일부이거나 더 긴 낱말 안에 들어 있으면 이름을 부른 것이 아니다.
        assert_ne!(picked(&skills, "weekly-reports 폴더"), Some(("weekly-report".into(), SelectReason::Explicit)));
        assert_ne!(picked(&skills, "my-weekly-report"), Some(("weekly-report".into(), SelectReason::Explicit)));
    }

    #[test]
    fn korean_request_matching_the_description_selects_implicitly() {
        let skills = sample_skills();
        assert_eq!(
            picked(&skills, "이번 주 주간 업무 보고서를 만들어 줘"),
            Some(("weekly-report".into(), SelectReason::Implicit))
        );
        // 하니스 proj-skill 시나리오의 요청
        assert_eq!(
            picked(&skills, "notes/week.md 메모를 바탕으로 이번 주 주간 업무 보고서를 만들어 줘."),
            Some(("weekly-report".into(), SelectReason::Implicit))
        );
        assert_eq!(picked(&skills, "어제 회의 내용으로 회의록 작성해 줘"), Some(("meeting-minutes".into(), SelectReason::Implicit)));
    }

    #[test]
    fn unrelated_or_short_requests_select_nothing() {
        let skills = sample_skills();
        assert_eq!(picked(&skills, "안녕"), None);
        assert_eq!(picked(&skills, "엑셀 차트 그려 줘"), None);
        assert_eq!(picked(&skills, "계속"), None);
        assert_eq!(picked(&skills, ""), None);
        // 맞는 토큰이 하나뿐이면(비율이 높아도) 고르지 않는다.
        assert_eq!(picked(&skills, "보고"), None);
    }

    #[test]
    fn a_tie_selects_nothing() {
        let skills = vec![info("report-a", WEEKLY_DESC), info("report-b", WEEKLY_DESC)];
        assert_eq!(picked(&skills, "이번 주 주간 업무 보고서를 만들어 줘"), None);
        // 이름을 부르면 같은 설명이어도 그것을 고른다.
        assert_eq!(picked(&skills, "/report-b 로 이번 주 주간 업무 보고서"), Some(("report-b".into(), SelectReason::Explicit)));
    }

    #[test]
    fn disabled_broken_or_shadowed_skills_are_never_selected() {
        for f in [
            (|s: &mut SkillInfo| s.enabled = false) as fn(&mut SkillInfo),
            |s: &mut SkillInfo| s.error = Some("x".into()),
            |s: &mut SkillInfo| s.shadowed = true,
        ] {
            let mut skills = sample_skills();
            f(&mut skills[0]);
            assert_eq!(picked(&skills, "/weekly-report"), None);
            assert_eq!(picked(&skills, "이번 주 주간 업무 보고서를 만들어 줘"), None);
        }
    }

    #[test]
    fn auto_skill_text_has_header_and_respects_budget() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = info("weekly-report", WEEKLY_DESC);
        s.dir = dir.path().join("weekly-report");
        write(&s.dir.join(SKILL_FILE), &format!("---\nname: weekly-report\ndescription: {}\n---\n\n# 양식\n보고 양식 버전: W-7\n", WEEKLY_DESC));
        let auto = build_auto_skill(&s, SelectReason::Implicit, 32768, true).unwrap();
        assert!(auto.text.starts_with(&format!("[Skill: weekly-report — {}]", AUTO_HEADER_TAIL)), "{}", auto.text);
        assert!(auto.text.contains("보고 양식 버전: W-7"));
        assert!(!auto.truncated);
        assert_eq!(auto.name, "weekly-report");

        // 예산 = min(10,000, 컨텍스트 × 1.5 × 0.20)
        assert_eq!(auto_skill_budget(32768), 9_830);
        assert_eq!(auto_skill_budget(8192), 2_457);
        assert_eq!(auto_skill_budget(200_000), 10_000);

        // 긴 SKILL.md는 예산에 맞춰 자르고, 나머지를 읽는 방법을 적는다.
        let long = "가".repeat(5_000);
        write(&s.dir.join(SKILL_FILE), &format!("---\nname: weekly-report\ndescription: d\n---\n\n{}", long));
        let auto = build_auto_skill(&s, SelectReason::Explicit, 8192, true).unwrap();
        assert!(auto.truncated);
        assert!(auto.text.chars().count() <= auto_skill_budget(8192), "{}", auto.text.chars().count());
        assert!(auto.text.contains("read_skill_file"), "{}", auto.text);
        assert!(auto.text.contains("load_skill"), "{}", auto.text);
    }

    #[test]
    fn load_skill_for_the_auto_loaded_skill_gets_a_short_reply() {
        let auto = AutoSkill { name: "weekly-report".into(), reason: SelectReason::Implicit, text: "x".into(), truncated: false };
        let args = serde_json::json!({ "name": "Weekly-Report" });
        assert_eq!(already_loaded_reply(Some(&auto), "load_skill", &args).as_deref(), Some(ALREADY_LOADED));
        assert_eq!(already_loaded_reply(Some(&auto), "load_skill", &serde_json::json!({ "name": "other" })), None);
        assert_eq!(already_loaded_reply(Some(&auto), "read_skill_file", &args), None);
        assert_eq!(already_loaded_reply(None, "load_skill", &args), None);
        // 잘라서 넣었으면 전체를 다시 읽을 수 있어야 한다.
        let cut = AutoSkill { truncated: true, ..auto };
        assert_eq!(already_loaded_reply(Some(&cut), "load_skill", &args), None);
    }

    #[test]
    fn auto_record_is_a_completed_automatic_load_skill_card() {
        let auto = AutoSkill { name: "weekly-report".into(), reason: SelectReason::Implicit, text: "x".repeat(10), truncated: false };
        let r = auto.tool_record();
        assert!(r.automatic);
        assert_eq!((r.tool_name.as_str(), r.status.as_str(), r.round), ("load_skill", "completed", 0));
        let args: serde_json::Value = serde_json::from_str(&r.arguments_json).unwrap();
        assert_eq!(args["name"], "weekly-report");
        assert!(r.output.contains("weekly-report"), "{}", r.output);
        assert!(auto.matches_record(&r));
        let other = AutoSkill { name: "meeting-minutes".into(), ..auto.clone() };
        assert!(!other.matches_record(&r));
    }

    const PDF_DESC: &str = "Use this skill whenever the user wants to do anything with PDF files. This includes reading or extracting text from PDFs.";
    const DOCX_DESC: &str = "Use this skill when the user wants to create, read, or edit Word documents (.docx).";
    const XLSX_DESC: &str = "Use this skill any time a spreadsheet file is the primary input or output. Open, read, edit, or fix an existing .xlsx, .csv file; add columns, compute formulas, formatting, charting, cleaning messy data.";
    const REVIEW_DESC: &str = "Review code changes for security, performance, and correctness. Trigger with a PR URL or diff, review this before I merge, is this code safe.";

    #[test]
    fn everyday_requests_do_not_pick_a_skill_even_when_it_is_the_only_one() {
        let three = sample_skills();
        let alone = vec![info("weekly-report", WEEKLY_DESC)];
        for q in [
            "이번 주 날씨 알려 줘",
            "이번 주 일정 정리해 줘",
            "사용자 요청을 처리하는 코드를 작성해 줘",
            "회사 소개 글 작성해 줘",
            "사용자 목록 요청하는 API 만들어 줘",
            "파이썬으로 엑셀 파일 정리해 줘",
            "안녕",
            "엑셀 차트 그려 줘",
        ] {
            assert_eq!(picked(&three, q), None, "세 Skill: {}", q);
            assert_eq!(picked(&alone, q), None, "Skill 하나: {}", q);
        }
        let english = vec![info("pdf", PDF_DESC), info("docx", DOCX_DESC)];
        let pdf_alone = vec![info("pdf", PDF_DESC)];
        for q in ["explain this code to the user", "how do I use the git rebase command", "what is the capital of France"] {
            assert_eq!(picked(&english, q), None, "{}", q);
            assert_eq!(picked(&pdf_alone, q), None, "PDF 하나: {}", q);
        }
        // 흔한 낱말(회사·업무·text·files·add…)만 맞으면 고르지 않는다.
        for q in ["회사 업무 매뉴얼 만들어 줘", "회사 업무 규정 요약해 줘"] {
            assert_eq!(picked(&three, q), None, "세 Skill: {}", q);
            assert_eq!(picked(&alone, q), None, "Skill 하나: {}", q);
        }
        for q in ["write a function that reads text files", "find text in files with grep"] {
            assert_eq!(picked(&english, q), None, "{}", q);
            assert_eq!(picked(&pdf_alone, q), None, "PDF 하나: {}", q);
        }
        let xlsx_alone = vec![info("xlsx", XLSX_DESC)];
        assert_eq!(picked(&xlsx_alone, "add columns to the table in the README"), None);
        let review_alone = vec![info("code-review", REVIEW_DESC)];
        assert_eq!(picked(&review_alone, "write a report on the code changes"), None, "이름 토큰이라도 흔한 낱말(code)이면 치지 않는다");
        // 맞는 요청은 여전히 고른다.
        assert_eq!(picked(&pdf_alone, "extract the text from this pdf"), Some(("pdf".into(), SelectReason::Implicit)));
        assert_eq!(picked(&xlsx_alone, "add columns to this spreadsheet and compute formulas"), Some(("xlsx".into(), SelectReason::Implicit)));
        assert_eq!(picked(&review_alone, "review this code for security issues"), Some(("code-review".into(), SelectReason::Implicit)));
        assert_eq!(picked(&alone, "주간 보고 써 줘"), Some(("weekly-report".into(), SelectReason::Implicit)));
        assert_eq!(picked(&alone, "이번 주 주간 업무 보고서를 만들어 줘"), Some(("weekly-report".into(), SelectReason::Implicit)));
    }

    #[test]
    fn bare_names_need_a_dash_or_four_letters_and_korean_names_need_a_slash() {
        let skills = vec![info("pdf", PDF_DESC), info("요약", "긴 글을 세 줄로 줄인다."), info("docx", DOCX_DESC)];
        assert_eq!(picked(&skills, "/pdf 이거 봐 줘"), Some(("pdf".into(), SelectReason::Explicit)));
        assert_ne!(picked(&skills, "pdf 이거 봐 줘").map(|p| p.1), Some(SelectReason::Explicit), "세 글자 영문 이름만으로는 부르지 않는다");
        assert_eq!(picked(&skills, "/요약 해 줘"), Some(("요약".into(), SelectReason::Explicit)));
        assert_ne!(picked(&skills, "요약 해 줘").map(|p| p.1), Some(SelectReason::Explicit), "한글 이름은 /로만");
        assert_eq!(picked(&skills, "/요약하지 말고 그대로"), None, "한글도 이름 글자라 '요약하지' 안에서는 맞지 않는다");
        let long = vec![info("translate", "번역")];
        assert_eq!(picked(&long, "translate this"), Some(("translate".into(), SelectReason::Explicit)));
    }

    fn answer_with(records: Vec<crate::store::ToolExecutionRecord>) -> crate::store::ChatMessage {
        crate::store::ChatMessage { role: "assistant".into(), tool_records: records, ..Default::default() }
    }

    fn user(text: &str) -> crate::store::ChatMessage {
        crate::store::ChatMessage { role: "user".into(), content: text.into(), ..Default::default() }
    }

    fn auto(name: &str, reason: SelectReason) -> AutoSkill {
        AutoSkill { name: name.into(), reason, text: "x".into(), truncated: false }
    }

    #[test]
    fn follow_ups_keep_the_auto_loaded_skill_for_three_turns() {
        let first = answer_with(vec![auto("weekly-report", SelectReason::Implicit).tool_record()]);
        let sticky = || answer_with(vec![auto("weekly-report", SelectReason::Sticky).tool_record()]);
        let first_again = || answer_with(vec![auto("weekly-report", SelectReason::Implicit).tool_record()]);
        let mut h = vec![user("주간 보고서"), first];
        assert_eq!(sticky_from_history(&h).as_deref(), Some("weekly-report"));
        h.push(user("계속"));
        assert_eq!(sticky_from_history(&h).as_deref(), Some("weekly-report"), "끝의 사용자 메시지는 건너뛴다");
        for n in 1..=3 {
            h.push(sticky());
            h.push(user("계속"));
            assert_eq!(sticky_from_history(&h).is_some(), n < SKILL_STICKY_TURNS, "이어 쓴 턴 {}", n);
        }
        // 이어 쓰기 기록 뒤에 새로 고른 Skill이 있으면 그 앞 기록은 세지 않는다(I,S,S,S,I 다음 "계속"은 이어 쓴다).
        let mut fresh = vec![user("주간 보고서"), first_again()];
        for _ in 0..3 {
            fresh.push(user("계속"));
            fresh.push(sticky());
        }
        fresh.push(user("새 주간 보고서"));
        fresh.push(first_again());
        fresh.push(user("계속"));
        assert_eq!(sticky_from_history(&fresh).as_deref(), Some("weekly-report"));
        fresh.push(sticky());
        fresh.push(user("계속"));
        assert_eq!(sticky_from_history(&fresh).as_deref(), Some("weekly-report"), "새로 고른 뒤 이어 쓴 턴은 1");
        // 바로 앞 답변에 자동 기록이 없으면 이어 쓰지 않는다.
        assert_eq!(sticky_from_history(&[user("a"), answer_with(vec![]), user("b")]), None);
        assert_eq!(sticky_from_history(&[]), None);
    }

    #[test]
    fn sticky_skill_gets_its_own_header_and_is_skipped_for_a_different_task() {
        let root = tempfile::tempdir().unwrap();
        let body = "# 주간 업무 보고서 양식\n\n1. `## 1. 이번 주 한 일` - 표로 작성 (열: 요일, 내용, 결과)\n2. `## 2. 다음 주 할 일` - 번호 목록\n3. `## 3. 요청·이슈`\n파일은 `reports/주간보고_YYYY-MM-DD.md` (보고서를 쓰는 날짜)로 저장합니다.";
        write(&root.path().join(".skills/fit-weekly/SKILL.md"), &format!("---\nname: fit-weekly\ndescription: {}\n---\n\n{}", WEEKLY_DESC, body));
        let ctx = Some(("p-fit", root.path()));
        let first = auto_for_turn(ctx, "이번 주 주간 업무 보고서를 만들어 줘", &[], 32768, true).unwrap();
        assert_eq!(first.reason, SelectReason::Implicit);
        assert!(first.text.starts_with(&format!("[Skill: fit-weekly — {}]", AUTO_HEADER_TAIL)), "{}", first.text);
        let history = vec![user("이번 주 주간 업무 보고서를 만들어 줘"), answer_with(vec![first.tool_record()])];
        for q in ["계속", "날짜만 바꿔 줘", "3번 섹션 다시 써 줘", "다음 주 할 일 표에 결과 열을 추가해 줘"] {
            let s = auto_for_turn(ctx, q, &history, 32768, true).unwrap_or_else(|| panic!("이어 써야 한다: {}", q));
            assert_eq!(s.reason, SelectReason::Sticky, "{}", q);
            assert!(s.text.starts_with(&format!("[Skill: fit-weekly — {}]", STICKY_HEADER_TAIL)), "{}", s.text);
            assert!(!s.text.contains(AUTO_HEADER_TAIL), "이어 쓰기에는 '맞는 Skill로 판단' 문구가 없다");
        }
        for q in ["파이썬으로 CSV 합치는 코드 짜 줘", "회사 홈페이지 소개 문구 세 줄로 써 줘"] {
            assert!(auto_for_turn(ctx, q, &history, 32768, true).is_none(), "다른 일에는 이어 넣지 않는다: {}", q);
        }
    }

    #[test]
    fn auto_for_turn_uses_selection_then_sticky_and_resume_uses_the_card() {
        let root = tempfile::tempdir().unwrap();
        write(&root.path().join(".skills/sticky-weekly/SKILL.md"), &format!("---\nname: sticky-weekly\ndescription: {}\n---\n\n본문 W-7", WEEKLY_DESC));
        let ctx = Some(("p-sticky", root.path()));
        let first = auto_for_turn(ctx, "/sticky-weekly 써 줘", &[], 32768, true).unwrap();
        assert_eq!((first.name.as_str(), first.reason), ("sticky-weekly", SelectReason::Explicit));
        let history = vec![user("/sticky-weekly 써 줘"), answer_with(vec![first.tool_record()])];
        let next = auto_for_turn(ctx, "계속", &history, 32768, true).unwrap();
        assert_eq!((next.name.as_str(), next.reason), ("sticky-weekly", SelectReason::Sticky));
        assert!(next.text.contains("본문 W-7"));
        assert!(auto_for_turn(ctx, "계속", &history, 32768, false).is_none(), "설정을 끄면 이어 쓰지 않는다");
        // 승인 뒤 이어가기: 카드에 적힌 Skill 이름·이유 그대로
        let paused = answer_with(vec![next.tool_record()]);
        let resumed = auto_for_resume(ctx, &paused, 32768).unwrap();
        assert_eq!((resumed.name.as_str(), resumed.reason), ("sticky-weekly", SelectReason::Sticky));
        assert!(auto_for_resume(ctx, &answer_with(vec![]), 32768).is_none());
    }

    #[test]
    fn prompt_section_tells_the_model_not_to_reload_an_injected_skill() {
        let s = prompt_section(&sample_skills()).unwrap();
        assert!(s.contains("질문에 맞는 Skill이 [Skill: …] 로 이미 들어와 있으면 load_skill을 다시 부르지 말고 그 지침을 따르세요."), "{}", s);
    }
}
