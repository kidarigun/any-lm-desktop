//! 프로젝트 안의 Python 스크립트 실행.
//!
//! AI가 엑셀(.xlsx)처럼 직접 만들 수 없는 파일을 "만드는 스크립트"로 남겼을 때,
//! 사용자가 앱 안에서 바로 실행해 결과 파일을 얻을 수 있게 한다.
//! - Python 찾기: 프로젝트의 가상환경(.venv, venv)을 먼저, 없으면 시스템 Python(py -3, python, python3)
//! - 실행 위치는 프로젝트 루트 (에이전트 도구의 상대 경로와 같은 기준)
//! - 실행 전후로 프로젝트 파일을 비교해 새로 생기거나 바뀐 파일을 알려 준다.
//! - 필요한 패키지가 없으면(ModuleNotFoundError) 설치할 패키지 이름을 알려 준다.

use crate::fs_ops;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

pub const RUN_TIMEOUT: Duration = Duration::from_secs(180);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// 화면과 모델에 넘기는 출력 길이 (끝부분을 남긴다: 오류는 보통 마지막에 나온다)
const MAX_OUTPUT_CHARS: usize = 20_000;
const MAX_LISTED_FILES: usize = 100;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PythonInfo {
    /// 실제 python.exe 경로 (sys.executable)
    pub path: String,
    pub version: String,
    /// 가상환경 안의 Python인지 (pip --user를 쓸 수 없다)
    pub in_venv: bool,
    /// 프로젝트 폴더의 .venv / venv 인지
    pub project_venv: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcome {
    pub script: String,
    pub python: PythonInfo,
    /// 시간 초과로 중단되면 None
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    /// 실행으로 새로 생긴 파일 (프로젝트 기준 상대 경로)
    pub created: Vec<String>,
    /// 실행으로 내용이 바뀐 파일
    pub modified: Vec<String>,
    /// 없어서 실행이 실패한 모듈 이름 (예: openpyxl)
    pub missing_module: Option<String>,
    /// 그 모듈을 설치할 pip 패키지 이름 (예: docx → python-docx)
    pub suggested_package: Option<String>,
}

impl RunOutcome {
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// 새로 생기거나 바뀐 파일 (생긴 것 먼저)
    pub fn produced_files(&self) -> Vec<String> {
        self.created.iter().chain(self.modified.iter()).cloned().collect()
    }

    /// 에이전트(모델)에게 돌려줄 실행 결과 요약
    pub fn summary(&self) -> String {
        let mut out = String::new();
        let status = match (self.timed_out, self.exit_code) {
            (true, _) => format!("{}초 안에 끝나지 않아 중단됨", RUN_TIMEOUT.as_secs()),
            (false, Some(code)) => format!("종료 코드 {}", code),
            (false, None) => "비정상 종료".to_string(),
        };
        out.push_str(&format!(
            "[Python {} · {} · {:.1}초 · 실행 위치: 프로젝트 루트]\n",
            self.python.version,
            status,
            self.duration_ms as f64 / 1000.0
        ));
        if !self.created.is_empty() {
            out.push_str(&format!("새로 생긴 파일: {}\n", self.created.join(", ")));
        }
        if !self.modified.is_empty() {
            out.push_str(&format!("바뀐 파일: {}\n", self.modified.join(", ")));
        }
        if self.created.is_empty() && self.modified.is_empty() {
            out.push_str("프로젝트 폴더에 새로 생기거나 바뀐 파일이 없습니다.\n");
        }
        if !self.stdout.trim().is_empty() {
            out.push_str(&format!("--- 출력 ---\n{}\n", self.stdout.trim_end()));
        }
        if !self.stderr.trim().is_empty() {
            out.push_str(&format!("--- 오류 출력 ---\n{}\n", self.stderr.trim_end()));
        }
        if let (Some(module), Some(pkg)) = (&self.missing_module, &self.suggested_package) {
            out.push_str(&format!(
                "\n필요한 모듈 '{}'이(가) 설치되어 있지 않습니다. 사용자에게 도구 카드의 [실행] 창에서 '{}' 패키지를 설치한 뒤 다시 실행하도록 안내하세요.\n",
                module, pkg
            ));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Python 찾기
// ---------------------------------------------------------------------------

/// 시스템 Python은 한 번 찾으면 기억한다 (찾지 못한 경우는 기억하지 않아, 설치 후 바로 쓸 수 있다).
static SYSTEM_PYTHON: Mutex<Option<PythonInfo>> = Mutex::new(None);

const PROBE_CODE: &str =
    "import sys;print(sys.executable);print('%d.%d.%d'%sys.version_info[:3]);print(int(sys.prefix!=sys.base_prefix))";

fn command(program: &Path) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut c = tokio::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    c
}

/// `program args -c PROBE_CODE`를 실행해 실제 Python 3인지 확인한다.
/// (Windows의 "Microsoft Store로 이동" python.exe 껍데기는 여기서 걸러진다.)
async fn probe(program: &Path, args: &[&str], project_venv: bool) -> Option<PythonInfo> {
    let mut c = command(program);
    c.args(args).args(["-c", PROBE_CODE]);
    let out = tokio::time::timeout(PROBE_TIMEOUT, c.output()).await.ok()?.ok()?;
    if !out.status.success() {
        return None;
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout), project_venv)
}

fn parse_probe(stdout: &str, project_venv: bool) -> Option<PythonInfo> {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let path = lines.next()?.to_string();
    let version = lines.next()?.to_string();
    let in_venv = lines.next()? == "1";
    if !version.starts_with("3.") || !Path::new(&path).is_file() {
        return None;
    }
    Some(PythonInfo { path, version, in_venv, project_venv })
}

fn project_venv_candidates(root: &Path) -> Vec<PathBuf> {
    ["venv", ".venv", "env"]
        .iter()
        .flat_map(|dir| {
            [
                root.join(dir).join("Scripts").join("python.exe"),
                root.join(dir).join("bin").join("python"),
            ]
        })
        .filter(|p| p.is_file())
        .collect()
}

/// 이 프로젝트에서 쓸 Python. 없으면 None.
/// 찾는 순서: 프로젝트 가상환경 → 앱 전용 Python(문서·차트 패키지 포함) → PC에 설치된 Python
pub async fn find_python(root: Option<&Path>) -> Option<PythonInfo> {
    if let Some(root) = root {
        for exe in project_venv_candidates(root) {
            if let Some(info) = probe(&exe, &[], true).await {
                return Some(info);
            }
        }
    }
    if crate::runtime::python_installed() {
        if let Some(info) = probe(&crate::runtime::pyenv_python(), &[], false).await {
            return Some(info);
        }
    }
    find_system_python().await
}

/// 앱 전용 Python을 설치·삭제한 뒤 기억해 둔 결과를 지운다.
pub fn forget_cached_python() {
    if let Ok(mut g) = SYSTEM_PYTHON.lock() {
        *g = None;
    }
}

/// PC에 따로 설치된 Python (앱 전용 Python은 빼고)
pub async fn find_system_python() -> Option<PythonInfo> {
    if let Some(info) = SYSTEM_PYTHON.lock().ok().and_then(|g| g.clone()) {
        if Path::new(&info.path).is_file() {
            return Some(info);
        }
    }
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "windows") {
        &[("py", &["-3"]), ("python", &[]), ("python3", &[])]
    } else {
        &[("python3", &[]), ("python", &[])]
    };
    for (program, args) in candidates {
        if let Some(info) = probe(Path::new(program), args, false).await {
            if let Ok(mut g) = SYSTEM_PYTHON.lock() {
                *g = Some(info.clone());
            }
            return Some(info);
        }
    }
    None
}

pub const NOT_FOUND_MESSAGE: &str = "Python이 아직 없습니다. 왼쪽 아래 [API 및 모델 설정] → '실행 환경'에서 [Python 설치]를 누르면 앱이 자동으로 설치합니다 (내려받기 약 100MB, 한 번만).";

// ---------------------------------------------------------------------------
// 실행
// ---------------------------------------------------------------------------

/// 바뀐 파일을 찾을 때 건너뛰는 폴더 (크고, 결과 파일이 생길 곳이 아니다)
const SKIP_DIRS: &[&str] = &[".git", "node_modules", ".venv", "venv", "env", "__pycache__", "target", ".idea", ".vscode", ".mypy_cache", ".pytest_cache"];
const SCAN_MAX_DEPTH: usize = 8;
const SCAN_MAX_ENTRIES: usize = 50_000;

type FileStamp = (u64, Option<SystemTime>);

fn scan_files(root: &Path) -> HashMap<String, FileStamp> {
    fn walk(dir: &Path, rel: &str, depth: usize, out: &mut HashMap<String, FileStamp>, seen: &mut usize) {
        if depth > SCAN_MAX_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            *seen += 1;
            if *seen > SCAN_MAX_ENTRIES {
                return;
            }
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let child = if rel.is_empty() { name.clone() } else { format!("{}/{}", rel, name) };
            if meta.is_dir() {
                if !SKIP_DIRS.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    walk(&entry.path(), &child, depth + 1, out, seen);
                }
            } else if !name.ends_with(".pyc") {
                out.insert(child, (meta.len(), meta.modified().ok()));
            }
        }
    }
    let mut out = HashMap::new();
    let mut seen = 0;
    walk(root, "", 0, &mut out, &mut seen);
    out
}

fn diff_files(before: &HashMap<String, FileStamp>, after: &HashMap<String, FileStamp>) -> (Vec<String>, Vec<String>) {
    let mut created: Vec<String> = after.keys().filter(|k| !before.contains_key(*k)).cloned().collect();
    let mut modified: Vec<String> = after
        .iter()
        .filter(|(k, v)| before.get(*k).is_some_and(|b| b != *v))
        .map(|(k, _)| k.clone())
        .collect();
    created.sort();
    modified.sort();
    created.truncate(MAX_LISTED_FILES);
    modified.truncate(MAX_LISTED_FILES);
    (created, modified)
}

/// 출력이 너무 길면 끝부분만 남긴다.
fn tail(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_OUTPUT_CHARS {
        return text.to_string();
    }
    let skip = count - MAX_OUTPUT_CHARS;
    format!("(앞부분 {}자 생략)\n{}", skip, text.chars().skip(skip).collect::<String>())
}

/// `ModuleNotFoundError: No module named 'x.y'` → "x"
pub fn missing_module(stderr: &str) -> Option<String> {
    const KEY: &str = "No module named '";
    let start = stderr.rfind(KEY)? + KEY.len();
    let name = &stderr[start..];
    let name = &name[..name.find('\'')?];
    let top = name.split('.').next()?.trim();
    (!top.is_empty() && top.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| top.to_string())
}

/// import 이름과 pip 패키지 이름이 다른 흔한 경우
pub fn package_for_module(module: &str) -> String {
    let known: &[(&str, &str)] = &[
        ("docx", "python-docx"),
        ("pptx", "python-pptx"),
        ("PIL", "Pillow"),
        ("cv2", "opencv-python"),
        ("yaml", "PyYAML"),
        ("sklearn", "scikit-learn"),
        ("skimage", "scikit-image"),
        ("bs4", "beautifulsoup4"),
        ("dateutil", "python-dateutil"),
        ("dotenv", "python-dotenv"),
        ("win32com", "pywin32"),
        ("win32api", "pywin32"),
        ("fitz", "PyMuPDF"),
        ("Crypto", "pycryptodome"),
    ];
    known
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, p)| p.to_string())
        .unwrap_or_else(|| module.to_string())
}

/// pip에 넘겨도 안전한 패키지 이름인지 (옵션·경로·URL·버전 지정 불가)
pub fn is_valid_package_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && name.len() <= 100
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn is_python_script(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("py") || e.eq_ignore_ascii_case("pyw"))
}

/// 프로젝트 안의 .py 스크립트를 프로젝트 루트에서 실행한다.
pub async fn run_script(root: &Path, relative_path: &str, timeout: Duration) -> Result<RunOutcome, String> {
    let script = fs_ops::resolve_safe_path(root, relative_path)?;
    run_script_at(root, &script, &relative_path.replace('\\', "/"), timeout).await
}

/// 프로젝트 밖(예: Skill 폴더)에 있을 수 있는 스크립트를 프로젝트 루트에서 실행한다.
/// `display`: 결과에 보여 줄 스크립트 이름
pub async fn run_script_at(root: &Path, script: &Path, display: &str, timeout: Duration) -> Result<RunOutcome, String> {
    let script = script.to_path_buf();
    if !script.is_file() {
        return Err(format!("스크립트 파일을 찾을 수 없습니다: {}", display));
    }
    if !is_python_script(&script) {
        return Err("Python 스크립트(.py)만 실행할 수 있습니다.".to_string());
    }
    let python = find_python(Some(root)).await.ok_or_else(|| NOT_FOUND_MESSAGE.to_string())?;

    let scan_root = root.to_path_buf();
    let before = tokio::task::spawn_blocking(move || scan_files(&scan_root)).await.map_err(|e| e.to_string())?;

    let mut c = command(Path::new(&python.path));
    c.args(["-X", "utf8", "-u"])
        .arg(&script)
        .current_dir(crate::fs_ops::process_path(root))
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        // matplotlib 등이 창을 띄우지 않고 파일로만 저장하게 한다.
        .env("MPLBACKEND", "Agg");
    // 어떤 Python으로 실행하든 차트의 한글이 깨지지 않게 한글 글꼴(맑은 고딕) 설정을 쓴다.
    if let Some(dir) = crate::runtime::ensure_mpl_dir() {
        c.env("MPLCONFIGDIR", dir);
    }
    let started = Instant::now();
    let child = c.spawn().map_err(|e| format!("Python을 실행할 수 없습니다: {}", e))?;
    // 시간 초과면 future가 버려지면서 kill_on_drop으로 프로세스가 종료된다.
    let result = tokio::time::timeout(timeout, child.wait_with_output()).await;
    let duration_ms = started.elapsed().as_millis() as u64;

    let (exit_code, timed_out, stdout, stderr) = match result {
        Ok(Ok(out)) => (
            out.status.code(),
            false,
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        ),
        Ok(Err(e)) => return Err(format!("Python 실행 중 오류: {}", e)),
        Err(_) => (None, true, String::new(), String::new()),
    };

    let scan_root = root.to_path_buf();
    let after = tokio::task::spawn_blocking(move || scan_files(&scan_root)).await.map_err(|e| e.to_string())?;
    let (created, modified) = diff_files(&before, &after);
    let module = if exit_code == Some(0) { None } else { missing_module(&stderr) };

    Ok(RunOutcome {
        script: display.to_string(),
        python,
        exit_code,
        timed_out,
        duration_ms,
        stdout: tail(&stdout),
        stderr: tail(&stderr),
        created,
        modified,
        suggested_package: module.as_deref().map(package_for_module),
        missing_module: module,
    })
}

const SYNTAX_CHECK_CODE: &str = "import ast,sys\nsrc=open(sys.argv[1],encoding='utf-8-sig').read()\ntry:\n    ast.parse(src,sys.argv[1])\nexcept SyntaxError as e:\n    print(f'{e.msg} (줄 {e.lineno}, 칸 {e.offset})')\n    print((e.text or '').rstrip())\n    sys.exit(1)\n";

/// .py 파일의 문법만 검사한다 (실행하지 않음). 문법 오류가 있으면 설명을 돌려준다.
/// Python이 없거나 검사 자체가 실패하면 None (쓰기 결과를 막지 않는다).
/// `python tests/test_calc.py`처럼 스크립트 하나만 실행하는 명령이면 그 스크립트 경로를 돌려준다.
/// 모델이 run_python 대신 run_command를 쓴 경우에도 앱의 Python(시스템에 Python이 없어도 동작)으로 실행하기 위함이다.
pub fn simple_script_command(command: &str) -> Option<String> {
    let mut parts = command.split_whitespace();
    let program = parts.next()?.to_lowercase();
    let script = parts.next()?;
    if parts.next().is_some() {
        return None; // 인자·파이프가 붙은 명령은 그대로 셸에서 실행한다
    }
    let is_python = matches!(program.as_str(), "python" | "python3" | "py" | "python.exe" | "py.exe");
    let script = script.trim_matches(|c| c == '"' || c == '\'');
    (is_python && script.to_lowercase().ends_with(".py")).then(|| script.to_string())
}

pub async fn check_syntax(root: &Path, relative_path: &str) -> Option<String> {
    let script = fs_ops::resolve_safe_path(root, relative_path).ok()?;
    if !is_python_script(&script) || !script.is_file() {
        return None;
    }
    let python = find_python(Some(root)).await?;
    let mut c = command(Path::new(&python.path));
    c.args(["-X", "utf8", "-c", SYNTAX_CHECK_CODE]).arg(&script).env("PYTHONIOENCODING", "utf-8");
    let out = tokio::time::timeout(PROBE_TIMEOUT, c.output()).await.ok()?.ok()?;
    if out.status.code() != Some(1) {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    (!text.is_empty()).then_some(text)
}

/// `python -m pip install <package>` (가상환경이 아니면 --user로 사용자 폴더에 설치)
pub async fn install_package(root: Option<&Path>, package: &str) -> Result<String, String> {
    if !is_valid_package_name(package) {
        return Err(format!("설치할 수 없는 패키지 이름입니다: {}", package));
    }
    let python = find_python(root).await.ok_or_else(|| NOT_FOUND_MESSAGE.to_string())?;
    let mut c = command(Path::new(&python.path));
    c.args(["-m", "pip", "install", "--disable-pip-version-check", "--no-input"]);
    if !python.in_venv {
        c.arg("--user");
    }
    c.arg(package).env("PYTHONIOENCODING", "utf-8");
    if let Some(root) = root {
        c.current_dir(crate::fs_ops::process_path(root));
    }
    let out = tokio::time::timeout(INSTALL_TIMEOUT, c.output())
        .await
        .map_err(|_| format!("{}초 안에 설치가 끝나지 않아 중단했습니다.", INSTALL_TIMEOUT.as_secs()))?
        .map_err(|e| format!("pip을 실행할 수 없습니다: {}", e))?;
    let text = tail(&format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ));
    if out.status.success() {
        Ok(text)
    } else {
        Err(format!("'{}' 설치에 실패했습니다.\n{}", package, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_missing_module_and_package() {
        let err = "Traceback (most recent call last):\n  File \"make.py\", line 1, in <module>\n    import openpyxl\nModuleNotFoundError: No module named 'openpyxl'\n";
        assert_eq!(missing_module(err).as_deref(), Some("openpyxl"));
        assert_eq!(missing_module("ModuleNotFoundError: No module named 'docx.shared'").as_deref(), Some("docx"));
        assert_eq!(package_for_module("docx"), "python-docx");
        assert_eq!(package_for_module("PIL"), "Pillow");
        assert_eq!(package_for_module("pandas"), "pandas");
        assert_eq!(missing_module("NameError: name 'x' is not defined"), None);
        assert_eq!(missing_module("No module named 'a b; rm'"), None);
    }

    #[test]
    fn package_names_cannot_smuggle_options() {
        for ok in ["openpyxl", "python-docx", "PyYAML", "ruamel.yaml", "scikit_learn"] {
            assert!(is_valid_package_name(ok), "{}", ok);
        }
        for bad in ["", "-r", "--index-url=http://x", "pkg==1.0", "../evil", "a b", "git+https://x", "C:\\x.whl"] {
            assert!(!is_valid_package_name(bad), "{}", bad);
        }
    }

    #[test]
    fn probe_output_must_be_python3_with_real_path() {
        let exe = std::env::current_exe().unwrap();
        let ok = format!("{}\n3.12.1\n0\n", exe.display());
        let info = parse_probe(&ok, false).unwrap();
        assert_eq!(info.version, "3.12.1");
        assert!(!info.in_venv);
        assert!(parse_probe(&format!("{}\n2.7.18\n0\n", exe.display()), false).is_none());
        assert!(parse_probe("C:\\nope\\python.exe\n3.12.1\n0\n", false).is_none());
        assert!(parse_probe("", false).is_none());
    }

    #[test]
    fn detects_created_and_modified_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("keep.txt"), "a").unwrap();
        std::fs::write(root.join("change.txt"), "a").unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        let before = scan_files(root);
        std::fs::write(root.join("change.txt"), "abc").unwrap();
        std::fs::create_dir_all(root.join("out")).unwrap();
        std::fs::write(root.join("out/report.xlsx"), "x").unwrap();
        std::fs::write(root.join("node_modules/x/ignored.js"), "x").unwrap();
        std::fs::write(root.join("cache.pyc"), "x").unwrap();
        let (created, modified) = diff_files(&before, &scan_files(root));
        assert_eq!(created, vec!["out/report.xlsx".to_string()]);
        assert_eq!(modified, vec!["change.txt".to_string()]);
    }

    #[test]
    fn long_output_keeps_the_end() {
        let long = format!("{}끝", "가".repeat(MAX_OUTPUT_CHARS + 10));
        let t = tail(&long);
        assert!(t.starts_with("(앞부분 11자 생략)"));
        assert!(t.ends_with('끝'));
    }

    /// 실제 Python이 있는 컴퓨터에서만 의미가 있다 (없으면 건너뛴다).
    #[tokio::test]
    async fn runs_a_real_script_in_the_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        if find_python(Some(root)).await.is_none() {
            eprintln!("Python이 없어 건너뜀");
            return;
        }
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(
            root.join("scripts/make.py"),
            "import os\nos.makedirs('out', exist_ok=True)\nopen('out/결과.csv','w',encoding='utf-8').write('이름,값\\n가,1\\n')\nprint('저장: out/결과.csv')\n",
        )
        .unwrap();
        let r = run_script(root, "scripts/make.py", Duration::from_secs(60)).await.unwrap();
        assert!(r.succeeded(), "{:?}", r);
        assert_eq!(r.created, vec!["out/결과.csv".to_string()]);
        assert!(r.stdout.contains("저장: out/결과.csv"), "{}", r.stdout);
        assert!(r.summary().contains("새로 생긴 파일: out/결과.csv"));

        std::fs::write(root.join("bad.py"), "import surely_not_installed_pkg_xyz\n").unwrap();
        let r = run_script(root, "bad.py", Duration::from_secs(60)).await.unwrap();
        assert!(!r.succeeded());
        assert_eq!(r.missing_module.as_deref(), Some("surely_not_installed_pkg_xyz"));

        assert!(run_script(root, "../outside.py", Duration::from_secs(5)).await.is_err());
        std::fs::write(root.join("note.txt"), "x").unwrap();
        assert!(run_script(root, "note.txt", Duration::from_secs(5)).await.is_err());

        // 문법 검사: 실행하지 않고 오류 줄을 알려 준다.
        std::fs::write(root.join("broken.py"), "if True:\n    x = 1\nelse:\n    y = 2\nelse:\n    z = 3\n").unwrap();
        let msg = check_syntax(root, "broken.py").await.expect("문법 오류를 찾아야 함");
        assert!(msg.contains("줄 5"), "{}", msg);
        assert_eq!(check_syntax(root, "scripts/make.py").await, None);
        assert_eq!(check_syntax(root, "note.txt").await, None);

        std::fs::write(root.join("slow.py"), "import time\ntime.sleep(30)\n").unwrap();
        let r = run_script(root, "slow.py", Duration::from_secs(2)).await.unwrap();
        assert!(r.timed_out && r.exit_code.is_none());
    }
}
