use thiserror::Error;

pub type Result<T> = std::result::Result<T, PaperError>;

#[derive(Debug, Error)]
pub enum PaperError {
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),

    #[error("unsupported word count: {0} (allowed: 12, 15, 18, 21, 24)")]
    InvalidWordCount(u8),

    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("AEAD failure (wrong words or tampered)")]
    Aead,

    #[error("invalid salt length")]
    InvalidSalt,

    #[error("invalid nonce length")]
    InvalidNonce,

    #[error("invalid file key length")]
    InvalidFileKey,

    #[error("HKDF expand error: {0}")]
    Hkdf(String),
}
