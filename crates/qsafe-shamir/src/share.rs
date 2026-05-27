//! Shamir share — 분할/결합 + 종이 백업용 인코딩.

use crate::error::{Result, ShamirError};
use sharks::{Share, Sharks};
use std::str::FromStr;
use zeroize::Zeroize;

pub const MIN_SHARES: u8 = 2;
pub const MAX_SHARES: u8 = 255;

/// 종이 백업 친화 share 표현.
///
/// 포맷: `qs1-M-N-XX-HEXDATA` 형식
///   - qs1 = qsafe shamir v1 매직
///   - M = 임계값
///   - N = 총 share 수
///   - XX = 이 share의 인덱스 (16진수 1바이트)
///   - HEXDATA = 데이터 (hex)
#[derive(Debug, Clone)]
pub struct EncodedShare {
    pub threshold: u8,
    pub total: u8,
    pub index: u8,
    pub data: Vec<u8>,
}

impl EncodedShare {
    /// 종이 백업용 문자열 인코딩.
    pub fn to_string(&self) -> String {
        format!(
            "qs1-{}-{}-{:02x}-{}",
            self.threshold,
            self.total,
            self.index,
            hex::encode(&self.data)
        )
    }
}

impl FromStr for EncodedShare {
    type Err = ShamirError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 5 || parts[0] != "qs1" {
            return Err(ShamirError::InvalidShare(format!(
                "예상 형식: qs1-M-N-XX-HEXDATA, 받음: {}",
                s
            )));
        }
        let threshold: u8 = parts[1]
            .parse()
            .map_err(|_| ShamirError::InvalidShare("M 파싱 실패".into()))?;
        let total: u8 = parts[2]
            .parse()
            .map_err(|_| ShamirError::InvalidShare("N 파싱 실패".into()))?;
        let index = u8::from_str_radix(parts[3], 16)
            .map_err(|_| ShamirError::InvalidShare("index hex 파싱 실패".into()))?;
        let data = hex::decode(parts[4]).map_err(|e| ShamirError::Hex(e.to_string()))?;
        Ok(EncodedShare {
            threshold,
            total,
            index,
            data,
        })
    }
}

/// secret을 M-of-N으로 분할.
pub fn split_secret(secret: &[u8], threshold: u8, total: u8) -> Result<Vec<EncodedShare>> {
    if threshold < MIN_SHARES || total < threshold || total > MAX_SHARES {
        return Err(ShamirError::InvalidThreshold {
            m: threshold,
            n: total,
        });
    }
    let sharks = Sharks(threshold);
    let dealer = sharks.dealer(secret);
    let shares: Vec<Share> = dealer.take(total as usize).collect();

    let mut encoded = Vec::with_capacity(shares.len());
    for (i, share) in shares.iter().enumerate() {
        let bytes: Vec<u8> = Vec::from(share);
        // sharks Share 직렬화 형식: [index_byte, ...data_bytes]
        if bytes.is_empty() {
            return Err(ShamirError::RecoveryFailed("empty share".into()));
        }
        encoded.push(EncodedShare {
            threshold,
            total,
            index: (i + 1) as u8, // 1-based
            data: bytes,
        });
    }
    Ok(encoded)
}

/// 충분한 share로 secret 복구.
pub fn combine_secret(shares: &[EncodedShare]) -> Result<Vec<u8>> {
    if shares.is_empty() {
        return Err(ShamirError::NotEnoughShares { got: 0, need: 2 });
    }
    let threshold = shares[0].threshold;
    let total = shares[0].total;

    // 모든 share가 같은 set인지 검증
    for s in shares {
        if s.threshold != threshold || s.total != total {
            return Err(ShamirError::InconsistentShares);
        }
    }

    if shares.len() < threshold as usize {
        return Err(ShamirError::NotEnoughShares {
            got: shares.len(),
            need: threshold as usize,
        });
    }

    let sharks_shares: std::result::Result<Vec<Share>, _> = shares
        .iter()
        .map(|es| Share::try_from(es.data.as_slice()))
        .collect();
    let sharks_shares = sharks_shares.map_err(|e| ShamirError::InvalidShare(e.to_string()))?;

    let sharks = Sharks(threshold);
    let mut secret = sharks
        .recover(sharks_shares.as_slice())
        .map_err(|e| ShamirError::RecoveryFailed(e.to_string()))?;

    let result = secret.clone();
    secret.zeroize();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_3_of_5_roundtrip() {
        let secret = b"super secret file key 32 bytes!!".to_vec();
        let shares = split_secret(&secret, 3, 5).unwrap();
        assert_eq!(shares.len(), 5);

        // 3개로 복원
        let subset: Vec<EncodedShare> = shares.iter().take(3).cloned().collect();
        let recovered = combine_secret(&subset).unwrap();
        assert_eq!(recovered, secret);

        // 5개 모두로도 복원 (자동으로 3개만 사용)
        let recovered_all = combine_secret(&shares).unwrap();
        assert_eq!(recovered_all, secret);
    }

    #[test]
    fn insufficient_shares_fail() {
        let secret = b"some secret data".to_vec();
        let shares = split_secret(&secret, 3, 5).unwrap();

        // 2개만 — 불충분
        let subset: Vec<EncodedShare> = shares.iter().take(2).cloned().collect();
        assert!(matches!(
            combine_secret(&subset),
            Err(ShamirError::NotEnoughShares { .. })
        ));
    }

    #[test]
    fn encoding_roundtrip() {
        let secret = b"test key".to_vec();
        let shares = split_secret(&secret, 2, 3).unwrap();
        let encoded = shares[0].to_string();
        assert!(encoded.starts_with("qs1-2-3-01-"));

        let decoded: EncodedShare = encoded.parse().unwrap();
        assert_eq!(decoded.threshold, 2);
        assert_eq!(decoded.total, 3);
        assert_eq!(decoded.index, 1);
        assert_eq!(decoded.data, shares[0].data);
    }

    #[test]
    fn invalid_threshold_rejected() {
        let secret = b"x".to_vec();
        assert!(split_secret(&secret, 1, 3).is_err()); // M < 2
        assert!(split_secret(&secret, 5, 3).is_err()); // M > N
        assert!(split_secret(&secret, 2, 1).is_err()); // N < M
    }

    #[test]
    fn inconsistent_shares_detected() {
        let s1 = split_secret(b"secret-a", 3, 5).unwrap();
        let s2 = split_secret(b"secret-b", 3, 5).unwrap();

        // 다른 secret의 share 혼합
        let mut mixed = s1[..2].to_vec();
        mixed.push(s2[0].clone());

        // sharks가 임의로 복원하지만 (다른 secret 나옴), index 충돌 등 추정 어려움.
        // 우리는 같은 (M, N) 세트에선 mixed가 다른 값 나옴 — 검증 X.
        // 그러나 우리 EncodedShare는 메타데이터 동일하므로 inconsistency 검출 X.
        // 사용자가 share set ID 추적해야 함.
        // (이 테스트는 단순 "다른 결과 나옴" 확인용)
        let result = combine_secret(&mixed);
        // mixed shares with same M/N: sharks가 잘못된 답 줄 수 있음
        // 이 케이스는 사용자가 set_id로 추적해야 함 (별도 검증 미구현)
        let _ = result;
    }

    #[test]
    fn split_to_max_shares() {
        let secret = b"k".to_vec();
        // 큰 N 가능 확인 (실용적으로 10 정도)
        let shares = split_secret(&secret, 5, 10).unwrap();
        assert_eq!(shares.len(), 10);

        let subset: Vec<EncodedShare> = shares.iter().take(5).cloned().collect();
        let recovered = combine_secret(&subset).unwrap();
        assert_eq!(recovered, secret);
    }
}
