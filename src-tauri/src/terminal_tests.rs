use super::*;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// 순수 도우미
// ---------------------------------------------------------------------------

#[test]
fn ring_buffer_keeps_only_the_newest_bytes() {
    let mut r = RingBuffer::new(8);
    r.push(b"abcdef");
    assert_eq!(r.text(), "abcdef");
    r.push(b"ghij");
    assert_eq!(r.len(), 8);
    assert_eq!(r.text(), "cdefghij");
    // 한 번에 용량보다 많이 넣으면 끝부분만 남는다.
    r.push(b"0123456789");
    assert_eq!(r.text(), "23456789");
    assert!(!r.is_empty());
}

#[test]
fn ring_buffer_replay_drops_a_character_cut_by_the_wrap() {
    // "가"(EA B0 80) "나"(EB 82 98): 5바이트만 남으면 "가"의 뒤 두 바이트가 앞에 걸린다.
    let mut r = RingBuffer::new(5);
    r.push("가나".as_bytes());
    assert_eq!(r.text(), "나");
}

#[test]
fn ring_buffer_replay_after_wrap_starts_at_the_next_line() {
    // 잘린 자리가 색상 순서(ESC[38;5;196m) 한가운데: 첫 줄바꿈 다음부터 보여 준다.
    let mut r = RingBuffer::new(12);
    r.push(b"\x1b[38;5;196mred\nok\n");
    assert_eq!(r.text(), "ok\n");
    // 버린 적이 없으면 처음부터 그대로
    let mut r = RingBuffer::new(64);
    r.push(b"\x1b[31mred\x1b[0m\nok\n");
    assert_eq!(r.text(), "\x1b[31mred\x1b[0m\nok\n");
    // 나눠 넣다가 넘쳐도 같다.
    let mut r = RingBuffer::new(8);
    r.push(b"\x1b[1mab");
    r.push(b"c\nde");
    assert_eq!(r.text(), "de");
    // 정확히 가득 채운 것은 버린 것이 아니다.
    let mut r = RingBuffer::new(4);
    r.push(b"a\nbc");
    assert_eq!(r.text(), "a\nbc");
}

#[test]
fn ring_buffer_default_capacity_is_256_kib() {
    assert_eq!(TERMINAL_BUFFER_BYTES, 256 * 1024);
    let mut r = RingBuffer::new(TERMINAL_BUFFER_BYTES);
    for _ in 0..300 {
        r.push(&[b'x'; 1024]);
    }
    assert_eq!(r.len(), TERMINAL_BUFFER_BYTES);
}

#[test]
fn utf8_decoder_joins_characters_split_across_chunks() {
    let text = "한글 ok ✓ 끝";
    let bytes = text.as_bytes();
    for split in 0..=bytes.len() {
        let mut d = Utf8Decoder::default();
        let mut out = d.push(&bytes[..split]);
        out.push_str(&d.push(&bytes[split..]));
        out.push_str(&d.finish());
        assert_eq!(out, text, "split at {split}");
    }
    // 한 바이트씩 들어와도 같다.
    let mut d = Utf8Decoder::default();
    let mut out = String::new();
    for b in bytes {
        out.push_str(&d.push(&[*b]));
    }
    assert_eq!(out, text);
}

#[test]
fn utf8_decoder_replaces_invalid_bytes_and_flushes_the_tail() {
    let mut d = Utf8Decoder::default();
    assert_eq!(d.push(&[0xFF, b'a']), "\u{FFFD}a");
    // 끝에서 잘린 글자는 기다렸다가, 스트림이 끝나면 대체 문자로
    assert_eq!(d.push(&[b'b', 0xEA, 0xB0]), "b");
    assert_eq!(d.finish(), "\u{FFFD}");
    assert_eq!(d.finish(), "");
}

#[test]
fn strip_ansi_removes_escape_sequences_and_keeps_text() {
    assert_eq!(strip_ansi("\x1b[32mhi\x1b[0m there"), "hi there");
    assert_eq!(strip_ansi("\x1b[?25l\x1b[2J\x1b[1;1Hok"), "ok");
    assert_eq!(strip_ansi("\x1b]0;제목\x07본문"), "본문");
    assert_eq!(strip_ansi("\x1b]8;;http://localhost:5173\x1b\\link\x1b]8;;\x1b\\"), "link");
    assert_eq!(strip_ansi("\x1b(Bx\x1b=y"), "xy");
    assert_eq!(strip_ansi("a\tb\nc\x07\x08"), "a\tb\nc");
    assert_eq!(strip_ansi("끝 \x1b"), "끝 ");
}

#[test]
fn plain_text_keeps_the_last_overwrite_of_progress_lines() {
    assert_eq!(plain_text("10%\r50%\r100%\r\nDone\r\n"), "100%\nDone\n");
    assert_eq!(plain_text("\x1b[32m  VITE ready\x1b[0m\r\n  ➜  Local:   http://localhost:5173/"), "  VITE ready\n  ➜  Local:   http://localhost:5173/");
    // 줄 끝의 \r만 있으면 그 앞 내용을 남긴다.
    assert_eq!(plain_text("abc\r"), "abc");
}

#[test]
fn shell_argv_defaults_to_windows_powershell_without_logo() {
    assert_eq!(shell_argv(Shell::PowerShell, None), vec!["powershell.exe", "-NoLogo"]);
    assert_eq!(shell_argv(Shell::Pwsh, None), vec!["pwsh.exe", "-NoLogo"]);
    assert_eq!(shell_argv(Shell::Cmd, None), vec!["cmd.exe"]);
    assert_eq!(
        shell_argv(Shell::PowerShell, Some("npm run dev")),
        vec!["powershell.exe", "-NoLogo", "-Command", "npm run dev"]
    );
    assert_eq!(shell_argv(Shell::Cmd, Some(" npm run dev ")), vec!["cmd.exe", "/c", "npm run dev"]);
    assert_eq!(shell_argv(Shell::Cmd, Some("  ")), vec!["cmd.exe"]);
    assert_eq!(Shell::from_id("PWSH"), Shell::Pwsh);
    assert_eq!(Shell::from_id(""), Shell::PowerShell);
    assert_eq!(Shell::from_id("bash"), Shell::PowerShell);
}

#[test]
fn pwsh_is_offered_only_when_found_on_path() {
    let empty = tempfile::tempdir().unwrap();
    let path = std::env::join_paths([empty.path()]).unwrap();
    let ids: Vec<String> = available_shells(Some(&path)).into_iter().map(|s| s.id).collect();
    assert_eq!(ids, vec!["powershell", "cmd"]);
    assert_eq!(effective_shell("pwsh", Some(&path)), Shell::PowerShell);

    let with = tempfile::tempdir().unwrap();
    std::fs::write(with.path().join("pwsh.exe"), b"").unwrap();
    let path = std::env::join_paths([empty.path(), with.path()]).unwrap();
    let ids: Vec<String> = available_shells(Some(&path)).into_iter().map(|s| s.id).collect();
    assert_eq!(ids, vec!["powershell", "pwsh", "cmd"]);
    assert_eq!(effective_shell("pwsh", Some(&path)), Shell::Pwsh);
    assert_eq!(effective_shell("cmd", None), Shell::Cmd);
    assert_eq!(find_on_path("pwsh.exe", Some(&path)), Some(with.path().join("pwsh.exe")));
    assert_eq!(find_on_path("pwsh.exe", None), None);
}

#[test]
fn path_is_prefixed_with_the_app_runtime_dirs() {
    let node = PathBuf::from(r"C:\rt\node");
    let uv = PathBuf::from(r"C:\rt\uv");
    let base = OsString::from(r"C:\Windows;C:\rt\node\;;C:\Tools");
    let joined = prefixed_path(&[node.clone(), uv.clone()], Some(&base));
    let parts: Vec<PathBuf> = std::env::split_paths(&joined).collect();
    assert_eq!(parts, vec![node.clone(), uv, PathBuf::from(r"C:\Windows"), PathBuf::from(r"C:\Tools")]);
    // 붙일 폴더가 없으면 그대로
    let same = prefixed_path(&[], Some(&OsString::from(r"C:\A;C:\B")));
    assert_eq!(std::env::split_paths(&same).collect::<Vec<_>>(), vec![PathBuf::from(r"C:\A"), PathBuf::from(r"C:\B")]);
    assert_eq!(std::env::split_paths(&prefixed_path(&[node.clone()], None)).collect::<Vec<_>>(), vec![node]);
}

#[test]
fn cwd_must_be_an_existing_absolute_folder() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(resolve_cwd(&dir.path().to_string_lossy(), None).unwrap(), dir.path());
    // 비어 있으면 사용자 홈
    assert_eq!(resolve_cwd("  ", Some(dir.path())).unwrap(), dir.path());
    assert!(resolve_cwd("", None).unwrap_err().contains("홈"));
    let missing = dir.path().join("없는 폴더");
    assert!(resolve_cwd(&missing.to_string_lossy(), None).unwrap_err().contains("찾을 수 없습니다"));
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "x").unwrap();
    assert!(resolve_cwd(&file.to_string_lossy(), None).is_err());
    assert!(resolve_cwd("relative\\dir", None).unwrap_err().contains("전체 경로"));
}

#[test]
fn ninth_session_is_refused_with_a_reason() {
    assert_eq!(TERMINAL_MAX_SESSIONS, 8);
    assert!(check_capacity(0).is_ok());
    assert!(check_capacity(7).is_ok());
    let err = check_capacity(8).unwrap_err();
    assert!(err.contains("최대 8개"), "{err}");
}

// ---------------------------------------------------------------------------
// 실제 ConPTY
// ---------------------------------------------------------------------------

struct Collected {
    events: Arc<Mutex<Vec<TerminalEvent>>>,
}

impl Collected {
    fn output(&self, id: &str) -> String {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                TerminalEvent::Output(p) if p.id == id => Some(p.data.clone()),
                _ => None,
            })
            .collect()
    }
    fn exit(&self, id: &str) -> Option<Option<u32>> {
        self.events.lock().unwrap().iter().find_map(|e| match e {
            TerminalEvent::Exit(p) if p.id == id => Some(p.code),
            _ => None,
        })
    }
    fn seqs(&self, id: &str) -> Vec<u64> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                TerminalEvent::Output(p) if p.id == id => Some(p.seq),
                _ => None,
            })
            .collect()
    }
}

fn manager() -> (TerminalManager, Collected) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink_events = events.clone();
    let sink: EventSink = Arc::new(move |e| sink_events.lock().unwrap().push(e));
    (TerminalManager::new(sink), Collected { events })
}

fn open(m: &TerminalManager, shell: Shell, command: Option<&str>) -> TerminalInfo {
    let dir = std::env::temp_dir();
    let req = OpenRequest {
        cwd: dir.to_string_lossy().to_string(),
        cols: 100,
        rows: 30,
        kind: if command.is_some() { TerminalKind::Server } else { TerminalKind::Shell },
        command: command.map(str::to_string),
        title: None,
    };
    m.open(&req, &dir, shell, &[], &[]).expect("open terminal")
}

fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}

#[cfg(windows)]
fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000)
}

/// (pid, 부모 pid, 이름)
#[cfg(windows)]
fn process_table() -> Vec<(u32, u32, String)> {
    let out = no_window(std::process::Command::new("powershell.exe").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId),$($_.ParentProcessId),$($_.Name)\" }",
    ]))
    .output()
    .expect("list processes");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.trim().splitn(3, ',');
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.to_string()))
        })
        .collect()
}

#[cfg(windows)]
fn descendants(root: u32, table: &[(u32, u32, String)]) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(p) = stack.pop() {
        for (pid, ppid, name) in table {
            if *ppid == p && *pid != root && !out.iter().any(|(q, _)| q == pid) {
                out.push((*pid, name.clone()));
                stack.push(*pid);
            }
        }
    }
    out
}

/// tasklist로 프로세스가 살아 있는지
#[cfg(windows)]
fn alive(pid: u32) -> bool {
    let out = no_window(std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"]))
        .output()
        .expect("tasklist");
    String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
}

#[cfg(windows)]
#[test]
fn command_session_streams_output_then_reports_the_exit_code() {
    let (m, ev) = manager();
    let info = open(&m, Shell::Cmd, Some("echo anylm-hello-42 & exit /b 3"));
    assert_eq!(info.kind, TerminalKind::Server);
    assert_eq!(info.title, "echo anylm-hello-42 & exit /b 3");
    assert!(wait_until(Duration::from_secs(20), || ev.exit(&info.id).is_some()), "no exit event");
    assert_eq!(ev.exit(&info.id), Some(Some(3)));
    assert!(ev.output(&info.id).contains("anylm-hello-42"));
    // 시작 DSR은 백엔드가 답하고 화면으로 보내지 않는다.
    assert!(!ev.output(&info.id).contains(DSR_REQUEST));
    // 출력 번호는 1부터 하나씩 늘어난다.
    let seqs = ev.seqs(&info.id);
    assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>());
    let snap = m.snapshot(&info.id).unwrap();
    assert!(snap.data.contains("anylm-hello-42"));
    assert_eq!(snap.seq, *seqs.last().unwrap());
    assert!(!snap.running);
    assert_eq!(snap.exit_code, Some(3));
    assert!(m.text(&info.id).unwrap().contains("anylm-hello-42"));
    // 끝난 세션도 닫기 전까지 목록에 남는다(탭에 "종료됨" 표시).
    let listed = m.list();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].running);
    m.close(&info.id).unwrap();
    assert!(m.list().is_empty());
    assert!(m.snapshot(&info.id).is_err());
}

#[cfg(windows)]
#[test]
fn default_windows_powershell_runs_with_colors() {
    let (m, ev) = manager();
    let info = open(&m, Shell::PowerShell, Some("Write-Host -ForegroundColor Green anylm-ps-ok; exit 7"));
    assert_eq!(info.shell, "powershell");
    assert!(wait_until(Duration::from_secs(30), || ev.exit(&info.id).is_some()), "no exit event");
    assert_eq!(ev.exit(&info.id), Some(Some(7)));
    let out = ev.output(&info.id);
    assert!(out.contains("anylm-ps-ok"), "{out:?}");
    // 색상은 ANSI 순서로 오고, 도구용 글자에서는 빠진다.
    assert!(out.contains("\x1b["), "{out:?}");
    assert!(m.text(&info.id).unwrap().contains("anylm-ps-ok"));
    assert!(!m.text(&info.id).unwrap().contains('\x1b'));
    m.close(&info.id).unwrap();
}

#[cfg(windows)]
#[test]
fn interactive_shell_takes_input_and_resizes() {
    let (m, ev) = manager();
    let info = open(&m, Shell::Cmd, None);
    assert_eq!(info.title, "명령 프롬프트");
    assert!(info.running);
    m.resize(&info.id, 120, 40).unwrap();
    m.write(&info.id, "echo anylm-typed-%COMSPEC:~-7%\r").unwrap();
    assert!(
        wait_until(Duration::from_secs(20), || ev.output(&info.id).contains("anylm-typed-cmd.exe")),
        "output: {:?}",
        ev.output(&info.id)
    );
    assert!(m.running(&info.id));
    // 셸만 있으면 바쁘지 않다(닫을 때 묻지 않는다).
    assert!(!m.busy(&info.id).unwrap());
    // 큰 붙여넣기도 쓰기 스레드로 넘기므로 바로 돌아온다(키 입력 순서는 채널이 지킨다).
    let started = Instant::now();
    m.write(&info.id, &"rem ".repeat(64 * 1024)).unwrap();
    m.write(&info.id, "\r").unwrap();
    assert!(started.elapsed() < Duration::from_millis(500), "write blocked for {:?}", started.elapsed());
    m.write(&info.id, "exit 5\r").unwrap();
    assert!(wait_until(Duration::from_secs(20), || ev.exit(&info.id).is_some()));
    assert_eq!(ev.exit(&info.id), Some(Some(5)));
    assert!(m.write(&info.id, "x").is_err());
    m.close(&info.id).unwrap();
}

#[cfg(windows)]
#[test]
fn ninth_real_session_is_refused_and_closing_frees_a_slot() {
    let (m, _ev) = manager();
    let ids: Vec<String> = (0..TERMINAL_MAX_SESSIONS).map(|_| open(&m, Shell::Cmd, None).id).collect();
    let dir = std::env::temp_dir();
    let req = OpenRequest { cwd: String::new(), cols: 80, rows: 24, kind: TerminalKind::Shell, command: None, title: None };
    let err = m.open(&req, &dir, Shell::Cmd, &[], &[]).unwrap_err();
    assert!(err.contains("최대 8개"), "{err}");
    m.close(&ids[0]).unwrap();
    let again = m.open(&req, &dir, Shell::Cmd, &[], &[]).expect("slot freed");
    // 끝났지만 탭이 남은 세션은 세지 않는다.
    assert!(m.open(&req, &dir, Shell::Cmd, &[], &[]).is_err());
    m.write(&ids[1], "exit 0\r").unwrap();
    assert!(wait_until(Duration::from_secs(20), || !m.running(&ids[1])), "shell did not exit");
    let after_exit = m.open(&req, &dir, Shell::Cmd, &[], &[]).expect("exited tab does not count");
    assert_eq!(m.list().len(), TERMINAL_MAX_SESSIONS + 1);
    assert!(m.snapshot(&after_exit.id).is_ok());
    m.close_all();
    assert!(m.list().is_empty());
    assert!(m.snapshot(&again.id).is_err());
}

#[cfg(windows)]
#[test]
fn closing_a_session_kills_the_whole_process_tree() {
    let (m, _ev) = manager();
    let info = open(&m, Shell::Cmd, None);
    let shell_pid = info.pid.expect("shell pid");
    m.write(&info.id, "ping -n 60 127.0.0.1\r").unwrap();
    let mut ping = None;
    assert!(
        wait_until(Duration::from_secs(30), || {
            ping = descendants(shell_pid, &process_table())
                .into_iter()
                .find(|(_, n)| n.eq_ignore_ascii_case("ping.exe"))
                .map(|(p, _)| p);
            ping.is_some()
        }),
        "ping did not start under the shell"
    );
    let ping = ping.unwrap();
    // 셸이 만든 자식도 Job Object 안에 있다.
    let in_job = m.job_pids(&info.id);
    assert!(in_job.contains(&shell_pid) && in_job.contains(&ping), "job pids {in_job:?}");
    assert!(alive(ping));
    assert!(m.busy(&info.id).unwrap(), "a running child makes the tab busy");
    m.close(&info.id).unwrap();
    assert!(wait_until(Duration::from_secs(10), || !alive(ping) && !alive(shell_pid)), "processes survived close");
}

#[cfg(windows)]
#[test]
fn closing_a_session_also_kills_orphaned_grandchildren() {
    // 중간 cmd가 바로 끝나 ping의 부모가 사라진다: taskkill /T로는 닿지 않고 Job Object가 끝낸다.
    let (m, _ev) = manager();
    let info = open(&m, Shell::Cmd, None);
    m.write(&info.id, "cmd /c start /b ping -n 60 127.0.0.2\r").unwrap();
    let mut ping = None;
    assert!(
        wait_until(Duration::from_secs(30), || {
            let in_job = m.job_pids(&info.id);
            ping = process_table()
                .into_iter()
                .find(|(pid, _, n)| n.eq_ignore_ascii_case("ping.exe") && in_job.contains(pid))
                .map(|(p, _, _)| p);
            ping.is_some()
        }),
        "orphan ping not found in the job"
    );
    let ping = ping.unwrap();
    m.close(&info.id).unwrap();
    assert!(wait_until(Duration::from_secs(10), || !alive(ping)), "orphan survived close");
}
