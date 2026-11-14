//! API 키를 설정 파일(평문 JSON) 대신 Windows 자격 증명 관리자에 보관한다.

const SERVICE: &str = "ai.exaone.desktop";
const ACCOUNT: &str = "exaone-api-key";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("자격 증명 저장소를 열 수 없습니다: {}", e))
}

/// 저장된 키. 없으면 `None`.
pub fn load_api_key() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("API 키를 읽지 못했습니다: {}", e)),
    }
}

pub fn store_api_key(key: &str) -> Result<(), String> {
    entry()?
        .set_password(key)
        .map_err(|e| format!("API 키를 Windows 자격 증명 관리자에 저장하지 못했습니다: {}", e))
}

pub fn delete_api_key() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("API 키를 삭제하지 못했습니다: {}", e)),
    }
}

/// MCP 서버의 환경 변수·헤더 값처럼 이름(account)별로 나눠 두는 비밀 값
fn named_entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| format!("자격 증명 저장소를 열 수 없습니다: {}", e))
}

pub fn load_secret(account: &str) -> Result<Option<String>, String> {
    match named_entry(account)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("저장된 값을 읽지 못했습니다: {}", e)),
    }
}

pub fn store_secret(account: &str, value: &str) -> Result<(), String> {
    named_entry(account)?
        .set_password(value)
        .map_err(|e| format!("값을 Windows 자격 증명 관리자에 저장하지 못했습니다: {}", e))
}

pub fn delete_secret(account: &str) -> Result<(), String> {
    match named_entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("저장된 값을 지우지 못했습니다: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    /// 실제 Windows 자격 증명 관리자를 쓰므로 직접 실행할 때만: cargo test -- --ignored
    #[test]
    #[ignore]
    fn credential_manager_round_trip() {
        let e = keyring::Entry::new(super::SERVICE, "exaone-selftest").unwrap();
        e.set_password("키-테스트-123").unwrap();
        assert_eq!(e.get_password().unwrap(), "키-테스트-123");
        e.delete_credential().unwrap();
        assert!(matches!(e.get_password(), Err(keyring::Error::NoEntry)));
    }
}
