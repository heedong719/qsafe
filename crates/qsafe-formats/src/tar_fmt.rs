//! TAR (.tar) 풀기/만들기 — 순수 Rust.

use crate::error::{FormatError, Result};
use crate::path_safety::{ensure_output_dir, sanitize_archive_path};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tar::Archive;

pub fn extract_tar(
    input: &Path,
    output_dir: &Path,
    _password: Option<&str>,
) -> Result<usize> {
    let base = ensure_output_dir(output_dir)?;
    let f = File::open(input).map_err(FormatError::Io)?;
    let mut archive = Archive::new(BufReader::new(f));

    let mut extracted = 0usize;
    for entry_result in archive.entries().map_err(|e| FormatError::Tar(e.to_string()))? {
        let mut entry = entry_result.map_err(|e| FormatError::Tar(e.to_string()))?;
        let path_in_archive = entry.path().map_err(|e| FormatError::Tar(e.to_string()))?;
        let name = path_in_archive.to_string_lossy().to_string();

        let safe_path = sanitize_archive_path(&name, &base)?;
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&safe_path).map_err(FormatError::Io)?;
        } else {
            if let Some(parent) = safe_path.parent() {
                std::fs::create_dir_all(parent).map_err(FormatError::Io)?;
            }
            entry.unpack(&safe_path).map_err(|e| FormatError::Tar(e.to_string()))?;
            extracted += 1;
        }
    }
    Ok(extracted)
}
