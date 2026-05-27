//! 스트리밍 압축 + 암호화 — 대용량 파일 OOM 방지.
//!
//! ## 메모리 사용
//!
//! 4MB 청크 단위 처리:
//!   - 압축 버퍼: ~4 MB
//!   - AEAD 버퍼: ~4 MB + 16 byte tag
//!   - 총 사용: **~10 MB regardless of file size**
//!
//! 10 GB 파일도 10 MB 메모리로 처리.
//!
//! ## 파일 포맷 (streaming 모드)
//!
//! ```text
//! [MAGIC 8B][HEADER_LEN u32][HEADER CBOR with chunks=Some]
//! [chunk_0_ct_len u32 LE][chunk_0_ct]
//! [chunk_1_ct_len u32 LE][chunk_1_ct]
//! ...
//! [chunk_N-1_ct_len u32 LE][chunk_N-1_ct]
//! [ORIGINAL_HASH 32B]
//! ```
//!
//! ## 보안
//!
//! - 청크별 다른 nonce (base_nonce || chunk_idx)
//! - 청크 순서 검증 (재정렬 공격 차단)
//! - 청크 누락 검증 (헤더의 num_chunks 비교)
//! - 마지막 청크 인식 (재생 공격 차단)

use crate::envelope::{
    decrypt_chunk, encrypt_chunk, FileKey, STREAM_BASE_NONCE_LEN, STREAM_CHUNK_SIZE,
};
use crate::error::{CoreError, Result};
use crate::format::{ChunkInfo, FileHeader, MAGIC};
use std::io::{Read, Write};

/// 스트리밍 pack with BLAKE3 hashing (단일 패스).
/// reader → hasher 갱신 → AEAD chunk → writer.
#[allow(clippy::type_complexity)]
pub fn stream_encrypt_with_hash<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    file_key: &FileKey,
    base_nonce: &[u8; STREAM_BASE_NONCE_LEN],
    hasher: &mut blake3::Hasher,
) -> Result<(u32, u32, u64)> {
    let mut chunk_idx: u32 = 0;
    let mut last_chunk_size: u32 = 0;
    let mut total_bytes: u64 = 0;
    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];

    loop {
        let mut filled = 0usize;
        while filled < STREAM_CHUNK_SIZE {
            let n = reader.read(&mut buf[filled..]).map_err(CoreError::Io)?;
            if n == 0 {
                break;
            }
            filled += n;
        }

        if filled == 0 {
            break;
        }

        // BLAKE3 갱신 (평문에 대해)
        hasher.update(&buf[..filled]);
        total_bytes += filled as u64;

        let ct = encrypt_chunk(file_key, base_nonce, chunk_idx, &buf[..filled])?;
        let ct_len = ct.len() as u32;
        writer
            .write_all(&ct_len.to_le_bytes())
            .map_err(CoreError::Io)?;
        writer.write_all(&ct).map_err(CoreError::Io)?;

        last_chunk_size = filled as u32;
        chunk_idx += 1;

        if filled < STREAM_CHUNK_SIZE {
            break;
        }
    }

    if chunk_idx == 0 {
        // 빈 입력
        let ct = encrypt_chunk(file_key, base_nonce, 0, b"")?;
        let ct_len = ct.len() as u32;
        writer
            .write_all(&ct_len.to_le_bytes())
            .map_err(CoreError::Io)?;
        writer.write_all(&ct).map_err(CoreError::Io)?;
        return Ok((1, 0, 0));
    }

    Ok((chunk_idx, last_chunk_size, total_bytes))
}

/// 스트리밍 unpack with BLAKE3 검증.
pub fn stream_decrypt_with_hash<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    file_key: &FileKey,
    base_nonce: &[u8; STREAM_BASE_NONCE_LEN],
    expected_chunks: u32,
    hasher: &mut blake3::Hasher,
) -> Result<()> {
    let mut len_buf = [0u8; 4];

    for chunk_idx in 0..expected_chunks {
        reader.read_exact(&mut len_buf).map_err(|e| {
            CoreError::Io(std::io::Error::new(
                e.kind(),
                format!("chunk {} length read: {}", chunk_idx, e),
            ))
        })?;
        let ct_len = u32::from_le_bytes(len_buf) as usize;

        if ct_len > STREAM_CHUNK_SIZE + 128 {
            return Err(CoreError::HeaderDecode(format!(
                "chunk {} too large: {}",
                chunk_idx, ct_len
            )));
        }

        let mut ct = vec![0u8; ct_len];
        reader.read_exact(&mut ct).map_err(CoreError::Io)?;

        let pt = decrypt_chunk(file_key, base_nonce, chunk_idx, &ct)?;
        hasher.update(&pt);
        writer.write_all(&pt).map_err(CoreError::Io)?;
    }

    Ok(())
}

/// 스트리밍 임계값 — 이보다 크면 자동 streaming.
pub const STREAM_THRESHOLD: u64 = 100 * 1024 * 1024; // 100 MB

/// 스트리밍 pack: reader → (압축) → (AEAD chunk) → writer.
///
/// `compressed_reader`는 이미 압축된 데이터 stream (zstd::stream::write::Encoder의 output 등).
/// 단순화: 호출자가 압축을 처리하고 평문(원본 또는 압축됨)을 reader로 전달.
///
/// `hasher`가 Some이면 원본 데이터에 대해 hash 갱신 (BLAKE3).
///
/// 반환: (총 청크 수, 마지막 청크 크기)
pub fn stream_encrypt<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    file_key: &FileKey,
    base_nonce: &[u8; STREAM_BASE_NONCE_LEN],
) -> Result<(u32, u32)> {
    let mut chunk_idx: u32 = 0;
    let mut last_chunk_size: u32 = 0;
    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];

    loop {
        // 청크 채우기 (EOF 나올 때까지)
        let mut filled = 0usize;
        while filled < STREAM_CHUNK_SIZE {
            let n = reader.read(&mut buf[filled..]).map_err(CoreError::Io)?;
            if n == 0 {
                break;
            }
            filled += n;
        }

        if filled == 0 {
            // 정상 EOF
            break;
        }

        // 청크 암호화
        let ct = encrypt_chunk(file_key, base_nonce, chunk_idx, &buf[..filled])?;

        // [u32 LE chunk_ct_len][ct]
        let ct_len = ct.len() as u32;
        writer
            .write_all(&ct_len.to_le_bytes())
            .map_err(CoreError::Io)?;
        writer.write_all(&ct).map_err(CoreError::Io)?;

        last_chunk_size = filled as u32;
        chunk_idx += 1;

        if filled < STREAM_CHUNK_SIZE {
            // 마지막 청크 (덜 채워짐)
            break;
        }
    }

    if chunk_idx == 0 {
        // 빈 입력 처리 — 길이 0 청크 하나 보냄
        let ct = encrypt_chunk(file_key, base_nonce, 0, b"")?;
        let ct_len = ct.len() as u32;
        writer
            .write_all(&ct_len.to_le_bytes())
            .map_err(CoreError::Io)?;
        writer.write_all(&ct).map_err(CoreError::Io)?;
        return Ok((1, 0));
    }

    Ok((chunk_idx, last_chunk_size))
}

/// 스트리밍 unpack: reader (chunk ciphertext stream) → (AEAD decrypt) → writer.
pub fn stream_decrypt<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    file_key: &FileKey,
    base_nonce: &[u8; STREAM_BASE_NONCE_LEN],
    expected_chunks: u32,
) -> Result<()> {
    let mut len_buf = [0u8; 4];

    for chunk_idx in 0..expected_chunks {
        reader.read_exact(&mut len_buf).map_err(|e| {
            CoreError::Io(std::io::Error::new(
                e.kind(),
                format!("chunk {} length read: {}", chunk_idx, e),
            ))
        })?;
        let ct_len = u32::from_le_bytes(len_buf) as usize;

        // sanity check: 청크 ciphertext는 평문 + AEAD tag(16) — STREAM_CHUNK_SIZE + 16 이하여야
        if ct_len > STREAM_CHUNK_SIZE + 128 {
            return Err(CoreError::HeaderDecode(format!(
                "chunk {} too large: {}",
                chunk_idx, ct_len
            )));
        }

        let mut ct = vec![0u8; ct_len];
        reader.read_exact(&mut ct).map_err(|e| {
            CoreError::Io(std::io::Error::new(
                e.kind(),
                format!("chunk {} data read: {}", chunk_idx, e),
            ))
        })?;

        let pt = decrypt_chunk(file_key, base_nonce, chunk_idx, &ct)?;
        writer.write_all(&pt).map_err(CoreError::Io)?;
    }

    Ok(())
}

/// 스트리밍 파일 헤더 작성 (MAGIC + HEADER_LEN + HEADER).
pub fn write_stream_header<W: Write>(mut w: W, header: &FileHeader) -> Result<()> {
    header.validate()?;

    w.write_all(MAGIC).map_err(CoreError::Io)?;
    let header_bytes = header.encode()?;
    let header_len: u32 = header_bytes
        .len()
        .try_into()
        .map_err(|_| CoreError::HeaderEncode("header too large".into()))?;
    w.write_all(&header_len.to_le_bytes())
        .map_err(CoreError::Io)?;
    w.write_all(&header_bytes).map_err(CoreError::Io)?;
    Ok(())
}

/// 스트리밍 파일 헤더 읽기.
pub fn read_stream_header<R: Read>(mut r: R) -> Result<FileHeader> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic).map_err(CoreError::Io)?;
    if &magic != MAGIC {
        return Err(CoreError::InvalidMagic);
    }

    let mut header_len_bytes = [0u8; 4];
    r.read_exact(&mut header_len_bytes).map_err(CoreError::Io)?;
    let header_len = u32::from_le_bytes(header_len_bytes);
    if header_len > 16 * 1024 * 1024 {
        return Err(CoreError::HeaderDecode("header too large".into()));
    }

    let mut header_bytes = vec![0u8; header_len as usize];
    r.read_exact(&mut header_bytes).map_err(CoreError::Io)?;
    let header = FileHeader::decode(&header_bytes)?;
    header.validate()?;
    Ok(header)
}

/// 스트리밍 파일에서 ChunkInfo 추출 + 검증.
pub fn require_chunked(header: &FileHeader) -> Result<ChunkInfo> {
    header
        .chunks
        .clone()
        .ok_or_else(|| CoreError::HeaderDecode("not a streaming file".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn stream_pack_unpack_roundtrip_small() {
        let file_key = FileKey::random();
        let base_nonce = crate::envelope::random_stream_base_nonce();

        let plaintext = b"hello stream world".repeat(100);
        let mut ct_buf = Vec::new();
        let (n_chunks, last_size) =
            stream_encrypt(Cursor::new(&plaintext), &mut ct_buf, &file_key, &base_nonce).unwrap();

        assert_eq!(n_chunks, 1);
        assert_eq!(last_size as usize, plaintext.len());

        let mut out = Vec::new();
        stream_decrypt(
            Cursor::new(&ct_buf),
            &mut out,
            &file_key,
            &base_nonce,
            n_chunks,
        )
        .unwrap();

        assert_eq!(out, plaintext);
    }

    #[test]
    fn stream_pack_unpack_multi_chunks() {
        let file_key = FileKey::random();
        let base_nonce = crate::envelope::random_stream_base_nonce();

        // 10 MB 입력 → 약 3 청크 (4MB + 4MB + 2MB)
        let plaintext: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 256) as u8).collect();

        let mut ct_buf = Vec::new();
        let (n_chunks, last_size) =
            stream_encrypt(Cursor::new(&plaintext), &mut ct_buf, &file_key, &base_nonce).unwrap();

        assert_eq!(n_chunks, 3);
        assert_eq!(last_size as usize, 2 * 1024 * 1024);

        let mut out = Vec::new();
        stream_decrypt(
            Cursor::new(&ct_buf),
            &mut out,
            &file_key,
            &base_nonce,
            n_chunks,
        )
        .unwrap();

        assert_eq!(out, plaintext);
    }

    #[test]
    fn stream_tampered_chunk_rejected() {
        let file_key = FileKey::random();
        let base_nonce = crate::envelope::random_stream_base_nonce();

        let plaintext = vec![42u8; 5 * 1024 * 1024];
        let mut ct_buf = Vec::new();
        let (n_chunks, _) =
            stream_encrypt(Cursor::new(&plaintext), &mut ct_buf, &file_key, &base_nonce).unwrap();

        // 두 번째 청크 ciphertext 1 byte 뒤집기
        // 첫 청크 끝 위치 찾기
        let first_chunk_ct_len = u32::from_le_bytes(ct_buf[..4].try_into().unwrap()) as usize;
        let second_chunk_start = 4 + first_chunk_ct_len + 4; // skip len of 2nd
        ct_buf[second_chunk_start + 100] ^= 1;

        let mut out = Vec::new();
        let result = stream_decrypt(
            Cursor::new(&ct_buf),
            &mut out,
            &file_key,
            &base_nonce,
            n_chunks,
        );
        assert!(result.is_err());
    }

    #[test]
    fn stream_empty_input() {
        let file_key = FileKey::random();
        let base_nonce = crate::envelope::random_stream_base_nonce();

        let mut ct_buf = Vec::new();
        let (n_chunks, last_size) =
            stream_encrypt(Cursor::new(&[][..]), &mut ct_buf, &file_key, &base_nonce).unwrap();
        assert_eq!(n_chunks, 1);
        assert_eq!(last_size, 0);

        let mut out = Vec::new();
        stream_decrypt(
            Cursor::new(&ct_buf),
            &mut out,
            &file_key,
            &base_nonce,
            n_chunks,
        )
        .unwrap();
        assert!(out.is_empty());
    }
}
