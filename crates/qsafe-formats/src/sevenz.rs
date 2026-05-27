//! 7-Zip (.7z) 풀기/만들기 — sevenz-rust2 (순수 Rust!).

use crate::error::{FormatError, Result};
use crate::path_safety::ensure_output_dir;
use std::path::Path;

pub fn extract_7z(input: &Path, output_dir: &Path, password: Option<&str>) -> Result<usize> {
    let base = ensure_output_dir(output_dir)?;

    if let Some(pw) = password {
        let pwd = sevenz_rust2::Password::from(pw);
        sevenz_rust2::decompress_file_with_password(input, &base, pwd)
            .map_err(|e| FormatError::SevenZ(format!("{}", e)))?;
    } else {
        sevenz_rust2::decompress_file(input, &base)
            .map_err(|e| FormatError::SevenZ(format!("{}", e)))?;
    }

    // sevenz-rust2 자체가 경로 이탈 방어 (Path.IsAbsolute/.. 거부)
    // 추가 안전성: 추출 후 base 외부에 생성된 파일 검출은 매번 비효율, 라이브러리 신뢰
    // 향후 자체 walk으로 검증 가능

    // 추출된 파일 수 측정 (재귀 walk)
    let count = count_files(&base)?;
    tracing::debug!(base = %base.display(), files = count, "7z extracted");
    Ok(count)
}

fn count_files(dir: &Path) -> Result<usize> {
    let mut count = 0;
    for entry in std::fs::read_dir(dir).map_err(FormatError::Io)? {
        let entry = entry.map_err(FormatError::Io)?;
        let ft = entry.file_type().map_err(FormatError::Io)?;
        if ft.is_dir() {
            count += count_files(&entry.path())?;
        } else if ft.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

/// 디렉토리 → .7z 생성.
pub fn create_7z(input_dir: &Path, output: &Path) -> Result<()> {
    sevenz_rust2::compress_to_path(input_dir, output)
        .map_err(|e| FormatError::SevenZ(format!("{}", e)))?;
    Ok(())
}
