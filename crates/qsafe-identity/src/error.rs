use thiserror::Error;

pub type Result<T> = std::result::Result<T, IdentityError>;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("invalid X25519 public key length (expected 32, got {0})")]
    InvalidX25519PkLen(usize),

    #[error("invalid X25519 private key length (expected 32, got {0})")]
    InvalidX25519SkLen(usize),

    #[error("invalid ephemeral X25519 public key")]
    InvalidEphemeralKey,

    #[error("invalid ML-KEM-768 public key length")]
    InvalidMlkemPkLen,

    #[error("invalid ML-KEM-768 ciphertext length")]
    InvalidMlkemCtLen,

    #[error("invalid ML-KEM-768 private key")]
    InvalidMlkemSk,

    #[error("recipient mismatch (pubkey hash does not match identity)")]
    RecipientMismatch,

    #[error("ML-KEM decapsulation failure")]
    MlkemDecapFailed,

    #[error("AEAD failure (tampered or wrong identity)")]
    Aead,

    #[error("invalid file key length")]
    InvalidFileKey,

    #[error("HKDF expand error: {0}")]
    Hkdf(String),

    #[error("hex decode error: {0}")]
    Hex(String),
}
