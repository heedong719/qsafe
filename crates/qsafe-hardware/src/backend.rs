//! PRF 백엔드 추상화.
//!
//! 실제 FIDO2 하드웨어와 테스트용 mock을 같은 인터페이스로 다룬다.
//! 이렇게 분리하면:
//! - wrap/unwrap 로직을 하드웨어 없이도 테스트 가능
//! - 향후 다른 PRF 소스 (TPM, Secure Enclave 등) 쉽게 추가
//! - CI에서 자동 테스트 가능

use crate::error::{HardwareError, Result};
use crate::{HMAC_SALT_LEN, PRF_OUTPUT_LEN};
use zeroize::Zeroize;

/// FIDO2 PRF 출력 — 32 bytes. Drop 시 zeroize.
pub struct PrfOutput([u8; PRF_OUTPUT_LEN]);

impl PrfOutput {
    pub fn new(bytes: [u8; PRF_OUTPUT_LEN]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; PRF_OUTPUT_LEN] {
        &self.0
    }
}

impl Drop for PrfOutput {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// PRF 백엔드 인터페이스. 구현체는 두 가지:
///   1. `hw::Fido2HwBackend` — 실 USB HID FIDO2 키
///   2. `MockPrfBackend` — 결정론적 HMAC, 테스트 전용
pub trait PrfBackend {
    /// salt와 credential_id로 PRF 출력을 얻는다.
    ///
    /// 실 하드웨어:
    ///   - 사용자에게 Touch 요구
    ///   - UV 필요 시 PIN/지문 prompt
    ///   - 출력 = HMAC-SHA256(device_secret_for_credential, salt)
    ///
    /// mock:
    ///   - 결정론적 HMAC-SHA256(mock_secret, credential_id || salt)
    fn evaluate(&self, credential_id: &[u8], salt: &[u8]) -> Result<PrfOutput>;

    /// 백엔드가 supported credential id를 등록.
    /// 실 하드웨어에서는 MakeCredential을 호출하여 credential_id 발급.
    /// mock에서는 단순 식별자 생성.
    fn enroll(&self, label: Option<&str>) -> Result<EnrolledCredential>;
}

/// 등록 결과.
#[derive(Debug, Clone)]
pub struct EnrolledCredential {
    pub credential_id: Vec<u8>,
    pub label: Option<String>,
}

// ─── Mock 백엔드 ─────────────────────────────────────────────

/// 테스트용 결정론적 PRF — 실 하드웨어 시뮬레이션.
///
/// **이 백엔드는 실제 보안 X**. 비밀 키가 메모리에 있고 단순 HMAC.
/// 오직 wrap/unwrap 로직 검증용.
pub struct MockPrfBackend {
    /// "device secret" — 실제로는 키 안에 있을 것
    pub mock_device_secret: [u8; 32],
}

impl MockPrfBackend {
    pub fn new(seed: u64) -> Self {
        // 결정론적 시드로 secret 생성 (테스트 재현성)
        let mut secret = [0u8; 32];
        let bytes = seed.to_le_bytes();
        for (i, b) in secret.iter_mut().enumerate() {
            *b = bytes[i % 8] ^ (i as u8);
        }
        Self {
            mock_device_secret: secret,
        }
    }
}

impl PrfBackend for MockPrfBackend {
    fn evaluate(&self, credential_id: &[u8], salt: &[u8]) -> Result<PrfOutput> {
        if salt.len() != HMAC_SALT_LEN {
            return Err(HardwareError::InvalidSaltLen {
                expected: HMAC_SALT_LEN,
                actual: salt.len(),
            });
        }
        // 실제 FIDO2와 다르게, credential_id를 입력에 포함시켜
        // credential별로 다른 출력 시뮬레이션.
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(&self.mock_device_secret).expect("HMAC key valid");
        mac.update(credential_id);
        mac.update(salt);
        let result = mac.finalize().into_bytes();
        let mut out = [0u8; PRF_OUTPUT_LEN];
        out.copy_from_slice(&result);
        Ok(PrfOutput::new(out))
    }

    fn enroll(&self, label: Option<&str>) -> Result<EnrolledCredential> {
        // mock에서는 label + 결정론적 식별자
        use rand::RngCore;
        let mut id = vec![0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut id);
        Ok(EnrolledCredential {
            credential_id: id,
            label: label.map(String::from),
        })
    }
}
