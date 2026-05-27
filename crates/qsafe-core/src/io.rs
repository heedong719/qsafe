//! 파일 I/O — qsafe 파일의 직렬화/역직렬화.
//!
//! 파일 구조:
//!
//! ```text
//! [MAGIC 8B]
//! [HEADER_LEN u32 LE]
//! [HEADER CBOR]
//! [PAYLOAD_LEN u64 LE]
//! [PAYLOAD]
//! [ORIGINAL_HASH BLAKE3 32B]   ← decompress 후 검증용
//! ```

use crate::error::{CoreError, Result};
use crate::format::{FileHeader, MAGIC};
use std::io::{Read, Write};

/// header/payload/hash 묶음.
#[derive(Debug)]
pub struct PackedFile {
    pub header: FileHeader,
    pub payload: Vec<u8>,
    pub original_hash: [u8; 32],
}

/// 헤더 최대 크기 (DoS 방지). 16 MiB.
const MAX_HEADER_LEN: u32 = 16 * 1024 * 1024;

/// 페이로드 최대 크기 (sanity check). 1 PiB.
const MAX_PAYLOAD_LEN: u64 = 1 << 50;

pub fn write_packed_file<W: Write>(
    mut w: W,
    header: &FileHeader,
    payload: &[u8],
    original_hash: &[u8; 32],
) -> Result<()> {
    header.validate()?;

    w.write_all(MAGIC)?;

    let header_bytes = header.encode()?;
    let header_len: u32 = header_bytes
        .len()
        .try_into()
        .map_err(|_| CoreError::HeaderEncode("header too large".into()))?;
    if header_len > MAX_HEADER_LEN {
        return Err(CoreError::HeaderEncode("header exceeds max size".into()));
    }
    w.write_all(&header_len.to_le_bytes())?;
    w.write_all(&header_bytes)?;

    let payload_len = payload.len() as u64;
    w.write_all(&payload_len.to_le_bytes())?;
    w.write_all(payload)?;

    w.write_all(original_hash)?;
    w.flush()?;
    Ok(())
}

pub fn read_packed_file<R: Read>(mut r: R) -> Result<PackedFile> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(CoreError::InvalidMagic);
    }

    let mut header_len_bytes = [0u8; 4];
    r.read_exact(&mut header_len_bytes)?;
    let header_len = u32::from_le_bytes(header_len_bytes);
    if header_len > MAX_HEADER_LEN {
        return Err(CoreError::HeaderDecode("header too large".into()));
    }

    let mut header_bytes = vec![0u8; header_len as usize];
    r.read_exact(&mut header_bytes)?;
    let header = FileHeader::decode(&header_bytes)?;
    header.validate()?;

    let mut payload_len_bytes = [0u8; 8];
    r.read_exact(&mut payload_len_bytes)?;
    let payload_len = u64::from_le_bytes(payload_len_bytes);
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(CoreError::HeaderDecode("payload too large".into()));
    }

    let payload_len_usize: usize = payload_len
        .try_into()
        .map_err(|_| CoreError::HeaderDecode("payload size exceeds platform".into()))?;
    let mut payload = vec![0u8; payload_len_usize];
    r.read_exact(&mut payload)?;

    let mut original_hash = [0u8; 32];
    r.read_exact(&mut original_hash)?;

    // Bug #7 fix: 파일 끝에 trailing garbage가 있으면 거부 (변조 탐지)
    let mut trailing = [0u8; 1];
    match r.read(&mut trailing) {
        Ok(0) => {}
        Ok(_) => return Err(CoreError::TrailingBytes),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
        Err(e) => return Err(CoreError::Io(e)),
    }

    Ok(PackedFile {
        header,
        payload,
        original_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{
        CipherSuite, CompressionAlgo, FileHeader, IntegrityAlgo, PasswordRecipient, Recipient,
    };

    fn sample_header() -> FileHeader {
        let mut h = FileHeader::new(CipherSuite::V1Xchacha20Blake3, CompressionAlgo::Zstd);
        h.original_size = 42;
        h.payload_nonce = vec![0; 24];
        h.integrity = IntegrityAlgo::Blake3;
        h.recipients.push(Recipient::Password(PasswordRecipient {
            salt: vec![1; 32],
            argon2_m_kib: 1024,
            argon2_t: 1,
            argon2_p: 1,
            nonce: vec![2; 24],
            encrypted_file_key: vec![3; 48],
        }));
        h
    }

    #[test]
    fn roundtrip_file() {
        let header = sample_header();
        let payload = vec![0u8, 1, 2, 3, 4];
        let hash = [0xAB; 32];

        let mut buf = Vec::new();
        write_packed_file(&mut buf, &header, &payload, &hash).unwrap();

        let pf = read_packed_file(buf.as_slice()).unwrap();
        assert_eq!(pf.payload, payload);
        assert_eq!(pf.original_hash, hash);
        assert_eq!(pf.header.original_size, 42);
        assert_eq!(pf.header.recipients.len(), 1);
    }

    #[test]
    fn invalid_magic_rejected() {
        let bad = vec![0xFFu8; 100];
        let err = read_packed_file(bad.as_slice()).unwrap_err();
        assert!(matches!(err, CoreError::InvalidMagic));
    }

    #[test]
    fn trailing_garbage_rejected() {
        let header = sample_header();
        let payload = vec![0u8; 10];
        let hash = [0xCD; 32];

        let mut buf = Vec::new();
        write_packed_file(&mut buf, &header, &payload, &hash).unwrap();
        buf.extend_from_slice(b"GARBAGE");

        let err = read_packed_file(buf.as_slice()).unwrap_err();
        assert!(matches!(err, CoreError::TrailingBytes));
    }

    #[test]
    fn empty_recipients_rejected() {
        let mut h = sample_header();
        h.recipients.clear();
        let mut buf = Vec::new();
        // 헤더 자체 validate에서 거부되어야 함
        let err = write_packed_file(&mut buf, &h, &[], &[0; 32]).unwrap_err();
        matches!(err, CoreError::HeaderDecode(_));
    }
}
