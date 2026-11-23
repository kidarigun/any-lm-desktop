# 0008 — 오른쪽 작업 패널의 PTY 터미널

2026-11-24. 설계: `docs/superpowers/specs/2026-11-24-dev-server-preview-console.md` §2·§3(T1). T2(개발 서버·미리보기)와 T3(모델 도구 `start_server` 등)가 이 위에 올라간다.

## 왜 진짜 PTY인가
- 사용자 요청은 "콘솔 창을 오른쪽에 열고 닫기"이고, 결정은 **완전한 터미널**(PowerShell 직접 입력, 색상, 진행 표시, Ctrl+C, 여러 탭)이다. 파이프(`std::process` + stdout 읽기)로는 색상·진행 막대·대화형 프로그램(`npm init`, `python` REPL)·Ctrl+C가 제대로 되지 않는다.
- **백엔드: `portable-pty` 0.9.0**(wezterm 프로젝트, Windows에서는 ConPTY). Windows 10 1809 이상에 기본으로 있는 의사 콘솔이라 따로 배포할 것이 없다. 직접 `CreatePseudoConsole`을 부르는 것보다 검증된 크레이트를 쓴다.
- **화면: `@xterm/xterm` 6.0.0 + `@xterm/addon-fit` 0.11.0 + `@xterm/addon-web-links` 0.12.0**(정확한 버전 고정, MIT). VS Code 터미널과 같은 엔진. 기본 DOM 렌더러는 `eval`이 없어 지금 CSP(`script-src 'self'`, `style-src 'unsafe-inline'`)를 바꾸지 않는다. 작업 패널을 처음 열 때 불러온다(`React.lazy`, 첫 화면 묶음에 넣지 않음).

## 동작
- 셸: 기본 `powershell.exe -NoLogo`. 설정 `terminalShell`(`AppSettings.terminal_shell`, 기본 `"powershell"`)으로 `pwsh`/`cmd`를 고른다. PowerShell 7은 앱 프로세스의 PATH에 `pwsh.exe`가 있을 때만 목록에 보이고, 설정이 `pwsh`인데 없으면 Windows PowerShell로 연다. 패널 위 셸 선택은 새로 여는 터미널부터 적용.
- 환경: `portable-pty`는 레지스트리의 시스템+사용자 환경(새로 설치한 도구의 PATH 포함)을 기본으로 쓴다. 그 PATH 앞에 앱이 설치한 실행 환경(`runtime::path_dirs()`: Node.js, uv)을 붙이고(`terminal::prefixed_path`, 중복은 앞으로 옮김), `UV_PYTHON_INSTALL_DIR`·`UV_CACHE_DIR`도 MCP와 같게 준다. `TERM=xterm-256color`, `COLORTERM=truecolor`.
- 작업 폴더: 프로젝트·코드 화면은 프로젝트 폴더, 일반 채팅은 사용자 홈(빈 값을 보내면 백엔드가 홈으로). 전체 경로이고 존재하는 폴더여야 연다(`resolve_cwd`).
- 명령: `terminal_open { request: { cwd, cols, rows, kind: "shell" | "server", command?, title? } } -> { id, info }`, `terminal_write`, `terminal_resize`, `terminal_close`, `terminal_list`, `terminal_snapshot`, 그리고 `terminal_busy`(아래), `terminal_shells`. `command`가 있으면 `powershell -NoLogo -Command <명령>`(cmd는 `/c`)로 그 명령 하나를 실행하고 끝나면 세션도 끝난다 — T2 서버 세션용. 명령은 Rust 쪽에서 앱 명령으로 등록한다(capabilities 파일은 core 이벤트 권한만 두고 앱 명령은 Rust에서 검사하는 기존 방식).
- 이벤트: `anylm-terminal-output { id, data, seq }`(16ms 모아 보냄, 한 번에 64KB 넘으면 바로), `anylm-terminal-exit { id, code }`. 출력 바이트는 `Utf8Decoder`가 덩어리 경계에서 잘린 글자를 다음 덩어리와 합쳐 글자로 만든다(잘못된 바이트는 U+FFFD).
- 링 버퍼: 세션마다 `TERMINAL_BUFFER_BYTES = 256 * 1024`(보낸 글자의 UTF-8 바이트, ANSI 포함). 앞이 잘려 글자 중간에서 시작하면 그 조각은 버린다. 도구용 글자는 `plain_text`(ANSI 제거 + 진행 표시처럼 `\r`로 덮어쓴 줄은 마지막 것만)로 만든다 — T3 `server_logs`가 `TerminalManager::text`로 쓴다.
- **스냅샷과 이벤트 번호(브리프와 다른 점)**: 브리프는 `terminal_snapshot { id } -> String`이었으나 `{ data, seq, running, exitCode }`를 돌려준다. 링 버퍼에 넣기와 이벤트 보내기를 한 잠금 안에서 하고 이벤트마다 `seq`를 1씩 올리므로, 화면은 스냅샷을 쓴 뒤 `seq`가 더 큰 이벤트만 이어 쓴다. 이벤트와 명령 응답은 도착 순서가 보장되지 않아 문자열만으로는 겹치거나 빠지는 출력이 생긴다. `running`·`exitCode`는 패널을 닫은 사이 끝난 셸을 다시 열 때 쓴다.
- 시작 DSR: ConPTY는 시작할 때 커서 위치를 묻고(`ESC[6n`) 답이 올 때까지 최대 3초 화면을 그리지 않는다. 화면(xterm)이 아직 없을 수 있으므로(T3: 패널을 닫은 채 서버 실행) 백엔드가 처음 1KB 안의 첫 질의에 `ESC[1;1R`로 답하고 그 질의는 화면으로 보내지 않는다. 그 뒤 질의는 xterm이 답한다. 스냅샷을 다시 그리는 동안 xterm이 옛 출력 속 질의에 답한 것은 셸로 보내지 않는다.
- 한도: `TERMINAL_MAX_SESSIONS = 8`(끝났지만 탭이 남은 세션도 센다). 넘으면 "터미널은 최대 8개까지 열 수 있습니다. 쓰지 않는 터미널 탭을 닫은 뒤 다시 시도하세요."

## 프로세스 트리 종료
- **Job Object + taskkill 둘 다**. 셸을 띄운 직후 `KILL_ON_JOB_CLOSE` Job Object에 넣는다(그 뒤 셸이 만드는 자식은 모두 Job에 들어간다). 닫을 때(`terminal_close`, 앱 종료 `RunEvent::Exit`의 `close_all`)는
  1. 셸이 살아 있으면 `taskkill /PID <셸> /T /F` — 트리 전체(Job에 넣기 전 아주 짧은 틈에 생긴 자식도),
  2. `TerminateJobObject` — 부모가 먼저 끝나 트리에서 떨어진 고아 프로세스(예: `cmd /c start /b …`)까지,
  3. 셸 핸들에 `TerminateProcess`.
- 앱이 비정상 종료해도 Job 핸들이 닫히면서 안의 프로세스가 끝난다. `portable-pty`가 프로세스를 일시 정지 상태로 띄우는 방법을 주지 않아 Job에 넣기 전 틈이 있으므로 taskkill을 함께 쓴다.
- 셸이 스스로 끝나면(`exit`) 남은 Job 프로세스는 탭을 닫을 때 끝낸다(탭에 "종료됨 (코드 n)"과 [다시 시작]).
- 탭 닫기 확인: `terminal_busy`가 Job 안에 셸 말고 다른 프로세스가 있으면 true → "이 터미널에서 실행 중인 프로그램이 있습니다. 탭을 닫으면 함께 종료됩니다. 닫을까요?". 셸만 있으면 묻지 않는다.
- 테스트(`terminal_tests.rs`, 실제 ConPTY, `cargo test --lib`에서 돈다): 셸에서 `ping -n 60`을 띄워 셸의 자손·Job에 있는지 확인 → 닫기 → `tasklist /FI "PID eq n"`로 ping·셸이 없어졌는지; `cmd /c start /b ping …`(고아)도 같은 방식. 닫기를 막아 보면 두 테스트가 실패하는 것도 확인했다.

## 화면
- 상단 바 오른쪽 끝 터미널 아이콘(툴팁 "터미널 (Ctrl+`)")과 `Ctrl+`` (`Backquote` 물리 키, 채팅 입력란·터미널 안에서도 — window capture 단계에서 받음). 열림 상태와 폭은 localStorage(`anylm_workpanel_open`, `anylm_workpanel_width`, try/catch), 폭은 끌어서 320px ~ 창 너비의 70%(기존 `PanelResizer`).
- 탭: 세션마다 하나, 같은 이름이면 "PowerShell 2"처럼 번호, [+ 새 터미널], ×(가운데 버튼 클릭도). 패널을 닫아도 세션은 계속 돌고, 다시 열면 링 버퍼로 화면을 복원한다. 탭 상태는 App의 `useWorkPanel`에 있어 패널을 닫아도 남고, 화면을 새로 불러오면 `terminal_list`로 복원한다. 패널을 처음 열 때 탭이 없으면 터미널 하나를 연다.
- 탭 모델은 T2를 위해 `{ kind: "terminal", sessionId, title, terminalKind: "shell" | "server", … } | { kind: "preview", url, title }`(미리보기는 아직 그리지 않음).
- 문서 캔버스와 함께: 둘 다 열리면 작업 패널이 캔버스 오른쪽, 캔버스가 쓸 수 있는 폭에서 작업 패널 폭을 뺀다. 창이 1100px보다 좁으면 둘 중 나중에 연 것만 보인다(`visibleSidePanels`).
- 복사·붙여넣기: 선택이 있으면 Ctrl+C(Ctrl+Shift+C)는 복사, 없으면 인터럽트(^C). Ctrl+V는 브라우저 붙여넣기 → xterm(괄호 붙여넣기). 오른쪽 클릭 붙여넣기는 넣지 않았다.
- 링크: 출력 속 http(s) 링크를 누르면 기본 브라우저(`open_external_url`). T2에서 localhost 링크를 미리보기로 돌린다.
- 테마: 앱의 밝은/어두운 색에 맞춘 팔레트(`xtermTheme.ts`, 밝은 바탕에서 노랑·흰 글자는 어둡게). 스크롤백 5000줄.
