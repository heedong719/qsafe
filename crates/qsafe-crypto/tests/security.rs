//! 보안 검증 통합 테스트 — Critical 갭 수정 사항 회귀 방지.

use qsafe_core::envelope::FileKey;
use qsafe_core::format::{PasswordRecipient, Recipient};
use qsafe_crypto::{unwrap_password, PasswordWrapper};

const TEST_M: u32 = 8 * 1024;
const TEST_T: u32 = 1;
const TEST_P: u32 = 1;

fn make_recipient() -> PasswordRecipient {
    let fk = FileKey::random();
    let w = PasswordWrapper::new("good-pw").with_params(TEST_M, TEST_T, TEST_P);
    match w.wrap(&fk).unwrap() {
        Recipient::Password(p) => p,
        _ => panic!(),
    }
}

#[test]
fn rejects_weak_argon2_m_below_minimum() {
    let mut pr = make_recipient();
    pr.argon2_m_kib = 1; // 1 KiB — 너무 약함
    assert!(unwrap_password("good-pw", &pr).is_err());
}

#[test]
fn rejects_argon2_m_zero() {
    let mut pr = make_recipient();
    pr.argon2_m_kib = 0;
    assert!(unwrap_password("good-pw", &pr).is_err());
}

#[test]
fn rejects_argon2_m_too_large_dos_guard() {
    let mut pr = make_recipient();
    pr.argon2_m_kib = u32::MAX; // 4 TiB — DoS 시도
    assert!(unwrap_password("good-pw", &pr).is_err());
}

#[test]
fn rejects_argon2_t_zero() {
    let mut pr = make_recipient();
    pr.argon2_t = 0;
    assert!(unwrap_password("good-pw", &pr).is_err());
}

#[test]
fn rejects_argon2_p_zero() {
    let mut pr = make_recipient();
    pr.argon2_p = 0;
    assert!(unwrap_password("good-pw", &pr).is_err());
}

#[test]
fn accepts_minimum_params() {
    // MIN_M_KIB = 8 * 1024 (정확히 경계)
    let pr = make_recipient();
    assert_eq!(pr.argon2_m_kib, 8 * 1024);
    assert!(unwrap_password("good-pw", &pr).is_ok());
}
