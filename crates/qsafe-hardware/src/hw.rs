//! 실 FIDO2 USB HID 하드웨어 통합 (ctap-hid-fido2 v2.2.x).
//!
//! 빌드: `cargo build --features fido2-hw`
//!
//! 작동 흐름:
//!   enroll:  make_credential_with_args + Extension::HmacSecret(Some(true))
//!            → verify_attestation → credential_id 발급
//!   evaluate: get_assertion_with_args + Extension::HmacSecret(Some(salt_32))
//!            → assertion.extensions에서 HmacSecret 출력 추출
//!
//! 사용자 경험:
//!   - 키 꽂으면 자동 감지 (1개 연결 가정)
//!   - PIN 설정된 키는 PIN 매개변수
//!   - 매 호출마다 Touch 필요 (CTAP 표준)

use crate::backend::{EnrolledCredential, PrfBackend, PrfOutput};
use crate::error::{HardwareError, Result};
use crate::HMAC_SALT_LEN;
use ctap_hid_fido2::{
    get_assertion_params::Extension as Gext, get_fidokey_devices,
    make_credential_params::Extension as Mext, verifier, Cfg, GetAssertionArgsBuilder,
    MakeCredentialArgsBuilder,
};

/// 실 USB HID FIDO2 백엔드.
pub struct Fido2HwBackend {
    rp_id: String,
    pin: Option<String>,
}

impl Fido2HwBackend {
    pub fn new(rp_id: impl Into<String>) -> Self {
        Self {
            rp_id: rp_id.into(),
            pin: None,
        }
    }

    /// PIN 설정 (UV 필요한 키에서).
    pub fn with_pin(mut self, pin: impl Into<String>) -> Self {
        self.pin = Some(pin.into());
        self
    }

    /// 연결된 FIDO2 기기 개수.
    pub fn device_count() -> usize {
        get_fidokey_devices().len()
    }

    fn check_device() -> Result<()> {
        let devices = get_fidokey_devices();
        if devices.is_empty() {
            return Err(HardwareError::NoDevice);
        }
        Ok(())
    }
}

impl PrfBackend for Fido2HwBackend {
    fn evaluate(&self, credential_id: &[u8], salt: &[u8]) -> Result<PrfOutput> {
        if salt.len() != HMAC_SALT_LEN {
            return Err(HardwareError::InvalidSaltLen {
                expected: HMAC_SALT_LEN,
                actual: salt.len(),
            });
        }
        Self::check_device()?;

        // salt를 [u8; 32]로
        let salt_arr: [u8; 32] = salt.try_into().map_err(|_| HardwareError::InvalidSaltLen {
            expected: HMAC_SALT_LEN,
            actual: salt.len(),
        })?;

        let cfg = Cfg::init();
        let challenge = verifier::create_challenge();
        let ext = Gext::HmacSecret(Some(salt_arr));

        let mut builder = GetAssertionArgsBuilder::new(&self.rp_id, &challenge);
        if let Some(pin) = self.pin.as_deref() {
            builder = builder.pin(pin);
        }
        builder = builder.credential_id(credential_id).extensions(&[ext]);
        let args = builder.build();

        let assertions = ctap_hid_fido2::get_assertion_with_args(&cfg, &args)
            .map_err(|e| HardwareError::Backend(format!("get_assertion: {}", e)))?;

        if assertions.is_empty() {
            return Err(HardwareError::CredentialNotFound);
        }

        let prf_bytes = assertions[0]
            .extensions
            .iter()
            .find_map(|e| match e {
                Gext::HmacSecret(Some(v)) => Some(*v),
                _ => None,
            })
            .ok_or(HardwareError::InvalidPrfOutput)?;

        // [u8; 32]를 PrfOutput으로
        Ok(PrfOutput::new(prf_bytes))
    }

    fn enroll(&self, label: Option<&str>) -> Result<EnrolledCredential> {
        Self::check_device()?;

        let cfg = Cfg::init();
        let challenge = verifier::create_challenge();
        let ext = Mext::HmacSecret(Some(true));

        let mut builder = MakeCredentialArgsBuilder::new(&self.rp_id, &challenge);
        if let Some(pin) = self.pin.as_deref() {
            builder = builder.pin(pin);
        }
        let args = builder.extensions(&[ext]).build();

        let attestation = ctap_hid_fido2::make_credential_with_args(&cfg, &args)
            .map_err(|e| HardwareError::Backend(format!("make_credential: {}", e)))?;

        let verify_result = verifier::verify_attestation(&self.rp_id, &challenge, &attestation);
        if !verify_result.is_success {
            return Err(HardwareError::Backend(
                "attestation verification failed".into(),
            ));
        }

        Ok(EnrolledCredential {
            credential_id: verify_result.credential_id,
            label: label.map(String::from),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_device_returns_error() {
        // 이 테스트는 하드웨어 없는 환경에서 동작 확인.
        // 만약 실 키 꽂혀 있으면 enroll/evaluate가 사용자 touch 요구하여 행 → 스킵.
        if Fido2HwBackend::device_count() > 0 {
            eprintln!("skipping: real FIDO2 device connected");
            return;
        }
        let backend = Fido2HwBackend::new("qsafe.test");
        let result = backend.enroll(Some("test"));
        assert!(matches!(result, Err(HardwareError::NoDevice)));
    }
}
