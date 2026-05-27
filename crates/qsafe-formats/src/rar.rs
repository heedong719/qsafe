//! RAR 4.x + RAR 5.x 풀기 (unrar 라이브러리 사용).
//!
//! ## 라이센스 준수
//!
//! 7-Zip과 동일한 방식으로 Eugene Roshal의 unrar 라이브러리를 사용합니다.
//! unrar 라이센스 (license.txt):
//!   ✅ "may be used in any software to handle RAR archives free of charge"
//!   ❌ "may not be used to develop a RAR compatible archiver"
//!
//! 따라서:
//!   ✅ RAR 풀기 (extract): 합법 + 가능
//!   ❌ RAR 만들기 (create): 합법적으로 불가능 → WinRAR 구매 필요
//!
//! ## 지원
//!
//! - RAR 4.x (legacy) + RAR 5.x (current) 모두
//! - 패스워드 보호 RAR
//! - 솔리드 아카이브
//! - 디렉토리 트리 추출
//! - 경로 이탈 (..) 공격 자동 차단

use crate::error::{FormatError, Result};
use std::path::{Component, Path, PathBuf};
use unrar::Archive;

#[derive(Debug, Clone)]
pub struct RarEntry {
    pub filename: String,
    pub unpacked_size: u64,
    pub is_directory: bool,
    pub is_encrypted: bool,
}

/// RAR 아카이브 내용 목록 조회 (추출 없이).
pub fn list_rar(rar_path: impl AsRef<Path>, password: Option<&str>) -> Result<Vec<RarEntry>> {
    let path = rar_path.as_ref();

    let archive = match password {
        Some(pw) => Archive::with_password(path, pw),
        None => Archive::new(path),
    };

    let archive_open = archive
        .open_for_listing()
        .map_err(|e| FormatError::Rar(format!("open_for_listing: {}", e)))?;

    let mut entries = Vec::new();
    for entry_result in archive_open {
        match entry_result {
            Ok(entry) => {
                entries.push(RarEntry {
                    filename: entry.filename.to_string_lossy().to_string(),
                    unpacked_size: entry.unpacked_size,
                    is_directory: entry.is_directory(),
                    is_encrypted: entry.is_encrypted(),
                });
            }
            Err(e) => {
                if entries.is_empty() {
                    return Err(FormatError::Rar(format!("list: {}", e)));
                }
                tracing::warn!("RAR entry 읽기 실패: {}", e);
                break;
            }
        }
    }
    if entries.is_empty() {
        return Err(FormatError::EmptyArchive);
    }
    Ok(entries)
}

/// RAR 아카이브를 대상 디렉토리에 모두 추출. 반환: 추출된 파일 수.
pub fn extract_rar(
    rar_path: impl AsRef<Path>,
    base_dir: impl AsRef<Path>,
    password: Option<&str>,
) -> Result<usize> {
    let rar_path = rar_path.as_ref();
    let base_dir = base_dir.as_ref();

    if !base_dir.is_dir() {
        return Err(FormatError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("base_dir 가 없습니다: {}", base_dir.display()),
        )));
    }
    let base_canon = base_dir.canonicalize().map_err(FormatError::Io)?;

    let archive = match password {
        Some(pw) => Archive::with_password(rar_path, pw),
        None => Archive::new(rar_path),
    };
    let mut archive = archive
        .open_for_processing()
        .map_err(|e| FormatError::Rar(format!("open_for_processing: {}", e)))?;

    let mut extracted = 0usize;

    while let Some(header) = archive
        .read_header()
        .map_err(|e| FormatError::Rar(format!("read_header: {}", e)))?
    {
        // entry 정보를 헤더 소비 전에 추출
        let entry_name = header.entry().filename.to_string_lossy().to_string();
        let is_directory = header.entry().is_directory();
        let safe_path = sanitize_archive_path(&entry_name, &base_canon)?;

        archive = if is_directory {
            std::fs::create_dir_all(&safe_path).map_err(FormatError::Io)?;
            header
                .skip()
                .map_err(|e| FormatError::Rar(format!("skip dir {}: {}", entry_name, e)))?
        } else {
            if let Some(parent) = safe_path.parent() {
                std::fs::create_dir_all(parent).map_err(FormatError::Io)?;
            }
            let next = header
                .extract_to(&safe_path)
                .map_err(|e| FormatError::Rar(format!("extract {}: {}", entry_name, e)))?;
            extracted += 1;
            next
        };
    }

    Ok(extracted)
}

/// 아카이브 내부 경로를 base 디렉토리 내로 정규화. ../ 공격 차단.
fn sanitize_archive_path(entry_name: &str, base: &Path) -> Result<PathBuf> {
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

    let full = base.join(&safe);
    if !full.starts_with(base) {
        return Err(FormatError::PathTraversal(entry_name.into()));
    }
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe_base() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "qsafe-rar-test-{}-{}",
            std::process::id(),
            rand_id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p.canonicalize().unwrap()
    }

    fn rand_id() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    #[test]
    fn sanitize_normal_path() {
        let base = safe_base();
        let result = sanitize_archive_path("folder/file.txt", &base).unwrap();
        assert!(result.starts_with(&base));
        assert!(result.ends_with("folder/file.txt"));
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn sanitize_blocks_parent_traversal() {
        let base = safe_base();
        let err = sanitize_archive_path("../../../etc/passwd", &base).unwrap_err();
        assert!(matches!(err, FormatError::PathTraversal(_)));
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn sanitize_blocks_absolute_path() {
        let base = safe_base();
        let err = sanitize_archive_path("/etc/passwd", &base).unwrap_err();
        assert!(matches!(err, FormatError::PathTraversal(_)));
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn nonexistent_rar_fails() {
        let result = list_rar("/nonexistent/file.rar", None);
        assert!(result.is_err());
    }
}
