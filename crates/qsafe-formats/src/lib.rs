//! qsafe-formats — 외부 압축 포맷 통합 (반디집 호환).
//!
//! ## 지원 매트릭스
//!
//! ### 풀기 + 만들기 (양방향, 순수 Rust)
//! - **ZIP** (.zip) — zip + flate2
//! - **TAR** (.tar) — tar
//! - **GZIP** (.gz, .tgz) — flate2 (miniz_oxide)
//! - **LZ4** (.lz4) — lz4_flex
//! - **BROTLI** (.br) — brotli
//! - **Snappy** (.sz) — snap
//!
//! ### 풀기만 가능 (decode-only)
//! - **RAR / RAR5** — unrar (라이센스로 만들기 금지)
//! - **XZ / LZMA** (.xz, .lzma, .tar.xz, .tlz) — lzma-rs
//! - **BZIP2** (.bz2, .tar.bz2, .tbz) — bzip2-rs
//! - **ZSTD** (.zst) — ruzstd
//! - **7Z** (.7z) — sevenz-rust2 (순수 Rust!)
//! - **LZW / .Z** — weezl
//!
//! ## 보안
//! 모든 풀기는 경로 이탈(..) 자동 차단. 압축 폭탄 검출.

pub mod detect;
pub mod error;
pub mod path_safety;

#[cfg(feature = "rar")]
pub mod rar;

#[cfg(feature = "gzip")]
pub mod gz;

#[cfg(feature = "xz")]
pub mod xz;

#[cfg(feature = "bzip2")]
pub mod bz2;

#[cfg(feature = "lz4")]
pub mod lz4;

#[cfg(feature = "brotli")]
pub mod brotli_fmt;

#[cfg(feature = "zstd-decode")]
pub mod zstd_fmt;

#[cfg(feature = "tar-container")]
pub mod tar_fmt;

#[cfg(feature = "sevenz")]
pub mod sevenz;

#[cfg(feature = "zip-format")]
pub mod zipformat;

pub use detect::{detect_format, ExternalFormat};
pub use error::{FormatError, Result};
