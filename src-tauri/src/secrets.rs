//! 로컬 LLM 공급자 API 토큰을 설정 파일(평문 JSON) 대신 Windows 자격 증명 관리자에 보관한다.

use crate::store::LlmProvider;

pub const SERVICE: &str = "io.github.kidarigun.anylm-desktop";
const ACCOUNT: &str = "lm-studio-api-token";
const LEGACY_ACCOUNT: &str = "exaone-api-key";

/// 저장된 LM Studio 키. 없으면 `None`.
pub fn load_api_key() -> Result<Option<String>, String> {
    let current = load_secret(ACCOUNT)?;
    if current.is_some() {
        return Ok(current);
    }
    let legacy = load_secret(LEGACY_ACCOUNT)?;
    if let Some(token) = &legacy {
        if let Err(e) = store_api_key(token) {
            eprintln!("기존 LM Studio 토큰을 새 저장 이름으로 복사하지 못했습니다: {e}");
        }
    }
    Ok(legacy)
}

pub fn store_api_key(key: &str) -> Result<(), String> {
    store_secret(ACCOUNT, key)
}

pub fn delete_api_key() -> Result<(), String> {
    // 사용자가 삭제하면 이전 이름의 토큰까지 지워야 다시 불러오지 않는다.
    delete_secret(LEGACY_ACCOUNT)?;
    delete_secret(ACCOUNT)
}

pub fn load_provider_api_key(provider: LlmProvider) -> Result<Option<String>, String> {
    match provider {
        LlmProvider::Lmstudio => load_api_key(),
        _ => load_secret(provider.secret_account()),
    }
}

pub fn store_provider_api_key(provider: LlmProvider, key: &str) -> Result<(), String> {
    match provider {
        LlmProvider::Lmstudio => store_api_key(key),
        _ => store_secret(provider.secret_account(), key),
    }
}

pub fn delete_provider_api_key(provider: LlmProvider) -> Result<(), String> {
    match provider {
        LlmProvider::Lmstudio => delete_api_key(),
        _ => delete_secret(provider.secret_account()),
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
        let e = keyring::Entry::new(super::SERVICE, "anylm-selftest").unwrap();
        e.set_password("키-테스트-123").unwrap();
        assert_eq!(e.get_password().unwrap(), "키-테스트-123");
        e.delete_credential().unwrap();
        assert!(matches!(e.get_password(), Err(keyring::Error::NoEntry)));
    }
}
