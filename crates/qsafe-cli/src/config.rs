//! qsafe 환경 설정 — 기본 패스워드 (OS 키링) + 사용자 설정.
//!
//! ## OS 키링 사용 (`keyring` crate)
//!
//! | OS | 백엔드 |
//! |----|--------|
//! | macOS | Keychain (login.keychain) |
//! | Windows | Credential Manager |
//! | Linux | Secret Service (gnome-keyring, KWallet 등) |
//!
//! 패스워드는 평문으로 저장되지 않고 OS 키링 (암호화 저장)에 위임.
//! 사용자가 OS에 로그인한 세션에서만 접근 가능.

use anyhow::{anyhow, Result};

const SERVICE: &str = "qsafe";
const ACCOUNT: &str = "default-password";

/// OS 키링에서 기본 패스워드 불러오기.
pub fn load_default_password() -> Result<Option<String>> {
    match keyring::Entry::new(SERVICE, ACCOUNT) {
        Ok(entry) => match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow!("키링 접근 실패: {}", e)),
        },
        Err(e) => Err(anyhow!("키링 엔트리 생성 실패: {}", e)),
    }
}

/// OS 키링에 기본 패스워드 저장.
pub fn save_default_password(password: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| anyhow!("키링 엔트리 생성: {}", e))?;
    entry
        .set_password(password)
        .map_err(|e| anyhow!("키링 저장: {}", e))?;
    Ok(())
}

/// OS 키링에서 기본 패스워드 삭제.
pub fn clear_default_password() -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| anyhow!("키링 엔트리: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!("키링 삭제: {}", e)),
    }
}

/// 기본 패스워드 존재 여부.
pub fn has_default_password() -> bool {
    load_default_password().ok().flatten().is_some()
}
