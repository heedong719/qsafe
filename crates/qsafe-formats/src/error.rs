use thiserror::Error;

pub type Result<T> = std::result::Result<T, FormatError>;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("unsupported format")]
    Unsupported,

    #[error("not built with feature: {0}")]
    FeatureDisabled(&'static str),

    #[error("invalid path traversal in archive entry: {0}")]
    PathTraversal(String),

    #[error("RAR error: {0}")]
    Rar(String),

    #[error("ZIP error: {0}")]
    Zip(String),

    #[error("7Z error: {0}")]
    SevenZ(String),

    #[error("TAR error: {0}")]
    Tar(String),

    #[error("gzip error: {0}")]
    Gzip(String),

    #[error("xz error: {0}")]
    Xz(String),

    #[error("bzip2 error: {0}")]
    Bzip2(String),

    #[error("lz4 error: {0}")]
    Lz4(String),

    #[error("brotli error: {0}")]
    Brotli(String),

    #[error("zstd error: {0}")]
    Zstd(String),

    #[error("password required")]
    PasswordRequired,

    #[error("incorrect password")]
    IncorrectPassword,

    #[error("archive is empty")]
    EmptyArchive,

    #[error("zip bomb detected (ratio {ratio} > limit {limit})")]
    BombDetected { ratio: u64, limit: u64 },

    #[error("invalid input: {0}")]
    InvalidInput(String),
}
