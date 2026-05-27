//! qsafe-core
//!
//! 압축, 무결성, 파일 포맷 v2 (다중 수신자 봉투) + 스트리밍 (대용량 파일).

pub mod compress;
pub mod envelope;
pub mod error;
pub mod format;
pub mod integrity;
pub mod io;
pub mod stream;

pub use error::{CoreError, Result};
pub use format::{
    Bip39Recipient, ChunkInfo, CipherSuite, CompressionAlgo, Fido2Recipient, FileHeader,
    IntegrityAlgo, PasswordRecipient, PubkeyRecipient, Recipient, ShamirCommitmentRecipient,
    SplitInfo, TimelockRecipient, MAGIC, VERSION,
};
