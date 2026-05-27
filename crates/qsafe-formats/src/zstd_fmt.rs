//! Zstd (.zst) 풀기 — ruzstd (순수 Rust decoder).

use crate::error::{FormatError, Result};
use crate::path_safety::ensure_output_dir;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read};
use std::path::Path;

pub fn extract_zstd(input: &Path, output_dir: &Path, _password: Option<&str>) -> Result<usize> {
    let out_base = ensure_output_dir(output_dir)?;
    let stem = input
        .file_stem()
        .ok_or_else(|| FormatError::InvalidInput("input has no stem".into()))?;
    let out_path = out_base.join(stem);

    let f = File::open(input).map_err(FormatError::Io)?;
    let reader = BufReader::new(f);
    let mut decoder = ruzstd::streaming_decoder::StreamingDecoder::new(reader)
        .map_err(|e| FormatError::Zstd(format!("{:?}", e)))?;
    let mut out = BufWriter::new(File::create(&out_path).map_err(FormatError::Io)?);

    let written = io::copy(&mut decoder, &mut out).map_err(|e| FormatError::Zstd(e.to_string()))?;
    tracing::debug!(out = %out_path.display(), bytes = written, "zst extracted");
    Ok(1)
}

pub fn decompress_buf(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ruzstd::streaming_decoder::StreamingDecoder::new(data)
        .map_err(|e| FormatError::Zstd(format!("{:?}", e)))?;
    let mut out = Vec::with_capacity(data.len() * 4);
    decoder
        .read_to_end(&mut out)
        .map_err(|e| FormatError::Zstd(e.to_string()))?;
    Ok(out)
}
