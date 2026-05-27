//! Brotli (.br) 풀기/만들기 — 순수 Rust.

use crate::error::{FormatError, Result};
use crate::path_safety::ensure_output_dir;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

pub fn extract_brotli(input: &Path, output_dir: &Path, _password: Option<&str>) -> Result<usize> {
    let out_base = ensure_output_dir(output_dir)?;
    let stem = input
        .file_stem()
        .ok_or_else(|| FormatError::InvalidInput("input has no stem".into()))?;
    let out_path = out_base.join(stem);

    let f = File::open(input).map_err(FormatError::Io)?;
    let mut reader = BufReader::new(f);
    let f_out = File::create(&out_path).map_err(FormatError::Io)?;
    let mut writer = BufWriter::new(f_out);
    brotli::BrotliDecompress(&mut reader, &mut writer)
        .map_err(|e| FormatError::Brotli(format!("{:?}", e)))?;
    writer.flush().map_err(FormatError::Io)?;
    tracing::debug!(out = %out_path.display(), "brotli extracted");
    Ok(1)
}

pub fn create_brotli(input: &Path, output: &Path, quality: u32) -> Result<()> {
    let f_in = File::open(input).map_err(FormatError::Io)?;
    let mut reader = BufReader::new(f_in);
    let f_out = File::create(output).map_err(FormatError::Io)?;
    let mut writer = BufWriter::new(f_out);
    let mut params = brotli::enc::BrotliEncoderParams::default();
    params.quality = quality as i32;
    brotli::BrotliCompress(&mut reader, &mut writer, &params)
        .map_err(|e| FormatError::Brotli(format!("{:?}", e)))?;
    writer.flush().map_err(FormatError::Io)?;
    Ok(())
}
