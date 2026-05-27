//! qsafe-hardware — FIDO2 PRF 기반 하드웨어 키 수신자.
//!
//! ## 아키텍처
//!
//! PRF 백엔드를 추상화 (PrfBackend trait):
//!   - 실 하드웨어: `fido2-hw` 피쳐로 ctap-hid-fido2 사용
//!   - 테스트용 mock: 결정론적 HMAC-SHA256 (PRF 시뮬레이션)
//!
//! 이렇게 분리하면 하드웨어 없이도 wrap/unwrap 로직을 검증 가능.
//!
//! ## 보안 모델
//!
//! - 키 내부 secret은 **절대로** qsafe에 노출되지 않음
//! - qsafe은 salt를 보내고 HMAC 출력만 받음 (32 bytes)
//! - 그 출력은 HKDF로 도메인 분리 후 wrap_key가 됨
//! - 키 분실 = 영구 봉인 (백업 키 또는 다른 Recipient로 복구 권장)

pub mod backend;
pub mod error;
pub mod recipient;

#[cfg(feature = "fido2-hw")]
pub mod hw;

pub use backend::{MockPrfBackend, PrfBackend, PrfOutput};
pub use error::{HardwareError, Result};
pub use recipient::{Fido2Wrapper, unwrap_fido2_with};

/// qsafe의 FIDO2 RP ID. 모든 .cl 파일은 이 RP로 등록된 credential 사용.
pub const DEFAULT_RP_ID: &str = "qsafe.local";

/// HKDF 도메인 분리 라벨. 알고리즘 변경 시 새 라벨로.
pub const HKDF_INFO_FIDO2_V1: &[u8] = b"qsafe-v1-fido2-prf-wrap-key";

/// FIDO2 hmac-secret salt 크기 (CTAP 표준).
pub const HMAC_SALT_LEN: usize = 32;

/// FIDO2 hmac-secret 출력 크기.
pub const PRF_OUTPUT_LEN: usize = 32;
