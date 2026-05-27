//! 다중 수신자 봉투의 핵심 — FileKey 생성/소멸 + 페이로드 AEAD.
//!
//! 흐름:
//!   pack:   FileKey = random 32B → 페이로드 AEAD(K, nonce, compressed)
//!           각 수신자는 FileKey를 자기 방식으로 봉투화 → 헤더에 저장
//!   unpack: 수신자 중 하나가 FileKey 복원 → AEAD 역연산 → decompress
//!
//! FileKey는 Drop 시 자동 zeroize. 절대 평문으로 헤더에 들어가지 않는다.

use crate::error::{CoreError, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use zeroize::Zeroize;

pub const FILE_KEY_LEN: usize = 32;
pub const PAYLOAD_NONCE_LEN: usize = 24; // XChaCha20

/// 스트리밍 청크 크기 (4 MiB). 메모리/오버헤드 균형.
pub const STREAM_CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// 스트리밍 base nonce 길이 (20 bytes). chunk_idx(4 bytes)를 붙여 24 bytes XChaCha20 nonce 완성.
pub const STREAM_BASE_NONCE_LEN: usize = 20;

/// 파일 콘텐츠를 직접 암호화하는 키. Drop 시 자동 zeroize.
pub struct FileKey([u8; FILE_KEY_LEN]);

impl FileKey {
    /// CSPRNG로 무작위 키 생성.
    pub fn random() -> Self {
        let mut k = [0u8; FILE_KEY_LEN];
        OsRng.fill_bytes(&mut k);
        Self(k)
    }

    /// 기존 바이트로부터 키 생성 (수신자가 봉투를 풀어 복원할 때 사용).
    pub fn from_bytes(bytes: [u8; FILE_KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// 키의 raw 바이트 접근. 호출자가 zeroize 책임.
    pub fn as_bytes(&self) -> &[u8; FILE_KEY_LEN] {
        &self.0
    }
}

impl Drop for FileKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Zeroize for FileKey {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

/// 페이로드 nonce를 무작위로 생성.
pub fn random_payload_nonce() -> Vec<u8> {
    let mut n = vec![0u8; PAYLOAD_NONCE_LEN];
    OsRng.fill_bytes(&mut n);
    n
}

/// 페이로드를 FileKey + nonce로 암호화 (XChaCha20-Poly1305).
pub fn encrypt_payload(file_key: &FileKey, nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    if nonce.len() != PAYLOAD_NONCE_LEN {
        return Err(CoreError::HeaderDecode(
            "invalid payload nonce length".into(),
        ));
    }
    let cipher = XChaCha20Poly1305::new(file_key.as_bytes().into());
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .encrypt(xnonce, plaintext)
        .map_err(|_| CoreError::IntegrityFailed)
}

/// 페이로드 복호화. MAC 실패 시 IntegrityFailed.
pub fn decrypt_payload(file_key: &FileKey, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    if nonce.len() != PAYLOAD_NONCE_LEN {
        return Err(CoreError::HeaderDecode(
            "invalid payload nonce length".into(),
        ));
    }
    let cipher = XChaCha20Poly1305::new(file_key.as_bytes().into());
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(xnonce, ciphertext)
        .map_err(|_| CoreError::IntegrityFailed)
}

// ─── 스트리밍 AEAD (큰 파일 OOM 방지) ─────────────────────────

/// 스트리밍 base nonce 무작위 생성 (20 bytes).
pub fn random_stream_base_nonce() -> [u8; STREAM_BASE_NONCE_LEN] {
    let mut n = [0u8; STREAM_BASE_NONCE_LEN];
    OsRng.fill_bytes(&mut n);
    n
}

/// 청크 인덱스 → 24-byte XChaCha20 nonce 변환.
/// `nonce_24 = base_nonce_20 || chunk_idx_be(4)`
fn build_chunk_nonce(base_nonce: &[u8; STREAM_BASE_NONCE_LEN], chunk_idx: u32) -> [u8; 24] {
    let mut nonce = [0u8; 24];
    nonce[..STREAM_BASE_NONCE_LEN].copy_from_slice(base_nonce);
    nonce[STREAM_BASE_NONCE_LEN..].copy_from_slice(&chunk_idx.to_be_bytes());
    nonce
}

/// 청크 하나 암호화. 청크별 다른 nonce → 재사용 위험 0.
pub fn encrypt_chunk(
    file_key: &FileKey,
    base_nonce: &[u8; STREAM_BASE_NONCE_LEN],
    chunk_idx: u32,
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(file_key.as_bytes().into());
    let nonce = build_chunk_nonce(base_nonce, chunk_idx);
    let xnonce = XNonce::from_slice(&nonce);
    cipher
        .encrypt(xnonce, plaintext)
        .map_err(|_| CoreError::IntegrityFailed)
}

/// 청크 하나 복호화. chunk_idx와 base_nonce가 정확히 맞아야 성공.
pub fn decrypt_chunk(
    file_key: &FileKey,
    base_nonce: &[u8; STREAM_BASE_NONCE_LEN],
    chunk_idx: u32,
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(file_key.as_bytes().into());
    let nonce = build_chunk_nonce(base_nonce, chunk_idx);
    let xnonce = XNonce::from_slice(&nonce);
    cipher
        .decrypt(xnonce, ciphertext)
        .map_err(|_| CoreError::IntegrityFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_roundtrip() {
        let k = FileKey::random();
        let nonce = random_payload_nonce();
        let plaintext = b"hello qsafe multi-recipient envelope";
        let ct = encrypt_payload(&k, &nonce, plaintext).unwrap();
        let pt = decrypt_payload(&k, &nonce, &ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let k = FileKey::random();
        let nonce = random_payload_nonce();
        let mut ct = encrypt_payload(&k, &nonce, b"data").unwrap();
        ct[0] ^= 1;
        assert!(decrypt_payload(&k, &nonce, &ct).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let k1 = FileKey::random();
        let k2 = FileKey::random();
        let nonce = random_payload_nonce();
        let ct = encrypt_payload(&k1, &nonce, b"data").unwrap();
        assert!(decrypt_payload(&k2, &nonce, &ct).is_err());
    }

    #[test]
    fn wrong_nonce_fails() {
        let k = FileKey::random();
        let nonce1 = random_payload_nonce();
        let nonce2 = random_payload_nonce();
        let ct = encrypt_payload(&k, &nonce1, b"data").unwrap();
        assert!(decrypt_payload(&k, &nonce2, &ct).is_err());
    }

    #[test]
    fn stream_chunk_roundtrip() {
        let k = FileKey::random();
        let base = random_stream_base_nonce();
        let chunks = [b"chunk0".to_vec(), b"chunk1".to_vec(), b"chunk2".to_vec()];

        let mut cts = Vec::new();
        for (i, c) in chunks.iter().enumerate() {
            cts.push(encrypt_chunk(&k, &base, i as u32, c).unwrap());
        }

        for (i, ct) in cts.iter().enumerate() {
            let pt = decrypt_chunk(&k, &base, i as u32, ct).unwrap();
            assert_eq!(pt, chunks[i]);
        }
    }

    #[test]
    fn stream_chunk_wrong_idx_fails() {
        let k = FileKey::random();
        let base = random_stream_base_nonce();
        let ct = encrypt_chunk(&k, &base, 0, b"data").unwrap();
        // idx 1로 풀려고 하면 다른 nonce → 실패
        assert!(decrypt_chunk(&k, &base, 1, &ct).is_err());
    }

    #[test]
    fn stream_chunk_reorder_detected() {
        let k = FileKey::random();
        let base = random_stream_base_nonce();
        let ct0 = encrypt_chunk(&k, &base, 0, b"first").unwrap();
        let ct1 = encrypt_chunk(&k, &base, 1, b"second").unwrap();

        // 순서 바꾸면 nonce 불일치 → 실패
        assert!(decrypt_chunk(&k, &base, 0, &ct1).is_err());
        assert!(decrypt_chunk(&k, &base, 1, &ct0).is_err());
    }
}
