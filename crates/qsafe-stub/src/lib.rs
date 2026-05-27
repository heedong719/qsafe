//! qsafe SFX (self-extracting) 파일 포맷 정의 + helper.
//!
//! ## 파일 구조 (SFX)
//!
//! ```text
//! [STUB BINARY        ] ← 이 crate의 release build (x86_64-linux / aarch64-mac 등)
//! [QSAFE .qs PAYLOAD  ] ← 일반 qsafe pack 결과 (헤더 + AEAD ciphertext + BLAKE3)
//! [PAYLOAD_LEN u64 LE ] ← 8 bytes, little endian
//! [SFX_MAGIC 8 bytes  ] ← "QSAFESFX"
//! ```
//!
//! 총 16-byte footer로 stub이 자기 자신의 파일에서 payload 위치를 찾는다.
//! Stub은 자기 path를 `argv[0]` 또는 `std::env::current_exe()`로 알아낸다.
//!
//! ## 보안 노트
//!
//! - SFX는 사용자 친화적이지만 신뢰 모델 위험이 큰 패턴이다:
//!   - 사용자가 임의의 `.exe`를 더블 클릭 = 광범위한 공격 표면
//!   - 안티바이러스 false positive 자주 발생 (실행파일에 임의 데이터 append = malware 의심)
//!   - macOS Gatekeeper / Windows SmartScreen은 unsigned 바이너리 차단
//! - 그래서 SFX 출력에는 **항상 codesign / notarization 권장**.
//! - 우리 stub은 payload AEAD 검증 + BLAKE3 hash 검증을 그대로 유지하므로
//!   변조된 payload는 풀리지 않는다. 단, stub 자체가 변조될 수 있다는 한계는 SFX 본질.

use std::io::{Read, Seek, SeekFrom};

pub const SFX_MAGIC: &[u8; 8] = b"QSAFESFX";
pub const SFX_FOOTER_LEN: u64 = 16; // 8 (payload_len) + 8 (magic)

#[derive(Debug, thiserror::Error)]
pub enum SfxError {
    #[error("file is not a qsafe SFX (missing QSAFESFX footer)")]
    NotSfx,
    #[error("invalid SFX footer (payload size {payload} exceeds file size {file})")]
    InvalidFooter { payload: u64, file: u64 },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// 한 SFX 파일에서 `.qs` payload 바이트만 추출.
///
/// `reader`는 seekable이어야 한다 (file 또는 cursor).
pub fn extract_payload<R: Read + Seek>(mut reader: R) -> Result<Vec<u8>, SfxError> {
    let file_size = reader.seek(SeekFrom::End(0))?;
    if file_size < SFX_FOOTER_LEN {
        return Err(SfxError::NotSfx);
    }

    // footer 읽기
    reader.seek(SeekFrom::End(-(SFX_FOOTER_LEN as i64)))?;
    let mut footer = [0u8; 16];
    reader.read_exact(&mut footer)?;

    if &footer[8..] != SFX_MAGIC {
        return Err(SfxError::NotSfx);
    }

    let payload_len = u64::from_le_bytes(footer[..8].try_into().unwrap());
    let stub_end = file_size
        .checked_sub(SFX_FOOTER_LEN)
        .and_then(|v| v.checked_sub(payload_len))
        .ok_or(SfxError::InvalidFooter {
            payload: payload_len,
            file: file_size,
        })?;

    reader.seek(SeekFrom::Start(stub_end))?;
    let mut payload = vec![0u8; payload_len as usize];
    reader.read_exact(&mut payload)?;
    Ok(payload)
}

/// stub binary + payload → SFX 파일 바이트 생성.
///
/// 호출자가 결과를 0755 권한으로 저장해야 한다 (실행 가능).
pub fn assemble_sfx(stub_binary: &[u8], qs_payload: &[u8]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(stub_binary.len() + qs_payload.len() + SFX_FOOTER_LEN as usize);
    out.extend_from_slice(stub_binary);
    out.extend_from_slice(qs_payload);
    out.extend_from_slice(&(qs_payload.len() as u64).to_le_bytes());
    out.extend_from_slice(SFX_MAGIC);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn assemble_then_extract_roundtrip() {
        let stub = vec![0xCAu8; 256]; // 가짜 stub binary
        let payload = b"PRETEND THIS IS A REAL .qs FILE".to_vec();
        let sfx = assemble_sfx(&stub, &payload);

        // footer 확인
        assert_eq!(&sfx[sfx.len() - 8..], SFX_MAGIC);
        let recovered_len =
            u64::from_le_bytes(sfx[sfx.len() - 16..sfx.len() - 8].try_into().unwrap());
        assert_eq!(recovered_len as usize, payload.len());

        // extract
        let extracted = extract_payload(Cursor::new(&sfx)).unwrap();
        assert_eq!(extracted, payload);
    }

    #[test]
    fn extract_rejects_non_sfx() {
        let bytes = b"not an sfx file at all".to_vec();
        assert!(matches!(
            extract_payload(Cursor::new(&bytes)),
            Err(SfxError::NotSfx)
        ));
    }

    #[test]
    fn extract_rejects_truncated_payload() {
        // footer는 정상이지만 payload_len이 파일 크기보다 큼
        let stub = vec![0u8; 32];
        let bogus_payload_len: u64 = 9999;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&stub);
        bytes.extend_from_slice(&bogus_payload_len.to_le_bytes());
        bytes.extend_from_slice(SFX_MAGIC);
        assert!(matches!(
            extract_payload(Cursor::new(&bytes)),
            Err(SfxError::InvalidFooter { .. })
        ));
    }

    #[test]
    fn footer_layout_is_exactly_16_bytes() {
        assert_eq!(SFX_FOOTER_LEN, 16);
        assert_eq!(SFX_MAGIC.len(), 8);
    }
}
