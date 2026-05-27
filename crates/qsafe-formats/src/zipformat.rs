//! ZIP (.zip) 풀기/만들기.

use crate::error::{FormatError, Result};
use crate::path_safety::{ensure_output_dir, sanitize_archive_path};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn extract_zip(
    input: &Path,
    output_dir: &Path,
    password: Option<&str>,
) -> Result<usize> {
    let base = ensure_output_dir(output_dir)?;
    let f = File::open(input).map_err(FormatError::Io)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(f))
        .map_err(|e| FormatError::Zip(e.to_string()))?;

    let mut extracted = 0usize;
    for i in 0..archive.len() {
        let mut entry = if let Some(pw) = password {
            archive
                .by_index_decrypt(i, pw.as_bytes())
                .map_err(|e| FormatError::Zip(e.to_string()))?
        } else {
            archive
                .by_index(i)
                .map_err(|e| FormatError::Zip(e.to_string()))?
        };

        let name = entry.name().to_string();
        let safe_path = sanitize_archive_path(&name, &base)?;

        if entry.is_dir() {
            std::fs::create_dir_all(&safe_path).map_err(FormatError::Io)?;
        } else {
            if let Some(parent) = safe_path.parent() {
                std::fs::create_dir_all(parent).map_err(FormatError::Io)?;
            }
            let mut out = File::create(&safe_path).map_err(FormatError::Io)?;
            std::io::copy(&mut entry, &mut out).map_err(FormatError::Io)?;
            extracted += 1;
        }
    }
    tracing::debug!(base = %base.display(), files = extracted, "zip extracted");
    Ok(extracted)
}
