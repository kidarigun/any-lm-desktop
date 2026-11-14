//! 앱을 두 번 실행하지 않게 한다.
//!
//! 내 PC 전용 주소(127.0.0.1)의 포트 하나를 잡아 둔다. 이미 잡혀 있으면 그 앱에 "창을 보여 줘"라고
//! 알리고 새 앱은 끝낸다. 저장 파일·로그인 포트를 두 앱이 함께 쓰다 꼬이는 것을 막는다.
//! (다른 프로그램이 그 포트를 쓰고 있으면 답이 맞지 않으므로 그냥 실행한다.)

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const PORT: u16 = 45788;
const HELLO: &[u8] = b"EXAONE-DESKTOP-SHOW\n";
const ACK: &[u8] = b"EXAONE-DESKTOP-OK\n";

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

/// 다른 실행이 보낸 요청을 받으면 `on_show`를 부른다.
pub fn listen(listener: TcpListener, on_show: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let _ = s.set_read_timeout(Some(Duration::from_millis(1000)));
            let mut buf = [0u8; 32];
            if matches!(s.read(&mut buf), Ok(n) if &buf[..n] == HELLO) {
                let _ = s.write_all(ACK);
                on_show();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn second_launch_wakes_the_first() {
        // 테스트용 빈 포트
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let Instance::First(l) = acquire_on(port) else { panic!("처음 실행이어야 함") };
        let shown = Arc::new(AtomicUsize::new(0));
        let s = shown.clone();
        listen(l, move || {
            s.fetch_add(1, Ordering::SeqCst);
        });
        assert!(matches!(acquire_on(port), Instance::AlreadyRunning));
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(shown.load(Ordering::SeqCst), 1);
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
