//! LZ4 (.lz4) 풀기/만들기 — lz4_flex (순수 Rust, frame format).

use crate::error::{FormatError, Result};
use crate::path_safety::ensure_output_dir;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;

pub fn extract_lz4(input: &Path, output_dir: &Path, _password: Option<&str>) -> Result<usize> {
    let out_base = ensure_output_dir(output_dir)?;
    let stem = input
        .file_stem()
        .ok_or_else(|| FormatError::InvalidInput("input has no stem".into()))?;
    let out_path = out_base.join(stem);

    let f = File::open(input).map_err(FormatError::Io)?;
    let mut decoder = lz4_flex::frame::FrameDecoder::new(BufReader::new(f));
    let mut out = BufWriter::new(File::create(&out_path).map_err(FormatError::Io)?);

    let written = io::copy(&mut decoder, &mut out).map_err(|e| FormatError::Lz4(e.to_string()))?;
    tracing::debug!(out = %out_path.display(), bytes = written, "lz4 extracted");
    Ok(1)
}

pub fn create_lz4(input: &Path, output: &Path) -> Result<u64> {
    let f_in = File::open(input).map_err(FormatError::Io)?;
    let mut reader = BufReader::new(f_in);
    let f_out = File::create(output).map_err(FormatError::Io)?;
    let mut encoder = lz4_flex::frame::FrameEncoder::new(BufWriter::new(f_out));
    let written =
        io::copy(&mut reader, &mut encoder).map_err(|e| FormatError::Lz4(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| FormatError::Lz4(e.to_string()))?;
    Ok(written)
}
