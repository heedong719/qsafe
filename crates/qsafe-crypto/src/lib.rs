//! qsafe-crypto
//!
//! 다중 수신자 봉투의 각 수신자 구현. MVP에서는 Password 수신자만 포함.
//! 향후: FIDO2, BIP39, X25519+ML-KEM, Shamir 수신자.

pub mod error;

#[cfg(feature = "password")]
pub mod password;

pub use error::{CryptoError, Result};

#[cfg(feature = "password")]
pub use password::{unwrap_password, PasswordWrapper};
