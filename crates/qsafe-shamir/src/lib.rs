//! qsafe-shamir — Shamir M-of-N 비밀 분할.
//!
//! ## 사용 시나리오
//!
//! - **가족 분산**: 5명 중 3명 동의 시 풀림
//! - **다중 위치 백업**: 본인/은행/변호사/친구/금고에 분산
//! - **사망 후 상속**: 가족이 협력하여 복구
//! - **단일 장애점 제거**: 종이 한 장 잃어도 OK
//!
//! ## 알고리즘
//!
//! Adi Shamir (1979) GF(256) 다항식 보간법.
//! 각 byte를 독립적으로 분할 (병렬화 가능).
//!
//! ## 보안
//!
//! - M-1 share 이하로는 정보 0 (수학적 증명)
//! - M share = 정확히 복원
//! - share 자체는 비밀이 아님 (X 좌표) — 단, 노출 시 한 카운트 줄어듦
//! - 각 share는 인덱스(1 byte) + 데이터로 구성

pub mod error;
pub mod share;

pub use error::{ShamirError, Result};
pub use share::{combine_secret, split_secret, EncodedShare, MAX_SHARES, MIN_SHARES};
