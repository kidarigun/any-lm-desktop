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
        .unwrap_or_else(|| std::env::temp_dir().join("exaone-desktop-skills"))
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
        "\n\n[Skill]\n사용자가 설치한 작업 지침(Skill)이 있습니다. 요청이 아래 Skill의 설명에 해당하면, 답하거나 작업하기 전에 먼저 load_skill로 그 Skill의 지침을 읽고 그대로 따르세요. 해당하지 않으면 쓰지 마세요.\n",
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
}
