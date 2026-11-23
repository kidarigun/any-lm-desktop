//! 앱을 두 번 실행하지 않게 한다.
//!
//! 내 PC 전용 주소(127.0.0.1)의 포트 하나를 잡아 둔다. 이미 잡혀 있으면 그 앱에 "창을 보여 줘"라고
//! 알리고 새 앱은 끝낸다. 저장 파일·로그인 포트를 두 앱이 함께 쓰다 꼬이는 것을 막는다.
//! (다른 프로그램이 그 포트를 쓰고 있으면 답이 맞지 않으므로 그냥 실행한다.)

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// 원본 앱(K-EXAONE Desktop)은 45788(중복 실행)·45789(MCP OAuth)를 쓴다. 이 앱은 45790, OAuth는 45791(`mcp_oauth::CALLBACK_PORT`).
pub const PORT: u16 = 45790;
const HELLO: &[u8] = b"ANYLM-DESKTOP-SHOW\n";
const ACK: &[u8] = b"ANYLM-DESKTOP-OK\n";

pub enum Instance {
    /// 처음 실행된 앱: 다른 실행의 요청을 받는다
    First(TcpListener),
    /// 이미 실행 중인 앱에 알렸다: 이 실행은 끝낸다
    AlreadyRunning,
    /// 확인할 수 없음 (포트를 다른 프로그램이 씀): 그냥 실행한다
    Unknown,
}

pub fn acquire() -> Instance {
    acquire_on(PORT)
}

fn acquire_on(port: u16) -> Instance {
    if let Ok(l) = TcpListener::bind(("127.0.0.1", port)) {
        return Instance::First(l);
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else { return Instance::Unknown };
    let _ = s.set_read_timeout(Some(Duration::from_millis(1500)));
    if s.write_all(HELLO).is_err() {
        return Instance::Unknown;
    }
    let mut buf = [0u8; 32];
    match s.read(&mut buf) {
        Ok(n) if &buf[..n] == ACK => Instance::AlreadyRunning,
        _ => Instance::Unknown,
    }
}

pub struct ListenerGuard {
    stopped: Arc<AtomicBool>,
    addr: Option<SocketAddr>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl ListenerGuard {
    /// 종료 전에 리스너를 멈추고 포트 점유를 즉시 해제한다.
    pub fn shutdown(&self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            if let Some(addr) = self.addr {
                let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(200));
            }
        }
        if let Ok(mut guard) = self.handle.lock() {
            if let Some(h) = guard.take() {
                let _ = h.join();
            }
        }
    }
}

/// 다른 실행이 보낸 요청을 받으면 `on_show`를 부른다.
pub fn listen(listener: TcpListener, on_show: impl Fn() + Send + 'static) -> ListenerGuard {
    let stopped = Arc::new(AtomicBool::new(false));
    let addr = listener.local_addr().ok();
    let stopped_flag = stopped.clone();
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming() {
            if stopped_flag.load(Ordering::SeqCst) {
                break;
            }
            let Ok(mut s) = stream else { continue };
            if stopped_flag.load(Ordering::SeqCst) {
                break;
            }
            let _ = s.set_read_timeout(Some(Duration::from_millis(1000)));
            let mut buf = [0u8; 32];
            if matches!(s.read(&mut buf), Ok(n) if &buf[..n] == HELLO) {
                let _ = s.write_all(ACK);
                on_show();
            }
        }
    });
    ListenerGuard {
        stopped,
        addr,
        handle: Mutex::new(Some(handle)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn identity_constants_differ_from_the_original_app() {
        assert_eq!(PORT, 45790);
        assert_eq!(HELLO, b"ANYLM-DESKTOP-SHOW
");
    }

    #[test]
    fn ports_do_not_collide_with_each_other_or_the_original_app() {
        let oauth = crate::mcp_oauth::CALLBACK_PORT;
        assert_eq!(oauth, 45791);
        // 앱이 켜져 있는 동안 중복 실행 포트를 잡고 있으므로 OAuth 포트와 같으면 로그인을 못 받는다.
        assert_ne!(PORT, oauth);
        // 원본 앱: 45788(중복 실행), 45789(MCP OAuth)
        for upstream in [45788u16, 45789] {
            assert_ne!(PORT, upstream);
            assert_ne!(oauth, upstream);
        }
        assert!(crate::mcp_oauth::redirect_uri().contains(":45791/"));
    }

    #[test]
    fn second_launch_wakes_the_first() {
        // 테스트용 빈 포트
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let Instance::First(l) = acquire_on(port) else { panic!("처음 실행이어야 함") };
        let shown = Arc::new(AtomicUsize::new(0));
        let s = shown.clone();
        let guard = listen(l, move || {
            s.fetch_add(1, Ordering::SeqCst);
        });
        assert!(matches!(acquire_on(port), Instance::AlreadyRunning));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(shown.load(Ordering::SeqCst), 1);

        // 종료 전 shutdown 시 포트를 즉시 재바인딩할 수 있어야 한다.
        guard.shutdown();
        let rebound = TcpListener::bind(("127.0.0.1", port));
        assert!(rebound.is_ok(), "shutdown 후 포트가 해제되어야 함");
    }

    #[test]
    fn other_program_on_the_port_is_ignored() {
        // 우리 앱이 아닌 프로그램이 포트를 잡고 있으면 답이 다르다 → 그냥 실행
        let other = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = other.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in other.incoming().flatten() {
                let mut s = s;
                let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
            }
        });
        assert!(matches!(acquire_on(port), Instance::Unknown));
    }
}
