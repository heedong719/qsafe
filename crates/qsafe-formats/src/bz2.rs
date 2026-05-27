//! BZip2 (.bz2) 풀기 — bzip2-rs (순수 Rust).

use crate::error::{FormatError, Result};
use crate::path_safety::ensure_output_dir;
use bzip2_rs::DecoderReader;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;

pub fn extract_bz2(input: &Path, output_dir: &Path, _password: Option<&str>) -> Result<usize> {
    let out_base = ensure_output_dir(output_dir)?;
    let stem = input
        .file_stem()
        .ok_or_else(|| FormatError::InvalidInput("input has no stem".into()))?;
    let out_path = out_base.join(stem);

    let f = File::open(input).map_err(FormatError::Io)?;
    let mut decoder = DecoderReader::new(BufReader::new(f));
    let mut out = BufWriter::new(File::create(&out_path).map_err(FormatError::Io)?);

    let written =
        io::copy(&mut decoder, &mut out).map_err(|e| FormatError::Bzip2(e.to_string()))?;
    tracing::debug!(out = %out_path.display(), bytes = written, "bz2 extracted");
    Ok(1)
}
