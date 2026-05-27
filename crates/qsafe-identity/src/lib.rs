//! qsafe-identity — X25519 + ML-KEM-768 하이브리드 공개키 수신자.
//!
//! ## 패턴: PQ Hybrid KEM (X-Wing 영감)
//!
//! 사용자는 장기 identity 키쌍을 가짐:
//!   (X25519_sk, X25519_pk)        — 고전 ECDH (검증된 32년)
//!   (MLKEM-768_sk, MLKEM-768_pk)  — 양자 안전 (FIPS 203, 2024)
//!
//! 다른 사람에게 파일 보낼 때:
//!   1. 발신자: ephemeral X25519 키쌍 생성
//!   2. ECDH로 X25519 shared = X25519(eph_sk, recipient_X25519_pk)
//!   3. ML-KEM 캡슐화: (ct, MLKEM_shared) = ML-KEM.Encap(recipient_MLKEM_pk)
//!   4. wrap_key = HKDF(X25519_shared || MLKEM_shared || transcript, info)
//!   5. AEAD(wrap_key, nonce, file_key) → 헤더에 저장
//!
//! 받는 사람:
//!   1. X25519_shared = X25519(my_X25519_sk, eph_X25519_pk)
//!   2. MLKEM_shared  = ML-KEM.Decap(my_MLKEM_sk, ct)
//!   3. 같은 wrap_key 재도출 → file_key 복원
//!
//! ## 보안 성질
//!
//! - X25519가 깨져도 ML-KEM이 막음
//! - ML-KEM이 깨져도 X25519가 막음
//! - 둘 다 깨지려면 양자컴 + 새 수학적 돌파 둘 다 필요
//! - **Forward Secrecy**: ephemeral 키 덕분에 매 파일 새 wrap_key
//!
//! ## 키 직렬화
//!
//! 키쌍은 qsafe-cli가 ~/.config/qsafe/identity.toml에 저장.
//! 단기 단순화: 평문 저장 (향후 OS 키링/패스워드 보호 추가).

pub mod error;
pub mod identity;
pub mod recipient;

pub use error::{IdentityError, Result};
pub use identity::{Identity, IdentityPublic};
pub use recipient::{unwrap_pubkey, PubkeyWrapper};

pub const HKDF_INFO_PQ_HYBRID_V1: &[u8] = b"qsafe-v1-pq-hybrid-pubkey-wrap-key";
