//! 앱 IPC를 앱 자신의 화면에만 열어 둔다 (결정 0019).
//!
//! WebView2(wry)는 초기화 스크립트를 모든 프레임에 넣으므로 미리보기 iframe 안의 문서도
//! `__TAURI_INTERNALS__.invoke`를 부를 수 있다. Tauri는 요청의 `Origin`이 `http://tauri.localhost`
//! 이면 포트를 보지 않고 "로컬"로 보고, 앱 ACL 매니페스트가 없으면 모든 앱 명령을 허용한다.
//! 그래서 (1) 앱 명령은 출처가 앱 주소와 정확히(scheme·호스트·포트) 같을 때만 실행하고,
//! (2) 하위 프레임이 Tauri 예약 호스트(`tauri.localhost` 등)로 이동하는 것을 막는다.

use tauri::ipc::Invoke;
use tauri::{Manager, Runtime, Url};

/// 출처가 맞지 않는 앱 명령에 돌려주는 오류.
pub const REJECTED: &str = "허용되지 않은 출처의 요청입니다.";

/// Tauri가 스스로 등록하는 프로토콜 이름 (`asset`은 assetProtocol을 켤 때).
const TAURI_PROTOCOLS: &[&str] = &["tauri", "ipc", "asset"];

/// 앱이 `register_uri_scheme_protocol`·`register_asynchronous_uri_scheme_protocol`로 등록하는
/// 사용자 정의 프로토콜 이름. 지금은 없다. 새로 등록할 때는 이름을 반드시 여기에 넣고 이 상수를
/// 등록 코드에서 그대로 써야 한다. Windows에서 Tauri는 `http://<이름>.localhost`를 포트와 상관없이
/// "로컬"로 보므로, 이 목록이 하위 프레임 차단·미리보기 거부 대상(`is_reserved_host`)이 된다.
pub const APP_PROTOCOLS: &[&str] = &[];

/// Tauri가 앱 자신으로 취급하는 `<프로토콜>.localhost` 호스트인지 (대소문자·끝의 점 무시).
pub fn is_reserved_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host.strip_suffix(".localhost")
        .is_some_and(|name| TAURI_PROTOCOLS.contains(&name) || APP_PROTOCOLS.contains(&name))
}

/// 앱 화면의 출처 목록. 개발 모드는 `build.devUrl`, 배포본은 Tauri 프로토콜 주소.
pub fn app_origins(is_dev: bool, dev_url: Option<&Url>, use_https_scheme: bool) -> Vec<String> {
    if is_dev {
        if let Some(url) = dev_url {
            return vec![url.origin().ascii_serialization()];
        }
    }
    let scheme = if use_https_scheme { "https" } else { "http" };
    // Windows·Android는 http(s)://tauri.localhost, macOS·Linux는 tauri://localhost
    vec![format!("{scheme}://tauri.localhost"), "tauri://localhost".to_string()]
}

/// 실행 중인 앱의 출처 목록.
pub fn origins_of<R: Runtime>(app: &tauri::AppHandle<R>) -> Vec<String> {
    let config = app.config();
    let https = config.app.windows.iter().any(|w| w.use_https_scheme);
    app_origins(tauri::is_dev(), config.build.dev_url.as_ref(), https)
}

static ORIGINS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// 앱 시작(setup) 때 앱 출처를 기억해 둔다. 앱 핸들이 없는 곳(`dev_servers::local_url`)에서 쓴다.
pub fn remember_origins(origins: Vec<String>) {
    let _ = ORIGINS.set(origins);
}

/// 기억해 둔 앱 출처. setup 전(단위 테스트 등)에는 배포본 출처를 쓴다.
pub fn remembered_origins() -> Vec<String> {
    ORIGINS.get().cloned().unwrap_or_else(|| app_origins(false, None, false))
}

/// 주소의 출처 문자열 (`scheme://host[:port]`). 불투명 출처는 None.
pub fn origin_of(raw: &str) -> Option<String> {
    let url = Url::parse(raw.trim()).ok()?;
    let origin = url.origin();
    if origin.is_tuple() {
        return Some(origin.ascii_serialization());
    }
    // macOS·Linux 앱 프로토콜(tauri://localhost)은 url 크레이트에서 불투명 출처라 직접 만든다.
    let host = url.host_str().filter(|_| url.scheme() == "tauri")?;
    Some(match url.port() {
        Some(port) => format!("tauri://{host}:{port}"),
        None => format!("tauri://{host}"),
    })
}

/// 주소가 앱 화면(앱 출처와 scheme·호스트·포트가 정확히 같은 곳)인지.
pub fn is_app_origin(raw: &str, app_origins: &[String]) -> bool {
    origin_of(raw).is_some_and(|origin| app_origins.iter().any(|allowed| *allowed == origin))
}

/// 앱 명령 요청이 앱 화면에서 왔는지 본다.
///
/// `Origin` 헤더가 있으면 앱 출처와 정확히 같아야 한다. 사용자 정의 프로토콜
/// (`http://ipc.localhost`)로 오는 요청은 교차 출처 POST라 브라우저가 항상 `Origin`을 붙인다.
/// 헤더가 없는 것은 `postMessage` 경로뿐이고, WebView2는 최상위 문서의 메시지만
/// `WebMessageReceived`로 넘긴다. 그래서 이때는 웹뷰 최상위 문서의 주소(`top_level_url`,
/// 헤더가 없을 때만 부른다)가 앱 출처여야 한다.
pub fn origin_allowed(
    origin_header: Option<&[u8]>,
    top_level_url: impl FnOnce() -> Option<String>,
    app_origins: &[String],
) -> bool {
    match origin_header {
        Some(raw) => std::str::from_utf8(raw).is_ok_and(|raw| is_app_origin(raw, app_origins)),
        None => top_level_url().is_some_and(|url| is_app_origin(&url, app_origins)),
    }
}

/// 하위 프레임(iframe) 이동을 막아야 하는지: Tauri 예약 호스트(포트가 있든 없든)나
/// 앱 자체 출처로 가는 이동. `blob:` 같은 주소는 안쪽 출처로 판단한다.
pub fn blocks_frame_navigation(raw: &str, app_origins: &[String]) -> bool {
    // blob:·filesystem: 주소는 안쪽 URL의 출처, 불투명 출처(about:, data:)는 막을 대상이 아니다.
    let Some(origin) = origin_of(raw) else { return false };
    let Ok(url) = Url::parse(&origin) else { return false };
    if !matches!(url.scheme(), "http" | "https" | "tauri") {
        return false;
    }
    is_reserved_host(url.host_str().unwrap_or_default())
        || app_origins.iter().any(|allowed| *allowed == origin)
}

/// 앱 명령 처리기를 감싸 출처가 앱이 아닌 요청을 거절한다.
pub fn guard_invoke<R, F>(handler: F) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static
where
    R: Runtime,
    F: Fn(Invoke<R>) -> bool + Send + Sync + 'static,
{
    move |invoke: Invoke<R>| {
        let origins = origins_of(invoke.message.webview_ref().app_handle());
        let header = invoke.message.headers().get("origin").map(|v| v.as_bytes());
        // 웹뷰 주소 조회는 메인 스레드에서 바로 처리된다(tauri-runtime-wry send_user_message).
        let top = || invoke.message.webview_ref().url().ok().map(|u| u.to_string());
        if !origin_allowed(header, top, &origins) {
            eprintln!(
                "출처가 앱이 아닌 명령 요청을 거절했습니다: {} ({})",
                invoke.message.command(),
                header.map(String::from_utf8_lossy).unwrap_or_default()
            );
            invoke.resolver.reject(REJECTED);
            return true;
        }
        handler(invoke)
    }
}

/// 웹뷰의 하위 프레임 이동 감시를 건다 (Windows WebView2만).
pub fn install_frame_guard<R: Runtime>(webview: &tauri::Webview<R>) {
    #[cfg(windows)]
    {
        let origins = origins_of(webview.app_handle());
        let label = webview.label().to_string();
        let result = webview.with_webview(move |platform| {
            // SAFETY: 웹뷰 스레드(UI 스레드)에서 WebView2 COM 객체를 다룬다.
            let installed = unsafe {
                platform.controller().CoreWebView2().and_then(|core| frame_hook::install(core, origins))
            };
            if let Err(err) = installed {
                eprintln!("'{label}' 웹뷰의 iframe 이동 감시를 설치하지 못했습니다: {err}");
            }
        });
        if let Err(err) = result {
            eprintln!("iframe 이동 감시를 설치하지 못했습니다: {err}");
        }
    }
    #[cfg(not(windows))]
    let _ = webview;
}

#[cfg(windows)]
mod frame_hook {
    use std::cell::Cell;
    use std::rc::Rc;

    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2, ICoreWebView2Frame, ICoreWebView2Frame2, ICoreWebView2Frame7,
        ICoreWebView2NavigationStartingEventArgs, ICoreWebView2_4,
    };
    use webview2_com::{
        FrameChildFrameCreatedEventHandler, FrameCreatedEventHandler, FrameNavigationStartingEventHandler,
        NavigationStartingEventHandler,
    };
    use windows_core::{Interface, PWSTR};

    unsafe fn cancel_if_blocked(args: &ICoreWebView2NavigationStartingEventArgs, origins: &[String]) -> windows_core::Result<()> {
        let mut uri = PWSTR::null();
        unsafe { args.Uri(&mut uri)? };
        let uri = webview2_com::take_pwstr(uri);
        if super::blocks_frame_navigation(&uri, origins) {
            unsafe { args.SetCancel(true)? };
            eprintln!("차단한 iframe 이동: {uri}");
        }
        Ok(())
    }

    /// 프레임 하나의 이동과, 그 안에서 새로 생기는 프레임을 감시한다. 중첩 감시를 걸었으면 true.
    unsafe fn watch_frame(frame: &ICoreWebView2Frame, origins: &Rc<Vec<String>>) -> bool {
        let mut token = 0i64;
        if let Ok(frame2) = frame.cast::<ICoreWebView2Frame2>() {
            let o = origins.clone();
            let handler = FrameNavigationStartingEventHandler::create(Box::new(move |_, args| {
                match args {
                    Some(args) => unsafe { cancel_if_blocked(&args, &o) },
                    None => Ok(()),
                }
            }));
            if let Err(err) = unsafe { frame2.add_NavigationStarting(&handler, &mut token) } {
                eprintln!("iframe 이동 감시를 걸지 못했습니다: {err}");
            }
        }
        let Ok(frame7) = frame.cast::<ICoreWebView2Frame7>() else { return false };
        let o = origins.clone();
        let handler = FrameChildFrameCreatedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                let child = unsafe { args.Frame()? };
                unsafe { watch_frame(&child, &o) };
            }
            Ok(())
        }));
        unsafe { frame7.add_FrameCreated(&handler, &mut token) }.is_ok()
    }

    pub unsafe fn install(core: ICoreWebView2, origins: Vec<String>) -> windows_core::Result<()> {
        let origins = Rc::new(origins);
        let mut token = 0i64;
        // 최상위 문서의 직속 iframe 이동 (오래된 런타임에서도 있는 이벤트)
        let o = origins.clone();
        let handler = NavigationStartingEventHandler::create(Box::new(move |_, args| match args {
            Some(args) => unsafe { cancel_if_blocked(&args, &o) },
            None => Ok(()),
        }));
        unsafe { core.add_FrameNavigationStarting(&handler, &mut token)? };

        // 새 iframe마다 그 프레임의 이동과 그 안의 중첩 iframe까지 감시한다.
        let core4: ICoreWebView2_4 = core.cast()?;
        let warned = Cell::new(false);
        let handler = FrameCreatedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                let frame = unsafe { args.Frame()? };
                if !unsafe { watch_frame(&frame, &origins) } && !warned.replace(true) {
                    eprintln!("WebView2 런타임이 ICoreWebView2Frame7을 지원하지 않아 중첩 iframe 이동은 감시하지 못합니다.");
                }
            }
            Ok(())
        }));
        unsafe { core4.add_FrameCreated(&handler, &mut token) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prod() -> Vec<String> {
        app_origins(false, None, false)
    }

    fn dev() -> Vec<String> {
        app_origins(true, Some(&Url::parse("http://localhost:1420").unwrap()), false)
    }

    #[test]
    fn app_origins_follow_dev_url_or_the_tauri_protocol() {
        assert_eq!(prod(), vec!["http://tauri.localhost", "tauri://localhost"]);
        assert_eq!(app_origins(false, None, true)[0], "https://tauri.localhost");
        assert_eq!(dev(), vec!["http://localhost:1420"]);
        // 배포본은 devUrl을 보지 않는다
        assert_eq!(app_origins(false, Some(&Url::parse("http://localhost:1420").unwrap()), false), prod());
    }

    #[test]
    fn only_the_exact_app_origin_may_invoke() {
        let ok = |h: &str, origins: &[String]| origin_allowed(Some(h.as_bytes()), || panic!("헤더가 있으면 웹뷰 주소를 보지 않는다"), origins);
        assert!(ok("http://tauri.localhost", &prod()));
        assert!(ok("tauri://localhost", &prod()));
        assert!(ok("http://localhost:1420", &dev()));
        for bad in [
            "http://tauri.localhost:5173",
            "http://ipc.localhost:1",
            "http://ipc.localhost",
            "http://localhost:5173",
            "http://anylm-preview.localhost:1234",
            "https://tauri.localhost",
            "http://tauri.localhost.evil.com",
            "null",
            "",
            "garbage",
        ] {
            assert!(!ok(bad, &prod()), "{bad}");
        }
        for bad in ["http://localhost:5173", "http://tauri.localhost", "http://127.0.0.1:1420", "https://localhost:1420"] {
            assert!(!ok(bad, &dev()), "{bad}");
        }
        assert!(!origin_allowed(Some(&[0xff, 0xfe]), || None, &prod()));
        // postMessage 경로(최상위 문서만)에는 헤더가 없다: 최상위 문서 주소가 앱이어야 한다
        let top = |u: &'static str| move || Some(u.to_string());
        assert!(origin_allowed(None, top("http://tauri.localhost/index.html"), &prod()));
        assert!(origin_allowed(None, top("http://localhost:1420/"), &dev()));
        for bad in ["http://tauri.localhost:5173/x", "http://localhost:5173/", "https://tauri.localhost/", "about:blank"] {
            assert!(!origin_allowed(None, top(bad), &prod()), "{bad}");
        }
        assert!(!origin_allowed(None, top("https://localhost:1420/"), &dev()));
        assert!(!origin_allowed(None, || None, &prod()));
    }

    #[test]
    fn reserved_hosts_are_tauri_and_app_protocol_names() {
        for host in ["tauri.localhost", "IPC.localhost", "asset.localhost.", "tauri.LOCALHOST"] {
            assert!(is_reserved_host(host), "{host}");
        }
        for host in ["localhost", "anylm-preview.localhost", "tauri.localhost.evil.com", "x.tauri.localhost", "tauri"] {
            assert!(!is_reserved_host(host), "{host}");
        }
        for name in APP_PROTOCOLS {
            assert!(is_reserved_host(&format!("{name}.localhost")));
        }
    }

    #[test]
    fn frames_may_not_navigate_to_reserved_tauri_hosts() {
        let blocked = |u: &str| blocks_frame_navigation(u, &prod());
        for bad in [
            "http://tauri.localhost:5173/x",
            "http://tauri.localhost/",
            "https://TAURI.localhost:443/",
            "http://ipc.localhost:1/",
            "http://ipc.localhost/plugin%3Aevent%7Cemit",
            "http://asset.localhost:8080/",
            "http://tauri.localhost.:5173/",
            "blob:http://tauri.localhost:5173/0b6f",
            "tauri://localhost/index.html",
        ] {
            assert!(blocked(bad), "{bad}");
        }
        for fine in [
            "http://localhost:5173/",
            "http://127.0.0.1:8000/",
            "http://anylm-preview.localhost:1234/token/site/index.html",
            "http://test.localhost:3000/",
            "about:blank",
            "about:srcdoc",
            "data:text/html,hi",
            "not a url",
        ] {
            assert!(!blocked(fine), "{fine}");
        }
        // 개발 모드에서는 앱 자체(devUrl)도 프레임에 넣지 못한다
        assert!(blocks_frame_navigation("http://localhost:1420/", &dev()));
        assert!(!blocks_frame_navigation("http://localhost:5173/", &dev()));
    }

    #[tauri::command]
    fn ping() -> &'static str {
        "pong"
    }

    fn call(app: &tauri::App<tauri::test::MockRuntime>, label: &str, origin: &str) -> Result<String, serde_json::Value> {
        let webview = tauri::WebviewWindowBuilder::new(app, label, Default::default()).build().unwrap();
        let mut headers = tauri::http::HeaderMap::new();
        headers.insert("Origin", origin.parse().unwrap());
        tauri::test::get_ipc_response(&webview, tauri::webview::InvokeRequest {
            cmd: "ping".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: origin.parse().unwrap(),
            body: tauri::ipc::InvokeBody::default(),
            headers,
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        }).map(|body| body.deserialize::<String>().unwrap())
    }

    #[test]
    fn spoofed_tauri_localhost_port_reaches_plain_handlers_but_not_the_guarded_one() {
        let ctx = || tauri::test::mock_context(tauri::test::noop_assets());
        // 대조군: Tauri 자체는 포트가 붙은 tauri.localhost도 "로컬"로 보고 앱 명령을 실행한다.
        let plain = tauri::test::mock_builder().invoke_handler(tauri::generate_handler![ping]).build(ctx()).unwrap();
        assert_eq!(call(&plain, "a", "http://tauri.localhost:5173").unwrap(), "pong");

        let guarded = tauri::test::mock_builder()
            .invoke_handler(guard_invoke(tauri::generate_handler![ping]))
            .build(ctx())
            .unwrap();
        assert_eq!(call(&guarded, "b", "http://tauri.localhost:5173").unwrap_err(), REJECTED);
        // 모의 런타임에는 ipc 프로토콜이 등록되지 않아 Tauri ACL이 먼저 거절한다 (실제 Windows에서는 위와 같은 경로).
        assert!(call(&guarded, "c", "http://ipc.localhost:1").is_err());
        assert!(call(&guarded, "e", "http://localhost:5173").is_err());
        assert_eq!(call(&guarded, "d", "http://tauri.localhost").unwrap(), "pong");

        // Origin 없는 요청(postMessage 경로): 최상위 문서가 앱일 때만 실행한다.
        let no_origin = |label: &str, top: Option<&str>| {
            let webview = tauri::WebviewWindowBuilder::new(&guarded, label, Default::default()).build().unwrap();
            if let Some(top) = top {
                webview.navigate(top.parse().unwrap()).unwrap();
            }
            let url = webview.url().unwrap();
            tauri::test::get_ipc_response(&webview, tauri::webview::InvokeRequest {
                cmd: "ping".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url,
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            }).map(|body| body.deserialize::<String>().unwrap())
        };
        assert_eq!(no_origin("f", Some("http://tauri.localhost/index.html")).unwrap(), "pong");
        assert_eq!(no_origin("g", Some("http://tauri.localhost:5173/x")).unwrap_err(), REJECTED);
    }
}
