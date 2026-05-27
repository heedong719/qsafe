//! Password 수신자 — Argon2id KDF + XChaCha20-Poly1305 봉투.
//!
//! 흐름:
//!   wrap:   PW → Argon2id(salt) → wrap_key → AEAD(wrap_key, nonce, file_key)
//!           → PasswordRecipient 생성
//!   unwrap: PW + PasswordRecipient → wrap_key 재계산 → AEAD 역연산 → FileKey 복원
//!
//! wrap_key는 사용 직후 zeroize. 패스워드는 Builder Drop 시 zeroize.

use crate::error::{CryptoError, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use qsafe_core::envelope::{FileKey, FILE_KEY_LEN};
use qsafe_core::format::{PasswordRecipient, Recipient};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use zeroize::Zeroize;

/// HKDF 도메인 분리 라벨. 알고리즘이 바뀌면 새 라벨로 (절대 재사용 금지).
const HKDF_INFO_PASSWORD_V1: &[u8] = b"qsafe-v1-password-wrap-key";

/// 기본 Argon2id 매개변수 (RFC 9106 second recommended).
/// m=64 MiB, t=3, p=4 — 일반 사용자에게 ~1초.
pub const DEFAULT_M_KIB: u32 = 64 * 1024;
pub const DEFAULT_T: u32 = 3;
pub const DEFAULT_P: u32 = 4;

/// Strong 프로필 (Tier 2+).
pub const STRONG_M_KIB: u32 = 256 * 1024;
pub const STRONG_T: u32 = 4;
pub const STRONG_P: u32 = 4;

/// **보안 최소값** — 이보다 작은 매개변수는 거부.
/// 악의적 .qs 파일이 약한 매개변수로 brute force 활성화하려는 공격 방지.
/// 8 MiB / t=1 / p=1 은 단일 패스워드 시도에 ~100ms 비용 (정상 사용자엔 무관).
pub const MIN_M_KIB: u32 = 8 * 1024; // 8 MiB
pub const MIN_T: u32 = 1;
pub const MIN_P: u32 = 1;
/// 거대한 m으로 메모리 고갈 DoS 방지 (16 GiB 상한)
pub const MAX_M_KIB: u32 = 16 * 1024 * 1024;

pub const SALT_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const WRAP_KEY_LEN: usize = 32;

/// 패스워드 봉투 빌더.
pub struct PasswordWrapper {
    password: Vec<u8>,
    m_kib: u32,
    t: u32,
    p: u32,
}

impl PasswordWrapper {
    /// 기본 매개변수로 빌더 생성.
    pub fn new(password: &str) -> Self {
        Self {
            password: password.as_bytes().to_vec(),
            m_kib: DEFAULT_M_KIB,
            t: DEFAULT_T,
            p: DEFAULT_P,
        }
    }

    /// 사용자 정의 매개변수.
    pub fn with_params(mut self, m_kib: u32, t: u32, p: u32) -> Self {
        self.m_kib = m_kib;
        self.t = t;
        self.p = p;
        self
    }

    /// Strong 프로필 (Tier 2).
    pub fn strong(password: &str) -> Self {
        Self::new(password).with_params(STRONG_M_KIB, STRONG_T, STRONG_P)
    }

    /// FileKey를 패스워드로 봉투화하여 Recipient를 만든다.
    pub fn wrap(&self, file_key: &FileKey) -> Result<Recipient> {
        let mut salt = vec![0u8; SALT_LEN];
        let mut nonce = vec![0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce);

        let mut wrap_key = derive_key(&self.password, &salt, self.m_kib, self.t, self.p)?;
        let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
        let xnonce = XNonce::from_slice(&nonce);
        let encrypted = cipher
            .encrypt(xnonce, file_key.as_bytes().as_ref())
            .map_err(|_| CryptoError::Aead)?;
        wrap_key.zeroize();

        Ok(Recipient::Password(PasswordRecipient {
            salt,
            argon2_m_kib: self.m_kib,
            argon2_t: self.t,
            argon2_p: self.p,
            nonce,
            encrypted_file_key: encrypted,
        }))
    }
}

impl Drop for PasswordWrapper {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// PasswordRecipient에서 FileKey 복구.
pub fn unwrap_password(password: &str, recipient: &PasswordRecipient) -> Result<FileKey> {
    if recipient.salt.len() < 8 {
        return Err(CryptoError::InvalidParams("salt too short".into()));
    }
    if recipient.nonce.len() != NONCE_LEN {
        return Err(CryptoError::InvalidNonce);
    }

    // 보안: 매개변수 최소값 강제 (악의적 약한 KDF 거부)
    if recipient.argon2_m_kib < MIN_M_KIB {
        return Err(CryptoError::InvalidParams(format!(
            "Argon2 m={} below minimum {}",
            recipient.argon2_m_kib, MIN_M_KIB
        )));
    }
    if recipient.argon2_m_kib > MAX_M_KIB {
        return Err(CryptoError::InvalidParams(format!(
            "Argon2 m={} above max {} (DoS guard)",
            recipient.argon2_m_kib, MAX_M_KIB
        )));
    }
    if recipient.argon2_t < MIN_T {
        return Err(CryptoError::InvalidParams(format!(
            "Argon2 t={} below minimum {}",
            recipient.argon2_t, MIN_T
        )));
    }
    if recipient.argon2_p < MIN_P {
        return Err(CryptoError::InvalidParams(format!(
            "Argon2 p={} below minimum {}",
            recipient.argon2_p, MIN_P
        )));
    }

    let mut wrap_key = derive_key(
        password.as_bytes(),
        &recipient.salt,
        recipient.argon2_m_kib,
        recipient.argon2_t,
        recipient.argon2_p,
    )?;

    let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
    let xnonce = XNonce::from_slice(&recipient.nonce);
    let plaintext_result = cipher.decrypt(xnonce, recipient.encrypted_file_key.as_slice());
    wrap_key.zeroize();

    let mut plaintext = plaintext_result.map_err(|_| CryptoError::Aead)?;

    if plaintext.len() != FILE_KEY_LEN {
        plaintext.zeroize();
        return Err(CryptoError::InvalidParams("file key length mismatch".into()));
    }

    let mut file_key_bytes = [0u8; FILE_KEY_LEN];
    file_key_bytes.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(FileKey::from_bytes(file_key_bytes))
}

/// Argon2id → HKDF-SHA256 도메인 분리 → wrap_key.
///
/// 이렇게 두 단계로 분리하는 이유:
/// - Argon2id 출력은 일종의 "패스워드 등가물" — 다른 용도로 재사용 위험
/// - HKDF로 라벨 명시 → 같은 패스워드라도 다른 용도엔 다른 키 (도메인 분리)
///
/// `#[inline(never)]`: 컴파일러가 인라인화하여 zeroize를 최적화로 제거하는 것을 방지.
#[inline(never)]
fn derive_key(password: &[u8], salt: &[u8], m: u32, t: u32, p: u32) -> Result<[u8; WRAP_KEY_LEN]> {
    let params = Params::new(m, t, p, Some(WRAP_KEY_LEN))
        .map_err(|e| CryptoError::Kdf(format!("invalid argon2 params: {}", e)))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut argon_out = [0u8; WRAP_KEY_LEN];
    argon2
        .hash_password_into(password, salt, &mut argon_out)
        .map_err(|e| CryptoError::Kdf(e.to_string()))?;

    // HKDF 도메인 분리
    let hk = Hkdf::<Sha256>::new(Some(salt), &argon_out);
    let mut wrap_key = [0u8; WRAP_KEY_LEN];
    hk.expand(HKDF_INFO_PASSWORD_V1, &mut wrap_key)
        .map_err(|e| CryptoError::Kdf(format!("hkdf expand: {}", e)))?;

    // Argon2 출력 즉시 zeroize
    argon_out.zeroize();

    Ok(wrap_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsafe_core::format::Recipient;

    // 테스트용 빠른 매개변수
    const TEST_M: u32 = 8 * 1024;
    const TEST_T: u32 = 1;
    const TEST_P: u32 = 1;

    #[test]
    fn wrap_unwrap_roundtrip() {
        let file_key = FileKey::random();
        let original = *file_key.as_bytes();

        let w = PasswordWrapper::new("hunter2").with_params(TEST_M, TEST_T, TEST_P);
        let r = w.wrap(&file_key).unwrap();

        let pr = match r {
            Recipient::Password(p) => p,
            _ => panic!("expected Password recipient"),
        };

        let recovered = unwrap_password("hunter2", &pr).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }

    #[test]
    fn wrong_password_fails() {
        let file_key = FileKey::random();
        let w = PasswordWrapper::new("hunter2").with_params(TEST_M, TEST_T, TEST_P);
        let r = w.wrap(&file_key).unwrap();
        let pr = match r {
            Recipient::Password(p) => p,
            _ => panic!(),
        };
        assert!(unwrap_password("wrong-pw", &pr).is_err());
    }

    #[test]
    fn tampered_nonce_fails() {
        let file_key = FileKey::random();
        let w = PasswordWrapper::new("hunter2").with_params(TEST_M, TEST_T, TEST_P);
        let r = w.wrap(&file_key).unwrap();
        let mut pr = match r {
            Recipient::Password(p) => p,
            _ => panic!(),
        };
        pr.nonce[0] ^= 1;
        assert!(unwrap_password("hunter2", &pr).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let file_key = FileKey::random();
        let w = PasswordWrapper::new("hunter2").with_params(TEST_M, TEST_T, TEST_P);
        let r = w.wrap(&file_key).unwrap();
        let mut pr = match r {
            Recipient::Password(p) => p,
            _ => panic!(),
        };
        pr.encrypted_file_key[0] ^= 1;
        assert!(unwrap_password("hunter2", &pr).is_err());
    }

    #[test]
    fn invalid_nonce_length_rejected() {
        let mut pr = PasswordRecipient {
            salt: vec![0; 32],
            argon2_m_kib: TEST_M,
            argon2_t: TEST_T,
            argon2_p: TEST_P,
            nonce: vec![0; 12], // wrong length
            encrypted_file_key: vec![0; 48],
        };
        assert!(matches!(
            unwrap_password("pw", &pr),
            Err(CryptoError::InvalidNonce)
        ));
        pr.nonce = vec![0; NONCE_LEN]; // fix
    }
}
