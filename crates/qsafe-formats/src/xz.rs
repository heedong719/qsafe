//! XZ / LZMA (.xz, .lzma) 풀기 — lzma-rs (순수 Rust).
//!
//! lzma-rs는 decode 완전 지원, encode는 부분만. 만들기는 향후 보강.

use crate::error::{FormatError, Result};
use crate::path_safety::ensure_output_dir;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

pub fn extract_xz(
    input: &Path,
    output_dir: &Path,
    _password: Option<&str>,
) -> Result<usize> {
    let out_base = ensure_output_dir(output_dir)?;
    let stem = input
        .file_stem()
        .ok_or_else(|| FormatError::InvalidInput("input has no stem".into()))?;
    let out_path = out_base.join(stem);

    let f = File::open(input).map_err(FormatError::Io)?;
    let mut reader = BufReader::new(f);
    let mut out_file = File::create(&out_path).map_err(FormatError::Io)?;
    let mut writer = BufWriter::new(&mut out_file);

    lzma_rs::xz_decompress(&mut reader, &mut writer)
        .map_err(|e| FormatError::Xz(format!("{}", e)))?;

    tracing::debug!(out = %out_path.display(), "xz extracted");
    Ok(1)
}

pub fn decompress_buf(data: &[u8]) -> Result<Vec<u8>> {
    let mut input = std::io::BufReader::new(data);
    let mut out = Vec::new();
    lzma_rs::xz_decompress(&mut input, &mut out)
        .map_err(|e| FormatError::Xz(format!("{}", e)))?;
    Ok(out)
}
