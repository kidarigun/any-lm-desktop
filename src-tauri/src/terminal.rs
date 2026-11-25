//! 오른쪽 작업 패널의 터미널 (PTY, 결정 0008).
//!
//! - Windows ConPTY(`portable-pty`)로 PowerShell(기본)·PowerShell 7·명령 프롬프트를 띄운다.
//! - 출력은 UTF-8이 덩어리 경계에서 깨지지 않게 이어 붙이고, 16ms 정도 모아 `anylm-terminal-output`으로 보낸다.
//! - 세션마다 최근 출력 256KB를 링 버퍼에 둔다. 패널을 닫았다 열면 `terminal_snapshot`으로 화면을 복원한다.
//! - 닫기·앱 종료 때 프로세스 트리 전체를 끝낸다: `taskkill /T /F`로 트리를, Job Object로 부모가 먼저 끝난
//!   고아 프로세스까지(앱이 비정상 종료돼도 Job 핸들이 닫히면 함께 끝난다).
//! - `kind: "server"`와 `command`는 T2(개발 서버)가 쓴다: 명령 하나를 PTY에서 실행하고, 끝나면 세션도 끝난다.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 세션마다 보관하는 최근 출력 (원본 그대로, 화면 복원용)
pub const TERMINAL_BUFFER_BYTES: usize = 256 * 1024;
/// 동시에 열 수 있는 터미널 세션 수
pub const TERMINAL_MAX_SESSIONS: usize = 8;
/// 출력을 모아 보내는 간격
pub const OUTPUT_BATCH: Duration = Duration::from_millis(16);
/// 한 번에 보내는 출력의 최대 크기 (넘으면 기다리지 않고 바로 보낸다)
const OUTPUT_BATCH_MAX_BYTES: usize = 64 * 1024;

pub const OUTPUT_EVENT: &str = "anylm-terminal-output";
pub const EXIT_EVENT: &str = "anylm-terminal-exit";

/// ConPTY는 시작할 때 커서 위치를 묻고(DSR) 답을 받을 때까지(최대 3초) 화면을 그리지 않는다.
/// 화면(xterm)이 아직 없을 수도 있으므로(T3: 패널을 닫은 채 서버 실행) 백엔드가 처음 한 번 답한다.
const DSR_REQUEST: &str = "\x1b[6n";
const DSR_REPLY: &[u8] = b"\x1b[1;1R";
/// 시작 DSR을 찾는 범위 (처음 출력 몇 바이트 안에서만)
const DSR_WINDOW_BYTES: usize = 1024;

// ---------------------------------------------------------------------------
// 자료형
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalKind {
    Shell,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// Windows PowerShell (기본)
    PowerShell,
    /// PowerShell 7 (PATH에 있을 때만)
    Pwsh,
    /// 명령 프롬프트
    Cmd,
}

impl Shell {
    pub fn id(self) -> &'static str {
        match self {
            Shell::PowerShell => "powershell",
            Shell::Pwsh => "pwsh",
            Shell::Cmd => "cmd",
        }
    }

    pub fn from_id(id: &str) -> Shell {
        match id.trim().to_ascii_lowercase().as_str() {
            "pwsh" => Shell::Pwsh,
            "cmd" => Shell::Cmd,
            _ => Shell::PowerShell,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Shell::PowerShell => "PowerShell",
            Shell::Pwsh => "PowerShell 7",
            Shell::Cmd => "명령 프롬프트",
        }
    }

    fn exe(self) -> &'static str {
        match self {
            Shell::PowerShell => "powershell.exe",
            Shell::Pwsh => "pwsh.exe",
            Shell::Cmd => "cmd.exe",
        }
    }
}

/// 셸 실행 인자. `command`가 있으면 그 명령 하나를 실행하고 끝난다(T2 서버 세션).
pub fn shell_argv(shell: Shell, command: Option<&str>) -> Vec<String> {
    let mut argv = vec![shell.exe().to_string()];
    let command = command.map(str::trim).filter(|c| !c.is_empty());
    match shell {
        Shell::PowerShell | Shell::Pwsh => {
            argv.push("-NoLogo".into());
            if let Some(c) = command {
                argv.push("-Command".into());
                argv.push(c.into());
            }
        }
        Shell::Cmd => {
            if let Some(c) = command {
                argv.push("/c".into());
                argv.push(c.into());
            }
        }
    }
    argv
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellOption {
    pub id: String,
    pub label: String,
}

/// 설정에서 고를 수 있는 셸. PowerShell 7은 PATH에 있을 때만 보인다.
pub fn available_shells(path: Option<&OsStr>) -> Vec<ShellOption> {
    let mut out = vec![Shell::PowerShell];
    if find_on_path("pwsh.exe", path).is_some() {
        out.push(Shell::Pwsh);
    }
    out.push(Shell::Cmd);
    out.into_iter().map(|s| ShellOption { id: s.id().into(), label: s.label().into() }).collect()
}

/// 설정 값을 실제로 쓸 셸로 바꾼다: PowerShell 7이 없으면 Windows PowerShell.
pub fn effective_shell(setting: &str, path: Option<&OsStr>) -> Shell {
    match Shell::from_id(setting) {
        Shell::Pwsh if find_on_path("pwsh.exe", path).is_none() => Shell::PowerShell,
        s => s,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    /// 작업 폴더. 비어 있으면 사용자 홈.
    #[serde(default)]
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub kind: TerminalKind,
    /// 실행할 명령 하나 (T2 서버). 없으면 대화형 셸.
    #[serde(default)]
    pub command: Option<String>,
    /// 탭 제목 (없으면 셸 이름)
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub kind: TerminalKind,
    pub title: String,
    pub cwd: String,
    pub command: Option<String>,
    pub shell: String,
    pub pid: Option<u32>,
    pub running: bool,
    pub exit_code: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshot {
    /// 링 버퍼에 남은 최근 출력 (ANSI 포함, 그대로 화면에 쓰면 된다)
    pub data: String,
    /// 이 스냅샷에 들어 있는 마지막 출력 이벤트 번호. 화면은 이보다 큰 `seq`의 이벤트만 이어 쓴다.
    pub seq: u64,
    pub running: bool,
    pub exit_code: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputPayload {
    pub id: String,
    pub data: String,
    /// 세션 안에서 1부터 늘어나는 출력 번호 (스냅샷과 겹치는 이벤트를 화면이 버리게)
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExitPayload {
    pub id: String,
    pub code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerminalEvent {
    Output(OutputPayload),
    Exit(ExitPayload),
}

/// 출력·종료 이벤트를 받는 곳 (앱은 Tauri 이벤트로, 테스트는 메모리로)
pub type EventSink = Arc<dyn Fn(TerminalEvent) + Send + Sync>;

// ---------------------------------------------------------------------------
// 순수 도우미 (단위 테스트)
// ---------------------------------------------------------------------------

/// 최근 출력 링 버퍼. 꽉 차면 오래된 바이트부터 버린다.
#[derive(Debug)]
pub struct RingBuffer {
    data: VecDeque<u8>,
    cap: usize,
    /// 오래된 바이트를 버린 적이 있는지 (앞부분이 줄·제어 순서 중간에서 시작할 수 있다)
    wrapped: bool,
}

impl RingBuffer {
    pub fn new(cap: usize) -> Self {
        Self { data: VecDeque::with_capacity(cap.min(64 * 1024)), cap, wrapped: false }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        if bytes.len() >= self.cap {
            self.wrapped = self.wrapped || bytes.len() > self.cap || !self.data.is_empty();
            self.data.clear();
            self.data.extend(&bytes[bytes.len() - self.cap..]);
            return;
        }
        let overflow = (self.data.len() + bytes.len()).saturating_sub(self.cap);
        if overflow > 0 {
            self.wrapped = true;
        }
        self.data.drain(..overflow);
        self.data.extend(bytes);
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 버퍼 내용을 글자로. 앞쪽을 버린 적이 있으면 제어 순서·글자 중간에서 시작하지 않도록
    /// 첫 줄바꿈 다음부터 쓴다(줄바꿈이 없으면 잘린 글자 조각만 버린다).
    pub fn text(&self) -> String {
        let (a, b) = self.data.as_slices();
        let mut bytes = Vec::with_capacity(self.data.len());
        bytes.extend_from_slice(a);
        bytes.extend_from_slice(b);
        let start = if self.wrapped {
            match bytes.iter().position(|b| *b == b'\n') {
                Some(nl) => nl + 1,
                None => bytes.iter().take(3).take_while(|b| (**b & 0xC0) == 0x80).count(),
            }
        } else {
            0
        };
        String::from_utf8_lossy(&bytes[start..]).into_owned()
    }
}

/// 읽은 바이트 덩어리를 UTF-8 글자로 이어 붙인다. 덩어리 끝에서 잘린 글자는 다음 덩어리와 합친다.
#[derive(Debug, Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        let mut rest: &[u8] = &self.pending;
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    out.push_str(s);
                    rest = &[];
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // valid_up_to까지는 올바른 UTF-8이다.
                    out.push_str(std::str::from_utf8(&rest[..valid]).unwrap_or_default());
                    match e.error_len() {
                        // 잘못된 바이트: 대체 문자로 바꾸고 계속
                        Some(n) => {
                            out.push('\u{FFFD}');
                            rest = &rest[valid + n..];
                        }
                        // 끝에서 글자가 잘렸다: 다음 덩어리를 기다린다
                        None => {
                            rest = &rest[valid..];
                            break;
                        }
                    }
                }
            }
        }
        self.pending = rest.to_vec();
        out
    }

    /// 스트림이 끝났을 때 남은 조각
    pub fn finish(&mut self) -> String {
        let s = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        s
    }
}

/// ANSI 제어 순서(CSI·OSC·그 밖의 ESC 순서)와 제어 문자를 지운다. 줄바꿈과 탭은 남긴다.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.peek().copied() {
                Some('[') => {
                    it.next();
                    // 매개변수·중간 바이트 뒤 마지막 바이트(0x40–0x7E)까지
                    for d in it.by_ref() {
                        if ('\x40'..='\x7e').contains(&d) {
                            break;
                        }
                    }
                }
                Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                    it.next();
                    // BEL 또는 ESC \ 까지 (OSC·DCS 등)
                    while let Some(d) = it.next() {
                        if d == '\x07' {
                            break;
                        }
                        if d == '\x1b' {
                            if it.peek() == Some(&'\\') {
                                it.next();
                            }
                            break;
                        }
                    }
                }
                Some(_) => {
                    // 두 글자 ESC 순서 (ESC ( B 처럼 중간 바이트가 있으면 하나 더)
                    if let Some(d) = it.next() {
                        if ('\x20'..='\x2f').contains(&d) {
                            it.next();
                        }
                    }
                }
                None => {}
            },
            '\u{9b}' => {
                for d in it.by_ref() {
                    if ('\x40'..='\x7e').contains(&d) {
                        break;
                    }
                }
            }
            '\n' | '\t' | '\r' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// 터미널 출력을 도구가 읽을 글자로: ANSI를 지우고, 줄마다 마지막 `\r` 뒤만 남긴다(진행 표시가 덮어쓴 줄).
pub fn plain_text(raw: &str) -> String {
    let stripped = strip_ansi(raw);
    let normalized = stripped.replace("\r\n", "\n");
    normalized
        .split('\n')
        .map(|line| line.rsplit('\r').find(|seg| !seg.is_empty()).unwrap_or("").trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 앱이 설치한 실행 환경 폴더를 PATH 앞에 붙인다(이미 들어 있으면 빼고 앞으로).
pub fn prefixed_path(dirs: &[PathBuf], base: Option<&OsStr>) -> OsString {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let mut all: Vec<PathBuf> = dirs.to_vec();
    if let Some(b) = base {
        for p in std::env::split_paths(b) {
            if p.as_os_str().is_empty() {
                continue;
            }
            if !dirs.iter().any(|d| norm(d) == norm(&p)) {
                all.push(p);
            }
        }
    }
    std::env::join_paths(all).unwrap_or_else(|_| base.map(OsStr::to_os_string).unwrap_or_default())
}

/// PATH에서 실행 파일을 찾는다.
pub fn find_on_path(exe: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?).map(|d| d.join(exe)).find(|p| p.is_file())
}

/// 작업 폴더 확인: 비어 있으면 사용자 홈, 없거나 폴더가 아니면 거절.
pub fn resolve_cwd(cwd: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let cwd = cwd.trim();
    let dir = if cwd.is_empty() {
        home.map(Path::to_path_buf).ok_or_else(|| "사용자 홈 폴더를 찾을 수 없습니다.".to_string())?
    } else {
        PathBuf::from(cwd)
    };
    if !dir.is_absolute() {
        return Err(format!("작업 폴더는 전체 경로여야 합니다: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("작업 폴더를 찾을 수 없습니다: {}", dir.display()));
    }
    Ok(dir)
}

/// 동시 세션 한도 확인. `running`은 아직 돌고 있는 세션 수(끝나서 탭만 남은 세션은 세지 않는다).
pub fn check_capacity(running: usize) -> Result<(), String> {
    if running >= TERMINAL_MAX_SESSIONS {
        return Err(format!(
            "터미널은 최대 {}개까지 열 수 있습니다. 쓰지 않는 터미널 탭을 닫은 뒤 다시 시도하세요.",
            TERMINAL_MAX_SESSIONS
        ));
    }
    Ok(())
}

fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(2, 1000), rows.clamp(1, 500))
}

// ---------------------------------------------------------------------------
// 프로세스 트리 종료
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// 터미널 세션의 프로세스들을 묶는 Job Object. 핸들이 닫히면(앱이 비정상 종료돼도) 안의 프로세스가 모두 끝난다.
    pub struct Job(HANDLE);

    // 커널 핸들은 스레드 사이에서 함께 써도 된다.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn new() -> Option<Job> {
            unsafe {
                let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if h.is_null() {
                    return None;
                }
                let job = Job(h);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    h,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    return None;
                }
                Some(job)
            }
        }

        /// 프로세스 핸들을 Job에 넣는다. 이후 그 프로세스가 만드는 자식도 모두 Job에 들어간다.
        pub fn assign(&self, process: HANDLE) -> bool {
            unsafe { AssignProcessToJobObject(self.0, process) != 0 }
        }

        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }

        /// Job 안에서 살아 있는 프로세스 ID들
        pub fn pids(&self) -> Vec<u32> {
            const MAX: usize = 1024;
            /// JOBOBJECT_BASIC_PROCESS_ID_LIST에 ID 자리를 넉넉히 붙인 모양
            #[repr(C)]
            struct PidList {
                assigned: u32,
                in_list: u32,
                ids: [usize; MAX],
            }
            let mut list = Box::new(PidList { assigned: 0, in_list: 0, ids: [0; MAX] });
            let ok = unsafe {
                QueryInformationJobObject(
                    self.0,
                    JobObjectBasicProcessIdList,
                    &mut *list as *mut PidList as *mut c_void,
                    std::mem::size_of::<PidList>() as u32,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Vec::new();
            }
            let _ = list.assigned;
            list.ids[..(list.in_list as usize).min(MAX)].iter().map(|p| *p as u32).collect()
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod job {
    pub struct Job;
    impl Job {
        pub fn new() -> Option<Job> {
            None
        }
        pub fn terminate(&self) {}
        pub fn pids(&self) -> Vec<u32> {
            Vec::new()
        }
    }
}

/// `taskkill /PID <pid> /T /F`: 아직 살아 있는 부모부터 아래 트리 전체를 끝낸다.
fn taskkill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
    }
}

// ---------------------------------------------------------------------------
// 세션
// ---------------------------------------------------------------------------

struct OutputLog {
    ring: RingBuffer,
    seq: u64,
}

struct Session {
    order: u64,
    info: Mutex<TerminalInfo>,
    /// 입력은 세션마다 둔 쓰기 스레드로 보낸다(큰 붙여넣기가 막혀도 화면 스레드는 멈추지 않고, 순서는 그대로).
    input: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    killer: Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
    output: Mutex<OutputLog>,
    job: Option<job::Job>,
}

impl Session {
    fn write(&self, data: &[u8]) -> Result<(), String> {
        let input = self.input.lock().map_err(|e| e.to_string())?;
        let tx = input.as_ref().ok_or_else(|| "터미널이 종료되었습니다.".to_string())?;
        tx.send(data.to_vec()).map_err(|_| "터미널이 종료되었습니다.".to_string())
    }

    fn running(&self) -> bool {
        self.info.lock().map(|i| i.running).unwrap_or(false)
    }

    /// 입력 스레드를 끝내고 PTY를 닫는다(남은 출력이 다 나오고 읽기 스레드가 끝난다).
    fn close_io(&self) {
        if let Ok(mut i) = self.input.lock() {
            i.take();
        }
        if let Ok(mut m) = self.master.lock() {
            m.take();
        }
    }

    /// 프로세스 트리 전체를 끝낸다.
    fn kill_tree(&self) {
        let (pid, running) = self.info.lock().map(|i| (i.pid, i.running)).unwrap_or((None, false));
        if running {
            if let Some(pid) = pid {
                taskkill_tree(pid);
            }
        }
        // 부모가 먼저 끝나 트리에서 떨어진 고아 프로세스까지
        if let Some(job) = &self.job {
            job.terminate();
        }
        if let Ok(mut k) = self.killer.lock() {
            let _ = k.kill();
        }
    }
}

pub struct TerminalManager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    sink: EventSink,
    counter: AtomicU64,
}

impl TerminalManager {
    pub fn new(sink: EventSink) -> Self {
        Self { sessions: Mutex::new(HashMap::new()), sink, counter: AtomicU64::new(0) }
    }

    fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .map_err(|e| e.to_string())?
            .get(id)
            .cloned()
            .ok_or_else(|| "터미널 세션을 찾을 수 없습니다.".to_string())
    }

    /// 세션을 연다. `cwd`는 이미 확인한 폴더, `path_dirs`는 PATH 앞에 붙일 폴더(앱 실행 환경).
    pub fn open(
        &self,
        req: &OpenRequest,
        cwd: &Path,
        shell: Shell,
        path_dirs: &[PathBuf],
        extra_env: &[(String, String)],
    ) -> Result<TerminalInfo, String> {
        self.ensure_capacity()?;
        let (cols, rows) = clamp_size(req.cols, req.rows);
        let pty = portable_pty::native_pty_system();
        let pair = pty
            .openpty(portable_pty::PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("터미널을 만들 수 없습니다: {e}"))?;

        let command = req.command.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
        let argv = shell_argv(shell, command.as_deref());
        let mut cmd = portable_pty::CommandBuilder::from_argv(argv.iter().map(OsString::from).collect());
        // 정규 경로(`\\?\C:\...`)를 그대로 주면 cmd가 C:\Windows에서 돈다 (fs_ops::process_path).
        let cwd = crate::fs_ops::process_path(cwd);
        cmd.cwd(&cwd);
        for (k, v) in extra_env {
            if !k.eq_ignore_ascii_case("PATH") {
                cmd.env(k, v);
            }
        }
        let path = prefixed_path(path_dirs, cmd.get_env("PATH"));
        cmd.env("PATH", path);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("{}을(를) 실행할 수 없습니다: {e}", shell.label()))?;
        // 자식이 PTY를 넘겨받았으니 우리 쪽 slave는 닫는다(그래야 종료 때 출력이 끝난다).
        drop(pair.slave);

        let job = job::Job::new();
        #[cfg(windows)]
        {
            if let (Some(j), Some(h)) = (&job, child.as_raw_handle()) {
                if !j.assign(h as _) {
                    eprintln!("터미널 프로세스를 Job Object에 넣지 못했습니다 (닫을 때 taskkill로 정리합니다).");
                }
            }
        }

        // 여기서 실패하면 방금 띄운 셸(과 Job)을 끝내고 돌려준다.
        let mut fail = |msg: String| {
            if let Some(pid) = child.process_id() {
                taskkill_tree(pid);
            }
            if let Some(j) = &job {
                j.terminate();
            }
            let _ = child.kill();
            msg
        };
        let reader = match pair.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => return Err(fail(format!("터미널 출력을 읽을 수 없습니다: {e}"))),
        };
        let writer = match pair.master.take_writer() {
            Ok(w) => w,
            Err(e) => return Err(fail(format!("터미널 입력을 열 수 없습니다: {e}"))),
        };
        let killer = child.clone_killer();
        let id = uuid::Uuid::new_v4().to_string();
        let title = req
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .or_else(|| command.clone())
            .unwrap_or_else(|| shell.label().to_string());
        let info = TerminalInfo {
            id: id.clone(),
            kind: req.kind,
            title,
            cwd: cwd.to_string_lossy().to_string(),
            command,
            shell: shell.id().to_string(),
            pid: child.process_id(),
            running: true,
            exit_code: None,
        };
        let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>();
        let session = Arc::new(Session {
            order: self.counter.fetch_add(1, Ordering::SeqCst),
            info: Mutex::new(info.clone()),
            input: Mutex::new(Some(input_tx)),
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(killer),
            output: Mutex::new(OutputLog { ring: RingBuffer::new(TERMINAL_BUFFER_BYTES), seq: 0 }),
            job,
        });
        // 스레드를 하나라도 못 띄우면 세션을 지도에 넣지 않고 프로세스 트리를 끝낸다.
        let abort = |e: std::io::Error| {
            session.kill_tree();
            session.close_io();
            format!("터미널을 시작할 수 없습니다: {e}")
        };

        // 0) 쓰기: 입력을 순서대로 PTY에
        spawn_writer(writer, input_rx).map_err(abort)?;
        // 1) 읽기: 바이트 → UTF-8 글자
        let (tx, rx) = mpsc::channel::<String>();
        spawn_reader(reader, tx).map_err(abort)?;
        // 2) 모아 보내기: 16ms마다 링 버퍼에 넣고 이벤트로
        let batcher = {
            let session = session.clone();
            let sink = self.sink.clone();
            let id = id.clone();
            std::thread::Builder::new()
                .name("terminal-batch".into())
                .spawn(move || run_batcher(&id, &session, &sink, rx))
                .map_err(abort)?
        };
        // 3) 종료 기다리기: 셸이 끝나면 PTY를 닫아 남은 출력을 다 보낸 뒤 종료 이벤트
        {
            let session = session.clone();
            let sink = self.sink.clone();
            let id = id.clone();
            std::thread::Builder::new()
                .name("terminal-wait".into())
                .spawn(move || {
                    let code = child.wait().ok().map(|s| s.exit_code());
                    if let Ok(mut i) = session.info.lock() {
                        i.running = false;
                        i.exit_code = code;
                    }
                    // ConPTY는 자식이 끝나도 PTY를 닫기 전까지 출력 파이프가 열려 있다.
                    session.close_io();
                    let _ = batcher.join();
                    sink(TerminalEvent::Exit(ExitPayload { id, code }));
                })
                .map_err(abort)?;
        }

        // 다 띄운 뒤에 지도에 넣는다(그 사이 다른 세션이 열려 한도를 넘었으면 이것을 거절).
        let mut sessions = self.sessions.lock().map_err(|e| e.to_string())?;
        let running = sessions.values().filter(|s| s.running()).count();
        if let Err(e) = check_capacity(running) {
            drop(sessions);
            session.kill_tree();
            session.close_io();
            return Err(e);
        }
        sessions.insert(id, session);
        Ok(info)
    }

    /// 아직 돌고 있는 세션만 한도에 센다.
    fn ensure_capacity(&self) -> Result<(), String> {
        let running = self.sessions.lock().map_err(|e| e.to_string())?.values().filter(|s| s.running()).count();
        check_capacity(running)
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        self.get(id)?.write(data.as_bytes())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let s = self.get(id)?;
        let (cols, rows) = clamp_size(cols, rows);
        let master = s.master.lock().map_err(|e| e.to_string())?;
        match master.as_ref() {
            Some(m) => m
                .resize(portable_pty::PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                .map_err(|e| format!("터미널 크기를 바꿀 수 없습니다: {e}")),
            None => Ok(()),
        }
    }

    /// 세션을 닫고 프로세스 트리 전체를 끝낸다.
    pub fn close(&self, id: &str) -> Result<(), String> {
        let removed = self.sessions.lock().map_err(|e| e.to_string())?.remove(id);
        if let Some(s) = removed {
            s.kill_tree();
        }
        Ok(())
    }

    /// 앱을 끝낼 때: 모든 세션의 프로세스 트리를 끝낸다(동시에).
    pub fn close_all(&self) {
        let all: Vec<Arc<Session>> = match self.sessions.lock() {
            Ok(mut m) => m.drain().map(|(_, s)| s).collect(),
            Err(_) => return,
        };
        let handles: Vec<_> = all.into_iter().map(|s| std::thread::spawn(move || s.kill_tree())).collect();
        for h in handles {
            let _ = h.join();
        }
    }

    pub fn list(&self) -> Vec<TerminalInfo> {
        let sessions: Vec<Arc<Session>> = match self.sessions.lock() {
            Ok(m) => m.values().cloned().collect(),
            Err(_) => return Vec::new(),
        };
        let mut v: Vec<(u64, TerminalInfo)> =
            sessions.iter().filter_map(|s| s.info.lock().ok().map(|i| (s.order, i.clone()))).collect();
        v.sort_by_key(|(o, _)| *o);
        v.into_iter().map(|(_, i)| i).collect()
    }

    pub fn snapshot(&self, id: &str) -> Result<TerminalSnapshot, String> {
        let s = self.get(id)?;
        let (running, exit_code) = s.info.lock().map(|i| (i.running, i.exit_code)).map_err(|e| e.to_string())?;
        let out = s.output.lock().map_err(|e| e.to_string())?;
        Ok(TerminalSnapshot { data: out.ring.text(), seq: out.seq, running, exit_code })
    }

    /// 도구용: 최근 출력을 ANSI 없는 글자로 (T3 `server_logs`가 쓴다)
    pub fn text(&self, id: &str) -> Result<String, String> {
        let s = self.get(id)?;
        let out = s.output.lock().map_err(|e| e.to_string())?;
        Ok(plain_text(&out.ring.text()))
    }

    /// 셸 말고 다른 프로그램이 돌고 있는지 (탭을 닫기 전에 물어볼지 정한다). Job Object 안의 프로세스로 판단한다.
    pub fn busy(&self, id: &str) -> Result<bool, String> {
        let s = self.get(id)?;
        let (pid, running) = s.info.lock().map(|i| (i.pid, i.running)).map_err(|e| e.to_string())?;
        if !running {
            return Ok(false);
        }
        Ok(s.job.as_ref().map(|j| j.pids().into_iter().any(|p| Some(p) != pid)).unwrap_or(false))
    }

    /// 테스트용: 세션의 Job Object 안 프로세스 ID들
    #[cfg(test)]
    fn job_pids(&self, id: &str) -> Vec<u32> {
        self.get(id).ok().and_then(|s| s.job.as_ref().map(|j| j.pids())).unwrap_or_default()
    }

    #[cfg(test)]
    fn running(&self, id: &str) -> bool {
        self.get(id).ok().and_then(|s| s.info.lock().ok().map(|i| i.running)).unwrap_or(false)
    }
}

fn spawn_writer(mut writer: Box<dyn Write + Send>, rx: mpsc::Receiver<Vec<u8>>) -> std::io::Result<()> {
    std::thread::Builder::new().name("terminal-write".into()).spawn(move || {
        // 보낸 쪽이 모두 닫히면(세션 종료) 끝나고, 그때 쓰기 핸들도 닫힌다.
        while let Ok(data) = rx.recv() {
            if writer.write_all(&data).and_then(|_| writer.flush()).is_err() {
                break;
            }
        }
    })?;
    Ok(())
}

fn spawn_reader(mut reader: Box<dyn Read + Send>, tx: mpsc::Sender<String>) -> std::io::Result<()> {
    std::thread::Builder::new().name("terminal-read".into()).spawn(move || {
        let mut decoder = Utf8Decoder::default();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = decoder.push(&buf[..n]);
                    if !s.is_empty() && tx.send(s).is_err() {
                        return;
                    }
                }
            }
        }
        let rest = decoder.finish();
        if !rest.is_empty() {
            let _ = tx.send(rest);
        }
    })?;
    Ok(())
}

fn run_batcher(id: &str, session: &Session, sink: &EventSink, rx: mpsc::Receiver<String>) {
    let mut seen_bytes = 0usize;
    let mut dsr_answered = false;
    while let Ok(first) = rx.recv() {
        let mut batch = first;
        let deadline = Instant::now() + OUTPUT_BATCH;
        let mut disconnected = false;
        while batch.len() < OUTPUT_BATCH_MAX_BYTES {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match rx.recv_timeout(deadline - now) {
                Ok(more) => batch.push_str(&more),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if !dsr_answered && seen_bytes < DSR_WINDOW_BYTES {
            if let Some(pos) = batch.find(DSR_REQUEST) {
                batch.replace_range(pos..pos + DSR_REQUEST.len(), "");
                let _ = session.write(DSR_REPLY);
                dsr_answered = true;
            }
        }
        seen_bytes += batch.len();
        if !batch.is_empty() {
            emit_output(id, session, sink, batch);
        }
        if disconnected {
            break;
        }
    }
}

/// 링 버퍼에 넣고 이벤트를 보내는 일을 한 잠금 안에서 한다(스냅샷과 이벤트 번호가 어긋나지 않게).
fn emit_output(id: &str, session: &Session, sink: &EventSink, data: String) {
    if let Ok(mut out) = session.output.lock() {
        out.ring.push(data.as_bytes());
        out.seq += 1;
        sink(TerminalEvent::Output(OutputPayload { id: id.to_string(), data, seq: out.seq }));
    }
}

// ---------------------------------------------------------------------------
// Tauri 명령
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedTerminal {
    pub id: String,
    pub info: TerminalInfo,
}

/// PTY 만들기·프로세스 실행은 수십 ms가 걸릴 수 있어 화면 스레드를 막지 않게 async로 둔다.
#[tauri::command]
pub async fn terminal_open<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    manager: tauri::State<'_, TerminalManager>,
    state: tauri::State<'_, crate::AppState>,
    request: OpenRequest,
) -> Result<OpenedTerminal, String> {
    use tauri::Manager;
    let home = app.path().home_dir().ok();
    let cwd = resolve_cwd(&request.cwd, home.as_deref())?;
    let setting = state.db.lock().map_err(|e| e.to_string())?.settings.terminal_shell.clone();
    let shell = effective_shell(&setting, std::env::var_os("PATH").as_deref());
    let env: Vec<(String, String)> = crate::runtime::child_env().into_iter().filter(|(k, _)| k != "PATH").collect();
    let info = manager.open(&request, &cwd, shell, &crate::runtime::path_dirs(), &env)?;
    Ok(OpenedTerminal { id: info.id.clone(), info })
}

/// 동기 명령(화면 스레드)으로 두어 키 입력 순서를 지킨다. 실제 쓰기는 세션의 쓰기 스레드가 하므로 막히지 않는다.
#[tauri::command]
pub fn terminal_write(manager: tauri::State<'_, TerminalManager>, id: String, data: String) -> Result<(), String> {
    manager.write(&id, &data)
}

#[tauri::command]
pub fn terminal_resize(manager: tauri::State<'_, TerminalManager>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    manager.resize(&id, cols, rows)
}

/// taskkill을 기다리므로 async (화면 스레드를 막지 않게)
#[tauri::command]
pub async fn terminal_close(manager: tauri::State<'_, TerminalManager>, id: String) -> Result<(), String> {
    manager.close(&id)
}

#[tauri::command]
pub fn terminal_list(manager: tauri::State<'_, TerminalManager>) -> Vec<TerminalInfo> {
    manager.list()
}

/// 최대 256KB를 글자로 만들어 보내므로 화면 스레드 밖(async)에서
#[tauri::command]
pub async fn terminal_snapshot(manager: tauri::State<'_, TerminalManager>, id: String) -> Result<TerminalSnapshot, String> {
    manager.snapshot(&id)
}

#[tauri::command]
pub async fn terminal_busy(manager: tauri::State<'_, TerminalManager>, id: String) -> Result<bool, String> {
    manager.busy(&id)
}

#[tauri::command]
pub fn terminal_shells() -> Vec<ShellOption> {
    available_shells(std::env::var_os("PATH").as_deref())
}

/// 앱의 Tauri 이벤트로 보내는 출력 받는 곳
pub fn tauri_sink<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> EventSink {
    use tauri::Emitter;
    Arc::new(move |ev| match ev {
        TerminalEvent::Output(p) => {
            let _ = app.emit(OUTPUT_EVENT, p);
        }
        TerminalEvent::Exit(p) => {
            let _ = app.emit(EXIT_EVENT, p);
        }
    })
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
