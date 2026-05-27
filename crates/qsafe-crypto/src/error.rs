use thiserror::Error;

pub type Result<T> = std::result::Result<T, CryptoError>;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("kdf error: {0}")]
    Kdf(String),
    #[error("aead error")]
    Aead,
    #[error("invalid nonce length")]
    InvalidNonce,
    #[error("invalid params: {0}")]
    InvalidParams(String),
}
