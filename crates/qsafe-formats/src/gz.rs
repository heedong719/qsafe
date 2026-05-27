//! gzip (.gz) 풀기/만들기 — flate2 (miniz_oxide 순수 Rust 백엔드).

use crate::error::{FormatError, Result};
use crate::path_safety::ensure_output_dir;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

/// gzip 단일 파일 풀기. 출력은 <output_dir>/<basename(input)>.
/// (예: file.txt.gz → file.txt)
pub fn extract_gz(input: &Path, output_dir: &Path, _password: Option<&str>) -> Result<usize> {
    let out_base = ensure_output_dir(output_dir)?;

    let stem = input
        .file_stem()
        .ok_or_else(|| FormatError::InvalidInput("input has no stem".into()))?;
    let out_path = out_base.join(stem);

    let f = File::open(input).map_err(FormatError::Io)?;
    let mut decoder = GzDecoder::new(BufReader::new(f));
    let mut out = BufWriter::new(File::create(&out_path).map_err(FormatError::Io)?);

    let written = io::copy(&mut decoder, &mut out).map_err(|e| FormatError::Gzip(e.to_string()))?;
    out.flush().map_err(FormatError::Io)?;
    tracing::debug!(out = %out_path.display(), bytes = written, "gz extracted");
    Ok(1)
}

/// gzip 단일 파일 만들기.
pub fn create_gz(input: &Path, output: &Path, level: u32) -> Result<u64> {
    let f_in = File::open(input).map_err(FormatError::Io)?;
    let mut reader = BufReader::new(f_in);
    let f_out = File::create(output).map_err(FormatError::Io)?;
    let mut encoder = GzEncoder::new(BufWriter::new(f_out), Compression::new(level));
    let written =
        io::copy(&mut reader, &mut encoder).map_err(|e| FormatError::Gzip(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| FormatError::Gzip(e.to_string()))?;
    Ok(written)
}

/// 메모리 버퍼에서 gzip decode (작은 파일용).
pub fn decompress_buf(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::with_capacity(data.len() * 4);
    decoder
        .read_to_end(&mut out)
        .map_err(|e| FormatError::Gzip(e.to_string()))?;
    Ok(out)
}

/// 메모리 버퍼 gzip encode.
pub fn compress_buf(data: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder
        .write_all(data)
        .map_err(|e| FormatError::Gzip(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| FormatError::Gzip(e.to_string()))
}
