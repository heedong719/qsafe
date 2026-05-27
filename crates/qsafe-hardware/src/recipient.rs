//! FIDO2 수신자 wrap/unwrap. 백엔드 추상화 위에서 동작.
//!
//! 흐름:
//!   wrap:   random salt → backend.evaluate(salt) → HKDF → wrap_key
//!           → AEAD(wrap_key, nonce, file_key) → Fido2Recipient
//!   unwrap: header의 salt → backend.evaluate(salt) → 같은 wrap_key
//!           → AEAD 역연산 → FileKey

use crate::backend::PrfBackend;
use crate::error::{HardwareError, Result};
use crate::{DEFAULT_RP_ID, HKDF_INFO_FIDO2_V1, HMAC_SALT_LEN};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use qsafe_core::envelope::{FileKey, FILE_KEY_LEN};
use qsafe_core::format::{Fido2Recipient, Recipient};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use zeroize::Zeroize;

const NONCE_LEN: usize = 24;
const WRAP_KEY_LEN: usize = 32;

/// FIDO2 봉투 빌더.
pub struct Fido2Wrapper<'b, B: PrfBackend> {
    backend: &'b B,
    credential_id: Vec<u8>,
    rp_id: String,
    user_verification_required: bool,
    label: Option<String>,
}

impl<'b, B: PrfBackend> Fido2Wrapper<'b, B> {
    pub fn new(backend: &'b B, credential_id: Vec<u8>) -> Self {
        Self {
            backend,
            credential_id,
            rp_id: DEFAULT_RP_ID.to_string(),
            user_verification_required: false,
            label: None,
        }
    }

    pub fn with_rp_id(mut self, rp_id: impl Into<String>) -> Self {
        self.rp_id = rp_id.into();
        self
    }

    pub fn with_uv(mut self, required: bool) -> Self {
        self.user_verification_required = required;
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// FileKey를 FIDO2로 봉투화.
    pub fn wrap(&self, file_key: &FileKey) -> Result<Recipient> {
        // 1. 무작위 salt 생성
        let mut salt = vec![0u8; HMAC_SALT_LEN];
        OsRng.fill_bytes(&mut salt);

        // 2. 백엔드로 PRF 평가 (실 하드웨어: Touch 필요)
        let prf = self.backend.evaluate(&self.credential_id, &salt)?;

        // 3. HKDF로 도메인 분리된 wrap_key 도출
        let mut wrap_key = derive_wrap_key(prf.as_bytes(), &salt)?;

        // 4. AEAD로 file_key 봉투화
        let mut nonce = vec![0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
        let xnonce = XNonce::from_slice(&nonce);
        let encrypted = cipher
            .encrypt(xnonce, file_key.as_bytes().as_ref())
            .map_err(|_| HardwareError::Aead)?;
        wrap_key.zeroize();

        Ok(Recipient::Fido2(Fido2Recipient {
            credential_id: self.credential_id.clone(),
            rp_id: self.rp_id.clone(),
            hmac_salt: salt,
            user_verification_required: self.user_verification_required,
            nonce,
            encrypted_file_key: encrypted,
            label: self.label.clone(),
        }))
    }
}

/// FIDO2 수신자에서 FileKey 복원. 백엔드를 통해 PRF 출력 얻음.
pub fn unwrap_fido2_with<B: PrfBackend>(
    backend: &B,
    recipient: &Fido2Recipient,
) -> Result<FileKey> {
    if recipient.hmac_salt.len() != HMAC_SALT_LEN {
        return Err(HardwareError::InvalidSaltLen {
            expected: HMAC_SALT_LEN,
            actual: recipient.hmac_salt.len(),
        });
    }
    if recipient.nonce.len() != NONCE_LEN {
        return Err(HardwareError::Aead);
    }

    // 1. 백엔드로 PRF 재평가
    let prf = backend.evaluate(&recipient.credential_id, &recipient.hmac_salt)?;

    // 2. 같은 HKDF로 wrap_key 도출
    let mut wrap_key = derive_wrap_key(prf.as_bytes(), &recipient.hmac_salt)?;

    // 3. AEAD 역연산
    let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
    let xnonce = XNonce::from_slice(&recipient.nonce);
    let result = cipher.decrypt(xnonce, recipient.encrypted_file_key.as_slice());
    wrap_key.zeroize();

    let mut plaintext = result.map_err(|_| HardwareError::Aead)?;
    if plaintext.len() != FILE_KEY_LEN {
        plaintext.zeroize();
        return Err(HardwareError::InvalidFileKey);
    }
    let mut bytes = [0u8; FILE_KEY_LEN];
    bytes.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(FileKey::from_bytes(bytes))
}

fn derive_wrap_key(prf_output: &[u8], salt: &[u8]) -> Result<[u8; WRAP_KEY_LEN]> {
    let hk = Hkdf::<Sha256>::new(Some(salt), prf_output);
    let mut out = [0u8; WRAP_KEY_LEN];
    hk.expand(HKDF_INFO_FIDO2_V1, &mut out)
        .map_err(|e| HardwareError::Hkdf(format!("expand: {}", e)))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockPrfBackend;

    #[test]
    fn wrap_unwrap_roundtrip_with_mock() {
        let backend = MockPrfBackend::new(42);
        let enrolled = backend.enroll(Some("test-key")).unwrap();

        let file_key = FileKey::random();
        let original = *file_key.as_bytes();

        let wrapper = Fido2Wrapper::new(&backend, enrolled.credential_id.clone())
            .with_label("test-key");
        let r = wrapper.wrap(&file_key).unwrap();

        let f2r = match r {
            Recipient::Fido2(f) => f,
            _ => panic!("expected Fido2 recipient"),
        };

        let recovered = unwrap_fido2_with(&backend, &f2r).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }

    #[test]
    fn wrong_device_fails() {
        let backend_a = MockPrfBackend::new(1);
        let backend_b = MockPrfBackend::new(2); // 다른 device
        let cred = backend_a.enroll(None).unwrap();

        let fk = FileKey::random();
        let r = Fido2Wrapper::new(&backend_a, cred.credential_id.clone())
            .wrap(&fk)
            .unwrap();
        let f2r = match r {
            Recipient::Fido2(f) => f,
            _ => panic!(),
        };

        // 다른 device로 풀면 실패해야 함
        assert!(unwrap_fido2_with(&backend_b, &f2r).is_err());
    }

    #[test]
    fn wrong_credential_id_fails() {
        let backend = MockPrfBackend::new(7);
        let cred1 = backend.enroll(None).unwrap();
        let cred2 = backend.enroll(None).unwrap();

        let fk = FileKey::random();
        let r = Fido2Wrapper::new(&backend, cred1.credential_id.clone())
            .wrap(&fk)
            .unwrap();
        let mut f2r = match r {
            Recipient::Fido2(f) => f,
            _ => panic!(),
        };

        // credential_id를 바꿔서 풀려 하면 다른 PRF 출력 → 다른 wrap_key → 실패
        f2r.credential_id = cred2.credential_id;
        assert!(unwrap_fido2_with(&backend, &f2r).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let backend = MockPrfBackend::new(11);
        let cred = backend.enroll(None).unwrap();
        let fk = FileKey::random();
        let r = Fido2Wrapper::new(&backend, cred.credential_id)
            .wrap(&fk)
            .unwrap();
        let mut f2r = match r {
            Recipient::Fido2(f) => f,
            _ => panic!(),
        };
        f2r.encrypted_file_key[0] ^= 1;
        assert!(unwrap_fido2_with(&backend, &f2r).is_err());
    }

    #[test]
    fn tampered_salt_fails() {
        let backend = MockPrfBackend::new(13);
        let cred = backend.enroll(None).unwrap();
        let fk = FileKey::random();
        let r = Fido2Wrapper::new(&backend, cred.credential_id)
            .wrap(&fk)
            .unwrap();
        let mut f2r = match r {
            Recipient::Fido2(f) => f,
            _ => panic!(),
        };
        f2r.hmac_salt[0] ^= 1;
        assert!(unwrap_fido2_with(&backend, &f2r).is_err());
    }

    #[test]
    fn invalid_salt_length_rejected() {
        let backend = MockPrfBackend::new(17);
        let cred = backend.enroll(None).unwrap();
        let fk = FileKey::random();
        let r = Fido2Wrapper::new(&backend, cred.credential_id)
            .wrap(&fk)
            .unwrap();
        let mut f2r = match r {
            Recipient::Fido2(f) => f,
            _ => panic!(),
        };
        f2r.hmac_salt = vec![0; 16]; // wrong length
        assert!(matches!(
            unwrap_fido2_with(&backend, &f2r),
            Err(HardwareError::InvalidSaltLen { .. })
        ));
    }
}
