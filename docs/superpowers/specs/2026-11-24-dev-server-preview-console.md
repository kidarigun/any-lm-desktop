# 개발 서버 실행 · 앱 안 미리보기 · 오른쪽 터미널 — 설계

날짜: 2026-11-24 · 상태: 사용자 결정 반영 · 저장소 위치(커밋 시): `docs/superpowers/specs/2026-11-24-dev-server-preview-console.md`

## 1. 사용자 요청과 결정

요청: "로컬에 설치된 개발환경에서 서버를 가동한 경우 서버를 실행하여 프로그램이나 웹사이트를 확인할 수 있는 기능을 추가해줘. 콘솔 창을 오른쪽 사이드바에 열고 닫을 수 있도록 해줘."

| 항목 | 결정 |
|---|---|
| 미리보기 | 앱 안 미리보기 창(오른쪽 패널). 주소창, 새로고침, 뒤로/앞으로, [브라우저로 열기]. |
| 실행 주체 | 사용자([서버 실행] 버튼)와 모델(`start_server` 도구, 승인 필요) 둘 다. |
| 콘솔 | 완전한 터미널(PTY): PowerShell 직접 입력, 색상, 진행 표시, Ctrl+C, 여러 탭. 서버 출력도 터미널 탭으로 본다. |

## 2. 화면 구성 — 오른쪽 작업 패널

- 오른쪽 사이드바 "작업 패널"을 연다/닫는다: 상단 바 버튼(터미널 아이콘)과 단축키 `` Ctrl+` ``. 열림 상태와 너비는 기억한다(localStorage, try/catch). 너비는 끌어서 조절(최소 320px, 최대 창 너비의 70%).
- 패널 위쪽 탭 두 종류: **터미널** 탭들(PowerShell, 서버)과 **미리보기** 탭. 탭마다 닫기(×). [+ 새 터미널], [서버 실행 ▾] 버튼.
- 기존 문서 캔버스(오른쪽 문서 창)와 동시에 열 수 있다: 캔버스가 열려 있으면 작업 패널은 캔버스 오른쪽에 붙는다. 창이 좁으면(1100px 미만) 둘 중 나중에 연 것만 보인다.
- 일반 채팅에서도 터미널은 열 수 있다(작업 폴더 = 사용자 홈). 프로젝트·코드 화면에서는 프로젝트 폴더.
- 화면 문구 한국어, 브랜드 문구 금지(EXAONE/엑사원/LG 금지) 규칙 유지.

## 3. 터미널 (PTY)

- 백엔드: Rust `portable-pty`(Windows ConPTY). 모듈 `terminal.rs`. 셸: `powershell.exe -NoLogo`(설정에서 `pwsh`/`cmd` 고를 수 있게 하되 기본은 Windows PowerShell). 환경 변수 PATH 앞에 앱이 설치한 실행 환경(uv, Node 등 `runtime` 모듈이 아는 경로)을 붙인다.
- 명령: `terminal_open { cwd, cols, rows, kind: "shell" | "server", command? } -> { id }`, `terminal_write { id, data }`, `terminal_resize { id, cols, rows }`, `terminal_close { id }`, `terminal_list`.
- 출력은 이벤트 `anylm-terminal-output { id, data }`(UTF-8, 16ms 정도 모아 보냄), 종료는 `anylm-terminal-exit { id, code }`.
- 세션마다 최근 출력 링 버퍼 `TERMINAL_BUFFER_BYTES = 256 * 1024`(ANSI 제거본은 도구용으로 따로 만든다). 패널을 닫았다 열어도 버퍼로 화면을 복원한다.
- 동시 세션 한도 `TERMINAL_MAX_SESSIONS = 8`. 넘으면 열지 않고 이유를 보인다.
- 닫기·앱 종료 때 **프로세스 트리 전체**를 끝낸다(Windows: `taskkill /PID <pid> /T /F` 또는 Job Object). 남는 node/python 프로세스가 없어야 한다.
- 프런트엔드: `@xterm/xterm` + `@xterm/addon-fit`(+ 가능하면 `@xterm/addon-web-links`: 링크 클릭 → localhost면 미리보기, 아니면 기본 브라우저). 스크롤백 5000줄. 테마는 앱의 밝은/어두운 색과 맞춘다. 복사(선택 후 Ctrl+C는 복사, 선택 없으면 인터럽트), 붙여넣기 Ctrl+V.

## 4. 개발 서버

- 서버 실행 = `kind: "server"` 터미널 세션. 탭 제목은 서버 이름, 상태 점(시작 중/실행 중/종료됨), 감지한 주소.
- **[서버 실행 ▾] 메뉴** 항목(프로젝트 폴더 기준 자동 감지, 하위 폴더 1단계까지):
  - `package.json`의 scripts 중 `dev`, `start`, `serve`, `preview`(있는 것만). 패키지 매니저: `pnpm-lock.yaml`→pnpm, `yarn.lock`→yarn, `bun.lockb`/`bun.lock`→bun, 그 밖→npm (`npm run dev`).
  - Python: `manage.py` → `python manage.py runserver`; `streamlit` 의존성이 있고 `app.py`/`streamlit_app.py` → `streamlit run …`. 
  - 정적 사이트: 루트에 `index.html`만 있고 위가 없으면 `python -m http.server 8000`.
  - 저장해 둔 사용자 정의 서버(아래) + [직접 입력…](이름, 명령, 폴더, 주소(선택)).
- 저장: `Project.dev_servers: Vec<DevServerConfig { id, name, command, cwd /*프로젝트 기준 상대 경로*/, url: Option<String> }>`(serde 기본값, 옛 데이터 호환). 마지막으로 쓴 서버를 메뉴 맨 위에.
- **주소 감지**: 서버 출력(ANSI 제거)에서 `https?://(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]|[\w.-]+\.localhost):\d+(/\S*)?`를 찾는다. `0.0.0.0`은 `localhost`로 바꾼다. 처음 찾은 주소를 그 서버의 주소로 삼고(사용자 정의 `url`이 있으면 그것), 주소의 포트가 열리면(TCP 연결 성공, 0.5초 간격 최대 60초) "실행 중"으로 바꾸고 **미리보기 탭을 자동으로 연다**(설정으로 끌 수 있음).
- 같은 서버를 두 번 실행하지 않는다(이미 실행 중이면 그 탭으로 이동).

## 5. 미리보기

- 미리보기 탭 = `<iframe>`(주소창, 새로고침, 뒤로/앞으로, [브라우저로 열기], 화면 폭 선택: 전체/태블릿 768/모바일 375).
- **로컬 주소만** 미리보기에서 연다: `http(s)://localhost`, `127.0.0.1`, `[::1]`, `*.localhost`. 다른 주소는 주소창에 넣으면 "외부 사이트는 브라우저로 엽니다"와 함께 기본 브라우저로 연다.
- CSP: `frame-src`에 `http://localhost:* http://127.0.0.1:* http://[::1]:* https://localhost:* http://*.localhost:*`만 추가. 다른 지시어는 그대로.
- iframe은 앱 IPC(Tauri API)에 접근할 수 없어야 한다(다른 출처 iframe에는 IPC가 주입되지 않음을 확인하고 테스트/문서로 남긴다).
- 페이지가 iframe을 거부하면(X-Frame-Options/CSP frame-ancestors, 로드 실패) "이 페이지는 앱 안에서 열 수 없습니다 — [브라우저로 열기]" 안내.
- 파일 저장 시 자동 새로고침은 하지 않는다(개발 서버의 HMR에 맡긴다). [새로고침] 버튼만.

## 6. 모델 도구 (프로젝트 모드만)

- `start_server { command, name?, cwd?, url? }`: **승인 필요**(프로젝트의 "묻지 않고 실행" 허용 목록에 `start_server`를 추가할 수 있다 — `AUTO_APPROVABLE`에 포함). 실행하면 서버 터미널 탭을 열고, 주소가 감지되어 포트가 열릴 때까지(최대 60초) 기다린 뒤 `{ id, url?, status, 최근 출력 40줄 }`을 돌려준다. 실패·종료 시 종료 코드와 출력.
- `server_logs { id?, lines? }`(기본 80, 최대 400, 도구 출력 상한 `tool_output_cap` 적용): 승인 없이. id가 없으면 실행 중인 서버 목록과 각 최근 10줄.
- `stop_server { id }`: 승인 없이(자기가 띄운 것이든 사용자가 띄운 것이든 서버 세션만; 셸 탭은 못 끈다).
- `open_preview { url }`: 승인 없이, 로컬 주소만. 미리보기 탭을 연다.
- 프롬프트 안내(프로젝트 모드 도구 설명 절): "개발 서버처럼 끝나지 않는 명령(npm run dev, python manage.py runserver 등)은 run_command가 아니라 start_server로 실행하세요. 서버가 뜨면 open_preview로 보여 주고, 오류가 나면 server_logs로 출력을 확인하세요."
- `run_command`가 끝나지 않는 서버 명령으로 시간 초과되는 기존 문제는 위 안내로 줄인다(동작 변경은 하지 않음).

## 7. 완료 기준

1. 상단 버튼/`` Ctrl+` ``로 오른쪽 작업 패널이 열리고 닫히며, PowerShell 터미널에서 `dir`, `node -v`, 색상 출력, Ctrl+C가 동작한다. 탭을 여러 개 열고 닫을 수 있다.
2. Vite/React 예제 프로젝트(또는 `index.html` 정적 사이트)에서 [서버 실행 ▾]으로 서버를 띄우면 주소가 감지되고 미리보기 탭에 페이지가 보인다. [브라우저로 열기]가 동작한다.
3. 서버 탭을 닫거나 앱을 끄면 node/python 프로세스가 남지 않는다.
4. 모델이 `start_server`(승인 후) → `open_preview`로 페이지를 보여 주고, `server_logs`로 오류를 읽는다(실측: 하니스 시나리오, 35B 또는 9B).
5. 세 검증 명령(`npm test`, `npx tsc --noEmit`, `cargo test --lib`) 통과, 새 모듈마다 단위 테스트(주소 감지 정규식, 서버 감지, 링 버퍼, 로컬 주소 판정, 도구 인자 검사).

## 8. 작업 나누기 (순서대로, 한 번에 하나)

- T1 터미널: `terminal.rs`(PTY, 링 버퍼, 트리 종료), 명령·이벤트, 오른쪽 작업 패널(열기/닫기/너비/탭), xterm 터미널 탭.
- T2 서버·미리보기: 서버 감지, `dev_servers` 저장, [서버 실행 ▾], 주소 감지·포트 확인, 미리보기 탭(iframe, 로컬만, CSP), 프로세스 정리 확인.
- T3 모델 도구: `start_server`/`server_logs`/`stop_server`/`open_preview`, 승인·자동 승인, 프롬프트 안내, 하니스 시나리오.
