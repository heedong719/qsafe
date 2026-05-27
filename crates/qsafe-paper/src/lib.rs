//! qsafe-paper — BIP39 24단어 종이 백업 수신자.
//!
//! ## 동작 원리
//!
//! ```text
//! 암호화:
//!   entropy (256 bit, 무작위) → BIP39.to_mnemonic() → 24개 영어 단어
//!   사용자에게 화면 표시 → 종이에 적어 보관 (이게 비밀)
//!
//!   seed = BIP39.to_seed(mnemonic, passphrase="")   // PBKDF2-HMAC-SHA512, 2048 iters, 64 bytes
//!   wrap_key = HKDF-SHA256(salt, seed, "qsafe-v1-bip39")
//!   AEAD(wrap_key, nonce, file_key) → 헤더에 저장
//!
//! 복구:
//!   사용자가 24단어 입력 → BIP39 체크섬 검증 (오타 즉시 거부)
//!   → seed 재도출 → wrap_key → file_key 복원
//! ```
//!
//! ## 보안 모델
//!
//! - 단어는 절대로 .cl 파일에 저장되지 않음. 종이만.
//! - 24단어 = 256 bit entropy ≈ 우주 수명에도 brute force 불가
//! - 한 단어 틀려도 BIP39 checksum이 즉시 검출
//! - PBKDF2 2048 iters는 BIP39 표준 (qsafe 추가 HKDF로 도메인 분리)

pub mod error;
pub mod recipient;

pub use error::{PaperError, Result};
pub use recipient::{display_words, unwrap_bip39, Bip39Wrapper, GeneratedMnemonic};

pub const HKDF_INFO_BIP39_V1: &[u8] = b"qsafe-v1-bip39-paper-wrap-key";
pub const BIP39_SALT_LEN: usize = 16;
pub const DEFAULT_WORD_COUNT: u8 = 24;
