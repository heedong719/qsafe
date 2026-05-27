use thiserror::Error;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid magic bytes (not a qsafe file)")]
    InvalidMagic,

    #[error("unsupported format version: {0}")]
    UnsupportedVersion(u16),

    #[error("header decode error: {0}")]
    HeaderDecode(String),

    #[error("header encode error: {0}")]
    HeaderEncode(String),

    #[error("compression failure: {0}")]
    Compression(String),

    #[error("integrity check failed")]
    IntegrityFailed,

    #[error("unsupported compression algorithm: {0:?}")]
    UnsupportedCompression(crate::format::CompressionAlgo),

    #[error("unsupported cipher suite: {0:?}")]
    UnsupportedCipherSuite(crate::format::CipherSuite),

    #[error("trailing bytes after file end (possibly tampered)")]
    TrailingBytes,

    #[error("file has no recipients (would be unrecoverable)")]
    EmptyRecipients,

    #[error("invalid header field: {0}")]
    InvalidHeaderField(&'static str),

    #[error("compression bomb detected (decompressed {got} > limit {limit})")]
    CompressionBomb { got: u64, limit: u64 },

    #[error("decompressed size mismatch (expected {expected}, got {got})")]
    SizeMismatch { expected: u64, got: u64 },
}
