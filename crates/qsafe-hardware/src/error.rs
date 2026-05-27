use thiserror::Error;

pub type Result<T> = std::result::Result<T, HardwareError>;

#[derive(Debug, Error)]
pub enum HardwareError {
    #[error("AEAD failure (wrong key or tampered)")]
    Aead,

    #[error("invalid salt length (expected {expected}, got {actual})")]
    InvalidSaltLen { expected: usize, actual: usize },

    #[error("invalid PRF output length")]
    InvalidPrfOutput,

    #[error("invalid file key length")]
    InvalidFileKey,

    #[error("HKDF expand error: {0}")]
    Hkdf(String),

    #[error("no FIDO2 device connected")]
    NoDevice,

    #[error("user cancelled (no touch)")]
    UserCancelled,

    #[error("user verification required (PIN/biometric)")]
    UvRequired,

    #[error("credential not found on device")]
    CredentialNotFound,

    #[error("FIDO2 backend error: {0}")]
    Backend(String),
}
