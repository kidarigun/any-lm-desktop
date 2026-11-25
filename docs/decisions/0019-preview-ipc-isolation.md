# 0019 — 미리보기 프레임의 앱 IPC 격리

0010의 "자식 프레임은 앱 명령을 쓸 수 없다"는 판단을 바로잡는다. 0010은 `http://localhost:<포트>`에서 연 자식 프레임만 확인했다. 아래 경로는 확인하지 않았다.

## 문제 (전체 검토 C1)

- wry 0.57.2(WebView2)는 초기화 스크립트를 `AddScriptToExecuteOnDocumentCreated`로 등록한다. 이 스크립트는 모든 프레임에서 돈다. 그래서 교차 출처 iframe에도 `__TAURI_INTERNALS__`와 invoke 키가 생긴다.
- tauri 2.12.5는 IPC 요청의 `Origin` 헤더를 출처로 쓴다. `is_local_url`의 첫 조건은 Tauri 프로토콜 주소(`http://tauri.localhost`)와 scheme·도메인만 비교하고 포트는 보지 않는다. Windows에서는 등록된 프로토콜 이름의 `<이름>.localhost`(예: `ipc.localhost`)도 포트와 상관없이 로컬로 본다.
- `http://tauri.localhost:5173`처럼 포트가 붙은 주소는 WebView2 필터 `http://tauri.localhost/*`에 걸리지 않는다. Chromium은 `*.localhost`를 루프백으로 풀기 때문에 이 요청은 프로젝트 개발 서버로 간다.
- 이 앱은 앱 ACL 매니페스트가 없다. 그래서 로컬로 판정된 요청은 모든 앱 명령(`dev_server_start`, `terminal_open`, 대화·설정 읽기 등)을 실행할 수 있다.
- 공격 경로: 악성 프로젝트의 서버 페이지가 `<iframe src="http://tauri.localhost:5173/x">`를 넣고, 그 안에서 `__TAURI_INTERNALS__.invoke(...)`를 부른다. 중첩 iframe은 앱 CSP의 적용을 받지 않는다. `on_navigation`은 최상위 문서의 이동만 본다.
- `ipc_guard::tests::spoofed_tauri_localhost_port_reaches_plain_handlers_but_not_the_guarded_one`의 대조군이 이 동작을 Tauri 모의 런타임에서 재현한다. 감싸지 않은 처리기는 `Origin: http://tauri.localhost:5173` 요청을 그대로 실행한다.

## 결정 (여러 겹)

1. **앱 명령 출처 확인** (`src-tauri/src/ipc_guard.rs`, `guard_invoke`): `generate_handler!`가 만든 처리기를 감싼다. 요청의 `Origin`이 앱 출처와 scheme·호스트·포트까지 정확히 같지 않으면 `"허용되지 않은 출처의 요청입니다."`로 거절한다.
   - 앱 출처는 개발 모드(`tauri::is_dev()`)에서 `build.devUrl`의 출처(`http://localhost:1420`)다. 배포본에서는 `http://tauri.localhost`이고, 창이 `useHttpsScheme`을 쓸 때만 `https://tauri.localhost`가 된다. macOS·Linux용 `tauri://localhost`도 앱 출처에 들어간다.
   - `Origin` 헤더가 없으면 허용한다. 사용자 정의 프로토콜(`http://ipc.localhost`)로 오는 요청은 교차 출처 POST라서 브라우저가 언제나 `Origin`을 붙인다. 헤더가 없는 요청은 `postMessage` 대체 경로에서만 나온다. WebView2는 최상위 문서의 `chrome.webview.postMessage`만 `CoreWebView2.WebMessageReceived`로 넘긴다. wry도 이 이벤트 하나만 듣는다. 최상위 문서는 탐색 가드가 앱 주소에 묶어 둔다(아래 4).
2. **하위 프레임 이동 차단** (Windows, `install_frame_guard`, `navigation-guard` 플러그인의 `on_webview_ready`): `ICoreWebView2.FrameNavigationStarting`이 최상위 문서의 직속 iframe을 본다. `ICoreWebView2_4.FrameCreated`는 새 프레임마다 `ICoreWebView2Frame2.NavigationStarting`과 `ICoreWebView2Frame7.FrameCreated`(중첩 프레임, 재귀)를 건다. 다음 두 경우의 이동은 취소한다(`blocks_frame_navigation`).
   - 호스트가 `tauri.localhost`, `ipc.localhost`, `asset.localhost` 중 하나다. 포트는 있든 없든 막는다. 대소문자와 끝의 점도 가리지 않는다. `blob:` 주소는 안쪽 출처로 판단한다.
   - 이동할 곳이 앱 출처와 같다(개발 모드의 `http://localhost:1420`).
   - 이 앱은 따로 등록한 사용자 정의 프로토콜이 없다. 그래서 예약 호스트 목록은 Tauri가 쓰는 이름으로 고정했다. 리디렉션도 `NavigationStarting`으로 다시 들어오므로 함께 막힌다. webview2-com은 wry와 같은 0.39.1(직접 의존성)이다.
3. **CSP 좁히기**: `tauri.conf.json`의 `csp`·`devCsp` 두 곳에서 `frame-src`의 `*.localhost` 와일드카드를 뺐다. 이제 `http://localhost:* https://localhost:* http://127.0.0.1:* https://127.0.0.1:* http://anylm-preview.localhost:*`만 남는다. `anylm-preview.localhost`는 캔버스 HTML 미리보기 서버(`html_preview.rs`)용이다.
   - 미리보기 창(`previewDestination`)은 `localhost`, `127.0.0.1`, `[::1]`, `anylm-preview.localhost`만 프레임에 넣는다. 그 밖의 `*.localhost`는 기존 안내("외부 사이트는 브라우저에서 엽니다.")를 띄우고 기본 브라우저로 연다.
   - 예약 호스트(`tauri`·`ipc`·`asset.localhost`)는 프레임에도 브라우저에도 열지 않는다. Rust `local_url`도 같은 예약 호스트를 거부한다. 개발 서버 주소 감지와 확인용으로 다른 `*.localhost`는 계속 허용한다.
4. **최상위 탐색 가드**: `is_app_url`은 이제 포트가 붙은 `tauri.localhost`를 앱 주소로 보지 않는다. 기본 포트는 포트가 없는 것과 같게 본다.

## 남는 것 (적용 범위 밖)

- **플러그인 명령과 이벤트는 1번 확인을 거치지 않는다.** Tauri는 `plugin:` 명령을 앱 처리기에 넘기지 않는다. 이 앱의 권한은 `capabilities/default.json`의 `core:event:default`(listen·unlisten·emit·emit_to)뿐이다.
  - 2번 차단이 뚫려 어떤 프레임이 예약 호스트 출처를 얻으면, 그 프레임은 `plugin:event|emit`으로 앱 이벤트를 위조할 수 있다. 예: `anylm-open-preview`, `anylm-dev-server`, 문서 변환 요청 이벤트. 2번 차단이 뚫리는 경우는 아래 셋이다.
    - 오래된 WebView2 런타임: 중첩 iframe을 감시하지 못한다.
    - WebView2가 `NavigationStarting`을 내지 않는 경로.
    - 감시를 거는 시점보다 앞서 일어나는 이동.
  - `listen` 콜백은 최상위 문서에서 `eval`되므로 iframe이 이벤트 내용을 받아 가지는 못한다고 판단한다. 이것은 소스를 읽고 내린 판단이다.
  - 플러그인 명령까지 막으려면 원격 출처를 명시한 capability나 Tauri 업스트림 수정(`is_local_url`의 포트 비교)이 필요하다.
- **오래된 WebView2 런타임**: `ICoreWebView2Frame7`이 없는 런타임은 1.0.2420보다 오래된 것이다. 이런 런타임에서는 최상위 문서의 직속 iframe만 감시하고, 로그에 "중첩 iframe 이동은 감시하지 못합니다"를 한 번 남긴다. 앱 명령은 이때도 1번이 막는다.
- **실행 중 확인**: 프레임 감시가 실제로 걸리는지는 Windows 개발 창에서 확인해야 한다. 절차는 STATUS 수동 확인 목록의 "미리보기 IPC 격리 (결정 0019)"에 있다. 단위 테스트는 출처·호스트 판정과 Tauri 모의 런타임에서의 거절을 확인한다.

## 참고

- tauri 2.12.5 `src/ipc/protocol.rs`(`Origin` 헤더 → `InvokeRequest.url`, `postMessage` 경로는 `WebMessageReceived`의 `Source`)
- tauri 2.12.5 `src/webview/mod.rs`(`is_local_url`, 앱 ACL 매니페스트가 없을 때 로컬 앱 명령은 ACL 확인 생략)
- wry 0.57.2 `src/webview2/mod.rs`(초기화 스크립트 등록, `WebMessageReceived`)
- [WebView2 iframe 이벤트](https://learn.microsoft.com/microsoft-edge/webview2/how-to/communicate-btwn-web-native)
