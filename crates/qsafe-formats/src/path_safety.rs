//! 아카이브 경로 이탈 (Zip Slip / RAR Slip) 방어 공통 모듈.

use crate::error::{FormatError, Result};
use std::path::{Component, Path, PathBuf};

/// 아카이브 내부 경로를 base 디렉토리 내로 정규화.
/// ../ 절대경로 차단. 모든 archive extractor가 사용해야 함.
pub fn sanitize_archive_path(entry_name: &str, base_canon: &Path) -> Result<PathBuf> {
    if entry_name.is_empty() {
        return Err(FormatError::PathTraversal("(empty)".into()));
    }
    let raw = PathBuf::from(entry_name);
    let mut safe = PathBuf::new();

    for component in raw.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => {
                return Err(FormatError::PathTraversal(entry_name.into()));
            }
            Component::ParentDir => {
                return Err(FormatError::PathTraversal(entry_name.into()));
            }
            Component::CurDir => continue,
            Component::Normal(c) => safe.push(c),
        }
    }

    let full = base_canon.join(&safe);
    if !full.starts_with(base_canon) {
        return Err(FormatError::PathTraversal(entry_name.into()));
    }
    Ok(full)
}

/// 압축 폭탄 방어: 압축 비율 검사.
/// 압축된 크기 input_size, 풀린 크기가 max_ratio 초과 시 거부.
pub fn check_bomb_ratio(input_size: u64, decompressed: u64, max_ratio: u64) -> Result<()> {
    if input_size == 0 {
        return Ok(()); // 메타데이터만 있는 빈 아카이브
    }
    let ratio = decompressed / input_size.max(1);
    if ratio > max_ratio {
        return Err(FormatError::BombDetected {
            ratio,
            limit: max_ratio,
        });
    }
    Ok(())
}

/// 출력 디렉토리 보장 + canonicalize.
pub fn ensure_output_dir(base_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(base_dir).map_err(FormatError::Io)?;
    base_dir.canonicalize().map_err(FormatError::Io)
}
