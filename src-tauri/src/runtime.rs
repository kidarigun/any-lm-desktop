//! 앱 전용 실행 환경 (Python·Node.js·uv) 설치와 관리.
//!
//! 일반 사용자 PC에는 Python이나 Node.js가 없는 경우가 많다. 사용자가 버튼을 누르면
//! 공식 배포본을 **버전 고정 + sha256 확인** 후 앱 데이터 폴더(%LOCALAPPDATA%)에 내려받아 쓴다.
//! PC에 설치된 다른 Python·Node.js와 섞이지 않고, 관리자 권한도 필요 없다.
//!
//! - uv: Python 실행기·패키지 관리자. Python MCP 서버(uvx)와 앱 전용 Python 설치에 쓴다.
//! - Python: uv로 3.13을 설치하고, 문서·차트용 패키지를 넣은 앱 전용 가상환경을 만든다.
//! - Node.js: npx로 실행하는 MCP 서버에 쓴다.

use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;

// ---- 고정 버전 (올릴 때는 공식 배포처의 sha256도 함께 바꾼다) ----
pub const UV_VERSION: &str = "0.13.8";
const UV_URL: &str = "https://github.com/astral-sh/uv/releases/download/0.13.8/uv-x86_64-pc-windows-msvc.zip";
const UV_SHA256: &str = "529e78a9cec6cd7e3e014f7ecd8bda0c113969d4ed0f2efeedb85c30943325b8";

pub const NODE_VERSION: &str = "26.13.1";
const NODE_URL: &str = "https://nodejs.org/dist/v26.13.1/node-v26.13.1-win-x64.zip";
const NODE_SHA256: &str = "e2edda75bfb089654705cf65a13b58933bfd5f57148e8ebda916f58a2474f1f7";

pub const PYTHON_VERSION: &str = "3.13";
/// 앱 전용 Python에 넣는 패키지 (엑셀·워드·파워포인트·PDF·차트·데이터)
pub const PYTHON_PACKAGES: &[&str] = &["openpyxl", "python-docx", "python-pptx", "pypdf", "matplotlib", "pandas", "pillow"];

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

static ROOT: OnceLock<PathBuf> = OnceLock::new();
static JOBS: LazyLock<Mutex<HashMap<String, Job>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 앱 시작 시 한 번: 실행 환경을 둘 폴더 (%LOCALAPPDATA%\io.github.kidarigun.anylm-desktop\runtimes)
pub fn set_root(dir: PathBuf) {
    let _ = ROOT.set(dir);
}

fn root() -> PathBuf {
    ROOT.get().cloned().unwrap_or_else(|| std::env::temp_dir().join("exaone-desktop-runtimes"))
}

pub fn uv_dir() -> PathBuf {
    root().join("uv")
}
pub fn node_dir() -> PathBuf {
    root().join("node")
}
fn python_install_dir() -> PathBuf {
    root().join("python")
}
pub fn pyenv_dir() -> PathBuf {
    root().join("pyenv")
}
pub fn pyenv_python() -> PathBuf {
    if cfg!(windows) {
        pyenv_dir().join("Scripts").join("python.exe")
    } else {
        pyenv_dir().join("bin").join("python")
    }
}
/// 앱 전용 Python의 matplotlib 설정 (한글 글꼴)
pub fn mpl_dir() -> PathBuf {
    root().join("mplconfig")
}
fn cache_dir() -> PathBuf {
    root().join("cache")
}
fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}
fn uv_exe() -> PathBuf {
    uv_dir().join(exe("uv"))
}

pub fn uv_installed() -> bool {
    uv_exe().is_file()
}
pub fn node_installed() -> bool {
    node_dir().join(exe("node")).is_file()
}
/// 앱 전용 Python 설치가 끝까지 성공했다는 표시
const COMPLETE_MARK: &str = ".exaone-complete";

pub fn python_installed() -> bool {
    pyenv_python().is_file() && pyenv_dir().join(COMPLETE_MARK).is_file()
}

/// 자식 프로세스에서 먼저 찾을 실행 파일 폴더 (설치된 것만)
pub fn path_dirs() -> Vec<PathBuf> {
    [node_dir(), uv_dir()].into_iter().filter(|d| d.is_dir()).collect()
}

/// MCP 서버 등 자식 프로세스에 줄 환경 변수: 앱 전용 Node.js·uv를 PATH 앞에 둔다.
pub fn child_env() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let dirs = path_dirs();
    if !dirs.is_empty() {
        let mut all: Vec<PathBuf> = dirs;
        if let Some(p) = std::env::var_os("PATH") {
            all.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(all) {
            out.push(("PATH".to_string(), joined.to_string_lossy().to_string()));
        }
    }
    if uv_installed() {
        // uvx가 내려받는 Python·패키지도 앱 폴더에 둔다.
        out.push(("UV_PYTHON_INSTALL_DIR".to_string(), python_install_dir().to_string_lossy().to_string()));
        out.push(("UV_CACHE_DIR".to_string(), cache_dir().join("uv").to_string_lossy().to_string()));
    }
    out
}

// ---------------------------------------------------------------------------
// 진행 상황
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub running: bool,
    pub message: String,
    pub percent: Option<u8>,
    pub error: Option<String>,
}

fn set_job(kind: &str, f: impl FnOnce(&mut Job)) {
    if let Ok(mut j) = JOBS.lock() {
        f(j.entry(kind.to_string()).or_default());
    }
}

fn progress(kind: &str, message: &str, percent: Option<u8>) {
    set_job(kind, |j| {
        j.message = message.to_string();
        j.percent = percent;
    });
}

fn job(kind: &str) -> Job {
    JOBS.lock().ok().and_then(|j| j.get(kind).cloned()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 상태
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    /// "python" | "node" | "uv"
    pub id: String,
    pub label: String,
    /// 무엇에 쓰는지
    pub purpose: String,
    /// 앱이 설치한 것이 있는지
    pub app_installed: bool,
    /// PC에 따로 설치된 것이 있는지
    pub system: bool,
    pub version: String,
    /// 내려받을 크기 안내
    pub size_hint: String,
    pub job: Job,
}

fn pyenv_version() -> String {
    std::fs::read_to_string(pyenv_dir().join("pyvenv.cfg"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == "version_info" || k.trim() == "version").map(|(_, v)| v.trim().to_string()))
        })
        .unwrap_or_else(|| PYTHON_VERSION.to_string())
}

pub async fn status() -> Vec<RuntimeInfo> {
    let system_python = crate::python_runner::find_system_python().await.is_some();
    vec![
        RuntimeInfo {
            id: "python".into(),
            label: "Python".into(),
            purpose: "차트·서식 문서를 만드는 Python 스크립트, Skill 스크립트 실행".into(),
            app_installed: python_installed(),
            system: system_python,
            version: if python_installed() { pyenv_version() } else { String::new() },
            size_hint: "내려받기 약 100MB, 설치 후 약 300MB (uv 포함, 엑셀·워드·파워포인트·PDF·차트 패키지)".into(),
            job: job("python"),
        },
        RuntimeInfo {
            id: "node".into(),
            label: "Node.js".into(),
            purpose: "npx로 실행하는 MCP 서버 (파일, 기억 저장소 등)".into(),
            app_installed: node_installed(),
            system: crate::mcp::system_program_available("npx"),
            version: if node_installed() { NODE_VERSION.into() } else { String::new() },
            size_hint: "약 35MB 내려받기".into(),
            job: job("node"),
        },
        RuntimeInfo {
            id: "uv".into(),
            label: "uv".into(),
            purpose: "uvx로 실행하는 Python MCP 서버 (Google Workspace, 시간, 웹 페이지 가져오기 등)".into(),
            app_installed: uv_installed(),
            system: crate::mcp::system_program_available("uvx"),
            version: if uv_installed() { UV_VERSION.into() } else { String::new() },
            size_hint: "약 16MB 내려받기".into(),
            job: job("uv"),
        },
    ]
}

// ---------------------------------------------------------------------------
// 설치
// ---------------------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    crate::catalog::sha256_hex(bytes)
}

/// 내려받으며 진행률을 알리고, sha256이 맞을 때만 돌려준다.
async fn download(kind: &str, label: &str, url: &str, sha256: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent(concat!("Any-LM-Desktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("{}을(를) 내려받지 못했습니다. 인터넷 연결을 확인하세요. ({})", label, e))?;
    if !resp.status().is_success() {
        return Err(format!("{}을(를) 내려받지 못했습니다 (HTTP {}).", label, resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("내려받는 중 오류: {}", e))? {
        buf.extend_from_slice(&chunk);
        if buf.len() as u64 > MAX_DOWNLOAD {
            return Err("파일이 예상보다 큽니다. 내려받기를 멈췄습니다.".into());
        }
        let pct = if total > 0 { Some((buf.len() as u64 * 100 / total).min(100) as u8) } else { None };
        progress(kind, &format!("{} 내려받는 중... {:.1}MB", label, buf.len() as f64 / 1_048_576.0), pct);
    }
    if sha256_hex(&buf) != sha256 {
        return Err(format!("내려받은 {} 파일이 공식 배포본과 다릅니다(sha256 불일치). 설치하지 않았습니다.", label));
    }
    Ok(buf)
}

/// zip을 `dest`에 푼다. `strip`이 있으면 그 최상위 폴더 안의 내용만 꺼낸다.
fn extract_zip(bytes: &[u8], dest: &Path, strip: Option<&str>) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("압축 파일 오류: {}", e))?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(path) = entry.enclosed_name() else { continue };
        let rel = match strip {
            Some(prefix) => match path.strip_prefix(prefix) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            },
            None => path,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        std::fs::write(&out, data).map_err(|e| format!("파일을 쓰지 못했습니다 ({}): {}", rel.display(), e))?;
    }
    Ok(())
}

/// 임시 폴더에 준비한 뒤 바꿔 끼운다 (중간에 실패해도 기존 설치가 망가지지 않게).
fn replace_dir(tmp: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        std::fs::remove_dir_all(target).map_err(|e| format!("기존 설치를 지우지 못했습니다 (사용 중인 프로그램을 닫고 다시 시도하세요): {}", e))?;
    }
    std::fs::rename(tmp, target).map_err(|e| e.to_string())
}

async fn install_uv() -> Result<(), String> {
    let bytes = download("uv", "uv", UV_URL, UV_SHA256).await?;
    progress("uv", "uv 설치하는 중...", None);
    tokio::task::spawn_blocking(move || {
        let tmp = root().join("uv.tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        extract_zip(&bytes, &tmp, None)?;
        if !tmp.join(exe("uv")).is_file() {
            return Err("uv 압축 파일에 uv.exe가 없습니다.".to_string());
        }
        replace_dir(&tmp, &uv_dir())
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn install_node() -> Result<(), String> {
    let bytes = download("node", "Node.js", NODE_URL, NODE_SHA256).await?;
    progress("node", "Node.js 설치하는 중...", None);
    tokio::task::spawn_blocking(move || {
        let tmp = root().join("node.tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        extract_zip(&bytes, &tmp, Some(&format!("node-v{}-win-x64", NODE_VERSION)))?;
        if !tmp.join(exe("node")).is_file() {
            return Err("Node.js 압축 파일에 node.exe가 없습니다.".to_string());
        }
        replace_dir(&tmp, &node_dir())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// uv 명령을 실행한다 (창 없이, 앱 폴더를 쓰도록).
async fn run_uv(args: &[&str]) -> Result<String, String> {
    let mut c = tokio::process::Command::new(uv_exe());
    c.args(args)
        .env("UV_PYTHON_INSTALL_DIR", python_install_dir())
        .env("UV_CACHE_DIR", cache_dir().join("uv"))
        .env("UV_NO_PROGRESS", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let out = tokio::time::timeout(COMMAND_TIMEOUT, c.output())
        .await
        .map_err(|_| "설치가 너무 오래 걸려 멈췄습니다.".to_string())?
        .map_err(|e| format!("uv를 실행하지 못했습니다: {}", e))?;
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() {
        let tail: String = stderr.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(format!("설치 중 오류가 났습니다:\n{}", tail));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// matplotlib이 한글을 네모(□)로 그리지 않게 맑은 고딕을 기본 글꼴로 둔다.
fn write_matplotlibrc() -> Result<(), String> {
    std::fs::create_dir_all(mpl_dir()).map_err(|e| e.to_string())?;
    std::fs::write(
        mpl_dir().join("matplotlibrc"),
        "font.family: Malgun Gothic, sans-serif\naxes.unicode_minus: False\nbackend: Agg\n",
    )
    .map_err(|e| e.to_string())
}

/// 한글 글꼴 설정 폴더를 준비해 돌려준다 (없으면 만든다). 앱 전용 Python뿐 아니라
/// 시스템·가상환경 Python으로 실행할 때도 쓴다: 글꼴 경고가 나오면 모델이 글꼴을 고치느라 호출을 여러 번 쓴다.
pub fn ensure_mpl_dir() -> Option<PathBuf> {
    if !mpl_dir().join("matplotlibrc").is_file() {
        write_matplotlibrc().ok()?;
    }
    Some(mpl_dir())
}

async fn install_python() -> Result<(), String> {
    if !uv_installed() {
        install_uv().await.map_err(|e| format!("uv: {}", e))?;
        set_job("python", |j| j.percent = None);
    }
    progress("python", &format!("Python {} 내려받아 설치하는 중... (1/3)", PYTHON_VERSION), None);
    run_uv(&["python", "install", PYTHON_VERSION]).await?;

    progress("python", "앱 전용 Python 환경 만드는 중... (2/3)", None);
    // 가상환경은 안에 경로가 적혀 있어 옮길 수 없으므로 정식 위치에 바로 만들고,
    // 끝까지 성공했을 때만 완료 표시 파일을 남긴다 (중간에 실패하면 설치 안 된 것으로 본다).
    let target = pyenv_dir();
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| format!("기존 Python 환경을 지우지 못했습니다 (Python 실행을 멈추고 다시 시도하세요): {}", e))?;
    }
    let target_s = target.to_string_lossy().to_string();
    run_uv(&["venv", "--seed", "--python", PYTHON_VERSION, "--python-preference", "only-managed", &target_s]).await?;

    progress("python", "문서·차트 패키지 설치하는 중... (3/3)", None);
    let py_s = pyenv_python().to_string_lossy().to_string();
    let mut args = vec!["pip", "install", "--python", py_s.as_str()];
    args.extend(PYTHON_PACKAGES.iter().copied());
    run_uv(&args).await?;
    write_matplotlibrc()?;
    std::fs::write(target.join(COMPLETE_MARK), PYTHON_PACKAGES.join(" ")).map_err(|e| e.to_string())?;
    // 내려받은 설치 파일 캐시는 더 필요 없다 (디스크를 절반 가까이 차지한다).
    let _ = run_uv(&["cache", "clean"]).await;
    crate::python_runner::forget_cached_python();
    Ok(())
}

/// 설치한다. 같은 것을 설치 중이면 오류. 끝날 때까지 기다린다 (진행 상황은 status()로 본다).
pub async fn install(kind: &str) -> Result<(), String> {
    let label = match kind {
        "python" => "Python",
        "node" => "Node.js",
        "uv" => "uv",
        _ => return Err(format!("알 수 없는 실행 환경입니다: {}", kind)),
    };
    {
        let mut jobs = JOBS.lock().map_err(|e| e.to_string())?;
        let j = jobs.entry(kind.to_string()).or_default();
        if j.running {
            return Err(format!("{}을(를) 이미 설치하고 있습니다.", label));
        }
        *j = Job { running: true, message: format!("{} 설치 준비 중...", label), ..Default::default() };
    }
    let _ = std::fs::create_dir_all(root());
    let res = match kind {
        "python" => install_python().await,
        "node" => install_node().await,
        _ => install_uv().await,
    };
    set_job(kind, |j| {
        j.running = false;
        j.percent = None;
        match &res {
            Ok(()) => {
                j.message = format!("{} 설치를 마쳤습니다.", label);
                j.error = None;
            }
            Err(e) => {
                j.message = String::new();
                j.error = Some(e.clone());
            }
        }
    });
    res
}

/// 앱이 설치한 실행 환경을 지운다.
pub fn remove(kind: &str) -> Result<(), String> {
    if job(kind).running {
        return Err("설치 중에는 지울 수 없습니다.".into());
    }
    let dirs: Vec<PathBuf> = match kind {
        "python" => vec![pyenv_dir(), python_install_dir(), mpl_dir()],
        "node" => vec![node_dir()],
        "uv" => vec![uv_dir()],
        _ => return Err(format!("알 수 없는 실행 환경입니다: {}", kind)),
    };
    for d in dirs.into_iter().filter(|d| d.exists()) {
        std::fs::remove_dir_all(&d).map_err(|e| format!("지우지 못했습니다 (사용 중인 프로그램을 닫고 다시 시도하세요): {}", e))?;
    }
    if kind == "python" {
        crate::python_runner::forget_cached_python();
    }
    set_job(kind, |j| *j = Job::default());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (name, data) in files {
                z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn extracts_with_and_without_top_folder() {
        let dir = tempfile::tempdir().unwrap();
        let z = zip_of(&[("node-v1-win-x64/node.exe", b"n"), ("node-v1-win-x64/node_modules/npm/a.js", b"a"), ("../evil", b"x")]);
        extract_zip(&z, &dir.path().join("n"), Some("node-v1-win-x64")).unwrap();
        assert!(dir.path().join("n/node.exe").is_file());
        assert!(dir.path().join("n/node_modules/npm/a.js").is_file());
        assert!(!dir.path().join("evil").exists());

        let z = zip_of(&[("uv.exe", b"u"), ("uvx.exe", b"x")]);
        extract_zip(&z, &dir.path().join("u"), None).unwrap();
        assert!(dir.path().join("u/uvx.exe").is_file());
    }

    #[test]
    fn replaces_directory_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let (tmp, target) = (dir.path().join("t.tmp"), dir.path().join("t"));
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("old"), "o").unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("new"), "n").unwrap();
        replace_dir(&tmp, &target).unwrap();
        assert!(target.join("new").is_file() && !target.join("old").exists() && !tmp.exists());
    }

    #[test]
    fn pinned_hashes_look_valid() {
        for h in [UV_SHA256, NODE_SHA256] {
            assert_eq!(h.len(), 64);
            assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        }
        assert!(NODE_URL.contains(NODE_VERSION) && UV_URL.contains(UV_VERSION));
    }

    #[tokio::test]
    async fn rejects_unknown_kind_and_bad_download() {
        assert!(install("ruby").await.is_err());
        // 받을 수 없는 주소 → 오류 (설치하지 않음)
        assert!(download("t", "테스트", "https://127.0.0.1:1/x.zip", UV_SHA256).await.is_err());
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// 공식 배포본이 고정한 주소·해시와 맞는지 (내려받아 확인만, 실행하지 않음): cargo test --lib live_runtime -- --ignored
    #[tokio::test]
    #[ignore]
    async fn live_runtime_downloads_match_pinned_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let uv = download("t", "uv", UV_URL, UV_SHA256).await.unwrap();
        extract_zip(&uv, &dir.path().join("uv"), None).unwrap();
        assert!(dir.path().join("uv").join(exe("uv")).is_file() && dir.path().join("uv").join(exe("uvx")).is_file());
        let node = download("t", "Node.js", NODE_URL, NODE_SHA256).await.unwrap();
        extract_zip(&node, &dir.path().join("node"), Some(&format!("node-v{}-win-x64", NODE_VERSION))).unwrap();
        for f in ["node.exe", "npx.cmd", "npm.cmd"] {
            assert!(dir.path().join("node").join(f).is_file(), "{}", f);
        }
        eprintln!("uv {}MB, node {}MB OK", uv.len() / 1_048_576, node.len() / 1_048_576);
    }
}

#[cfg(test)]
mod live_install_tests {
    use super::*;

    /// 실제로 uv·Python·패키지를 임시 폴더에 설치해 본다 (몇 분 걸림): cargo test --lib live_python_install -- --ignored
    #[tokio::test]
    #[ignore]
    async fn live_python_install() {
        let dir = tempfile::tempdir().unwrap();
        set_root(dir.path().join("runtimes"));
        install("python").await.unwrap();
        assert!(python_installed());
        let out = tokio::process::Command::new(pyenv_python())
            .args(["-c", "import openpyxl, docx, pptx, pypdf, matplotlib, pandas, PIL, sys; print(sys.version.split()[0])"])
            .output()
            .await
            .unwrap();
        eprintln!("python: {} {}", String::from_utf8_lossy(&out.stdout).trim(), String::from_utf8_lossy(&out.stderr).trim());
        assert!(out.status.success());
        let info = crate::python_runner::find_python(None).await.unwrap();
        assert!(Path::new(&info.path).starts_with(pyenv_dir()), "앱 전용 Python을 먼저 쓴다: {}", info.path);
        eprintln!("size: {}MB", dir_size(&root()) / 1_048_576);
    }

    fn dir_size(p: &Path) -> u64 {
        std::fs::read_dir(p)
            .map(|rd| rd.flatten().map(|e| if e.path().is_dir() { dir_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum())
            .unwrap_or(0)
    }
}
