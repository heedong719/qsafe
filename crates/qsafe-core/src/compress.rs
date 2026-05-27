//! 압축 알고리즘 추상화 + 폭탄 방어.
//!
//! ## 압축 폭탄 (Zip Bomb) 방어
//!
//! 악의적 압축 파일은 작지만 풀면 거대해서 메모리/디스크 고갈.
//! 이 모듈은 `expected_size`가 주어지면 그것을 limit으로 강제.
//! 없으면 입력 크기 × MAX_RATIO 까지만 허용.

use crate::error::{CoreError, Result};
use crate::format::CompressionAlgo;
use std::io::Read;

/// 압축 폭탄 방어 — 입력 크기 대비 최대 압축비.
/// 정상 텍스트도 1000:1 넘기는 경우 거의 없음.
pub const MAX_DECOMPRESSION_RATIO: u64 = 1000;

/// 입력 크기 모를 때 절대 한도 (10 GiB).
pub const ABSOLUTE_MAX_OUTPUT: u64 = 10 * 1024 * 1024 * 1024;

pub trait Compressor {
    fn algo(&self) -> CompressionAlgo;
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>>;
    /// `expected_size`가 Some이면 정확히 그 크기만 허용 (헤더 기반).
    /// None이면 ABSOLUTE_MAX_OUTPUT 또는 `data.len() × MAX_RATIO` 중 작은 값으로 제한.
    fn decompress(&self, data: &[u8], expected_size: Option<usize>) -> Result<Vec<u8>>;
}

/// 압축 폭탄 방어 limit 계산.
pub fn compute_decompress_limit(input_len: usize, expected: Option<usize>) -> u64 {
    if let Some(s) = expected {
        // 헤더가 정확한 크기를 알려주면 그것 + 작은 여유
        // (압축 라이브러리가 끝낸 후 어차피 정확 검증)
        (s as u64).saturating_add(64)
    } else {
        let by_ratio = (input_len as u64).saturating_mul(MAX_DECOMPRESSION_RATIO);
        by_ratio.min(ABSOLUTE_MAX_OUTPUT)
    }
}

pub struct NoneCompressor;

impl Compressor for NoneCompressor {
    fn algo(&self) -> CompressionAlgo {
        CompressionAlgo::None
    }
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
    fn decompress(&self, data: &[u8], expected_size: Option<usize>) -> Result<Vec<u8>> {
        if let Some(s) = expected_size {
            if data.len() != s {
                return Err(CoreError::SizeMismatch {
                    expected: s as u64,
                    got: data.len() as u64,
                });
            }
        }
        Ok(data.to_vec())
    }
}

#[cfg(feature = "compress-zstd")]
pub struct ZstdCompressor {
    pub level: i32,
}

#[cfg(feature = "compress-zstd")]
impl ZstdCompressor {
    pub fn new(level: i32) -> Self {
        Self { level }
    }
}

#[cfg(feature = "compress-zstd")]
impl Compressor for ZstdCompressor {
    fn algo(&self) -> CompressionAlgo {
        CompressionAlgo::Zstd
    }
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        // 멀티스레드 인코더 (zstd 1.5+ 자체 worker thread)
        // 큰 파일에서 4-8배 빠름
        let num_workers = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);

        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), self.level)
            .map_err(|e| CoreError::Compression(e.to_string()))?;
        // multithread만 활성화 (입력이 작으면 단일 스레드)
        if data.len() > 1024 * 1024 {
            let _ = encoder.multithread(num_workers);
        }
        use std::io::Write;
        encoder
            .write_all(data)
            .map_err(|e| CoreError::Compression(e.to_string()))?;
        encoder
            .finish()
            .map_err(|e| CoreError::Compression(e.to_string()))
    }
    fn decompress(&self, data: &[u8], expected_size: Option<usize>) -> Result<Vec<u8>> {
        let limit = compute_decompress_limit(data.len(), expected_size);
        let decoder = zstd::stream::read::Decoder::new(data)
            .map_err(|e| CoreError::Compression(e.to_string()))?;
        // Read::take 으로 하드 limit 강제 (압축 폭탄 방어)
        let mut limited = decoder.take(limit);
        let mut out: Vec<u8> = Vec::with_capacity(expected_size.unwrap_or(0).min(64 * 1024 * 1024));
        let bytes_read = limited
            .read_to_end(&mut out)
            .map_err(|e| CoreError::Compression(e.to_string()))? as u64;

        // limit 도달 후에도 데이터가 남아 있는지 확인 — 폭탄 신호
        if bytes_read == limit {
            // 추가 byte 읽기 시도 (있으면 폭탄)
            let mut extra = [0u8; 1];
            let mut decoder_check = limited.into_inner();
            if let Ok(n) = decoder_check.read(&mut extra) {
                if n > 0 {
                    return Err(CoreError::CompressionBomb {
                        got: bytes_read + n as u64,
                        limit,
                    });
                }
            }
        }

        // expected_size 정확 매치 검증
        if let Some(s) = expected_size {
            if out.len() != s {
                return Err(CoreError::SizeMismatch {
                    expected: s as u64,
                    got: out.len() as u64,
                });
            }
        }

        Ok(out)
    }
}

pub fn make_compressor(algo: CompressionAlgo) -> Result<Box<dyn Compressor>> {
    match algo {
        CompressionAlgo::None => Ok(Box::new(NoneCompressor)),
        #[cfg(feature = "compress-zstd")]
        CompressionAlgo::Zstd => Ok(Box::new(ZstdCompressor::new(3))),
        #[cfg(not(feature = "compress-zstd"))]
        CompressionAlgo::Zstd => Err(CoreError::UnsupportedCompression(algo)),
        _ => Err(CoreError::UnsupportedCompression(algo)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_computation() {
        // expected이 있으면 그 크기 + 작은 여유만
        let l = compute_decompress_limit(1000, Some(5000));
        assert_eq!(l, 5064);

        // expected 없으면 input * MAX_RATIO
        let l = compute_decompress_limit(100, None);
        assert_eq!(l, 100_000);

        // ABSOLUTE_MAX 초과 방어
        let l = compute_decompress_limit(usize::MAX, None);
        assert_eq!(l, ABSOLUTE_MAX_OUTPUT);
    }

    #[test]
    fn none_compressor_size_mismatch_rejected() {
        let c = NoneCompressor;
        let data = vec![1u8, 2, 3];
        assert!(c.decompress(&data, Some(99)).is_err());
        assert!(c.decompress(&data, Some(3)).is_ok());
    }

    #[cfg(feature = "compress-zstd")]
    #[test]
    fn zstd_roundtrip_with_size_check() {
        let c = ZstdCompressor::new(3);
        let original = b"hello world".repeat(100);
        let compressed = c.compress(&original).unwrap();
        let decompressed = c.decompress(&compressed, Some(original.len())).unwrap();
        assert_eq!(decompressed, original);
    }

    #[cfg(feature = "compress-zstd")]
    #[test]
    fn zstd_wrong_expected_size_rejected() {
        let c = ZstdCompressor::new(3);
        let original = b"hello".repeat(100);
        let compressed = c.compress(&original).unwrap();
        // 잘못된 expected_size — 거부 또는 limit 도달
        let result = c.decompress(&compressed, Some(10));
        assert!(result.is_err());
    }
}
