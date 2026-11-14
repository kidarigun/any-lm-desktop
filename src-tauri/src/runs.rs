//! 대화별 실행 관리.
//!
//! 대화마다 따로 답변을 생성할 수 있도록 실행 중인 대화와 그 중지 플래그를 기록한다.
//! 한 대화에서는 한 번에 하나만 실행하고, 서로 다른 대화는 동시에 실행할 수 있다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const ALREADY_RUNNING: &str =
    "이 대화는 아직 답변을 작성하고 있습니다. 끝나거나 중지한 뒤에 다시 보내 주세요.";

#[derive(Default)]
pub struct RunRegistry {
    runs: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

/// 실행이 끝나면(정상·오류·중지 모두) 등록을 지운다.
pub struct RunGuard {
    runs: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    conversation_id: String,
    flag: Arc<AtomicBool>,
}

impl RunGuard {
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.flag.clone()
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Ok(mut runs) = self.runs.lock() {
            // 같은 대화의 다음 실행이 이미 등록됐다면 지우지 않는다.
            if runs.get(&self.conversation_id).is_some_and(|f| Arc::ptr_eq(f, &self.flag)) {
                runs.remove(&self.conversation_id);
            }
        }
    }
}

impl RunRegistry {
    /// 대화의 실행을 시작한다. 이미 실행 중이면 오류.
    pub fn begin(&self, conversation_id: &str) -> Result<RunGuard, String> {
        let mut runs = self.runs.lock().map_err(|e| e.to_string())?;
        if runs.contains_key(conversation_id) {
            return Err(ALREADY_RUNNING.to_string());
        }
        let flag = Arc::new(AtomicBool::new(false));
        runs.insert(conversation_id.to_string(), flag.clone());
        Ok(RunGuard { runs: self.runs.clone(), conversation_id: conversation_id.to_string(), flag })
    }

    /// 한 대화의 실행을 멈춘다. 실행 중이 아니면 false.
    pub fn cancel(&self, conversation_id: &str) -> bool {
        match self.runs.lock() {
            Ok(runs) => match runs.get(conversation_id) {
                Some(flag) => {
                    flag.store(true, Ordering::Relaxed);
                    true
                }
                None => false,
            },
            Err(_) => false,
        }
    }

    pub fn cancel_all(&self) {
        if let Ok(runs) = self.runs.lock() {
            for flag in runs.values() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }

    /// 지금 실행 중인 대화 ID들
    pub fn running(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.runs.lock().map(|r| r.keys().cloned().collect()).unwrap_or_default();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn different_conversations_run_at_the_same_time() {
        let reg = RunRegistry::default();
        let a = reg.begin("a").unwrap();
        let b = reg.begin("b").unwrap();
        assert_eq!(reg.running(), vec!["a", "b"]);
        assert_eq!(reg.begin("a").err().as_deref(), Some(ALREADY_RUNNING));

        // 한 대화를 멈춰도 다른 대화는 계속된다.
        assert!(reg.cancel("a"));
        assert!(a.cancel_flag().load(Ordering::Relaxed));
        assert!(!b.cancel_flag().load(Ordering::Relaxed));

        drop(a);
        assert_eq!(reg.running(), vec!["b"]);
        assert!(!reg.cancel("a"));
        let a2 = reg.begin("a").unwrap();
        assert!(!a2.cancel_flag().load(Ordering::Relaxed), "새 실행은 이전 중지의 영향을 받지 않는다");

        reg.cancel_all();
        assert!(a2.cancel_flag().load(Ordering::Relaxed) && b.cancel_flag().load(Ordering::Relaxed));
        drop(a2);
        drop(b);
        assert!(reg.running().is_empty());
    }

    #[test]
    fn guard_is_released_even_when_the_run_fails() {
        let reg = RunRegistry::default();
        let run = |fail: bool| -> Result<(), String> {
            let _guard = reg.begin("c")?;
            if fail {
                return Err("API 오류".into());
            }
            Ok(())
        };
        assert!(run(true).is_err());
        assert!(reg.running().is_empty());
        assert!(run(false).is_ok());
    }
}
