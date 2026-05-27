use thiserror::Error;

pub type Result<T> = std::result::Result<T, ShamirError>;

#[derive(Debug, Error)]
pub enum ShamirError {
    #[error("invalid threshold: M={m}, N={n} (need 2 ≤ M ≤ N ≤ 255)")]
    InvalidThreshold { m: u8, n: u8 },

    #[error("not enough shares: have {got}, need at least {need}")]
    NotEnoughShares { got: usize, need: usize },

    #[error("invalid share encoding: {0}")]
    InvalidShare(String),

    #[error("inconsistent shares (different secret? wrong set?)")]
    InconsistentShares,

    #[error("recovery failed: {0}")]
    RecoveryFailed(String),

    #[error("hex decode: {0}")]
    Hex(String),
}
