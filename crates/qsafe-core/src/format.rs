//! qsafe v2 파일 포맷 — 다중 수신자 봉투 (age 영감)
//!
//! 핵심 원칙:
//! - 파일은 랜덤 FileKey(32B)로 한 번만 암호화됨
//! - FileKey는 N개의 수신자(Recipient) 방식으로 각각 봉투화됨
//! - 어떤 한 수신자라도 성공하면 FileKey 복원 → 복호화 가능
//! - 어떤 블록체인이 사라져도 다른 수신자가 살아있으면 파일은 안전
//!
//! 파일 구조:
//!   [MAGIC 8B][HEADER_LEN u32 LE][HEADER CBOR][PAYLOAD_LEN u64 LE][PAYLOAD][BLAKE3 32B]
//!
//! 모든 알고리즘은 named cipher suite로 명시 → crypto-agility 보장.

use serde::{Deserialize, Serialize};

/// qsafe v2 매직넘버 (v1과 다르게 설계되어 비호환)
pub const MAGIC: &[u8; 8] = b"QSAFE001";

/// 현재 포맷 버전. 마이그레이션 시 증가.
pub const VERSION: u16 = 2;

/// 명명된 cipher suite — 알고리즘 교체 시 새 suite 이름 추가
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CipherSuite {
    /// XChaCha20-Poly1305 + BLAKE3 + Argon2id (2026 baseline)
    V1Xchacha20Blake3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionAlgo {
    None,
    Zstd,
    Bzip3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegrityAlgo {
    Blake3,
}

// ─── 수신자 (Recipient) 정의 ──────────────────────────────────

/// 한 파일에는 여러 수신자가 있을 수 있다.
/// 각 수신자는 동일한 FileKey를 자기 방식으로 봉투화한다.
/// **단 하나만 충족돼도 복호화 가능 = 단일 의존 제거**
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Recipient {
    Password(PasswordRecipient),
    /// FIDO2 PRF (hmac-secret) 기반 — YubiKey, Nitrokey 등 하드웨어 키.
    /// 키 내부 secret이 절대 밖으로 나오지 않음. Touch 필요.
    Fido2(Fido2Recipient),
    /// BIP39 24단어 종이 백업 — 사용자가 화면에 표시된 단어를 종이에 적어 보관.
    /// 네트워크 없이 100% 오프라인. 단어를 다 잃으면 영구 손실.
    Bip39(Bip39Recipient),
    Timelock(TimelockRecipient),
    /// X25519 + ML-KEM-768 하이브리드 공개키 (PQ 안전).
    Pubkey(PubkeyRecipient),
    ShamirCommitment(ShamirCommitmentRecipient),
}

/// 패스워드 기반 수신자 (Argon2id + XChaCha20-Poly1305)
/// **언제나 가장 안정적인 fallback** — 블록체인 없이도 동작
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordRecipient {
    #[serde(with = "serde_bytes")]
    pub salt: Vec<u8>,
    pub argon2_m_kib: u32,
    pub argon2_t: u32,
    pub argon2_p: u32,
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub encrypted_file_key: Vec<u8>,
}

/// FIDO2 hmac-secret 수신자.
///
/// 동작 원리:
///   1. 사용자가 미리 키에 등록된 credential_id 보유
///   2. qsafe이 salt를 헤더에 평문 저장 (salt 자체는 비밀이 아님)
///   3. 복호화 시 키에 GetAssertion(credential_id, salt) 호출
///   4. 키 내부에서 HMAC-SHA256(device_secret, salt) 계산 → 출력
///   5. 출력을 HKDF로 wrap_key 도출 → encrypted_file_key 복호화
///
/// device_secret은 키를 떠나지 않음. 키가 없으면 절대 복원 불가능.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fido2Recipient {
    /// CTAP credential ID (등록 시 발급, 비밀 아님)
    #[serde(with = "serde_bytes")]
    pub credential_id: Vec<u8>,
    /// Relying Party ID (qsafe.local 등)
    pub rp_id: String,
    /// hmac-secret salt (32B, 매 봉투마다 무작위)
    #[serde(with = "serde_bytes")]
    pub hmac_salt: Vec<u8>,
    /// 사용자 검증(UV) 요구 여부 (PIN/지문)
    pub user_verification_required: bool,
    /// 봉투 AEAD nonce
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// 암호화된 FileKey
    #[serde(with = "serde_bytes")]
    pub encrypted_file_key: Vec<u8>,
    /// 사용자가 식별하기 쉽도록 하는 라벨 (예: "yubikey-mini")
    pub label: Option<String>,
}

/// BIP39 (24-word mnemonic) 종이 백업 수신자.
///
/// 동작 원리:
///   1. 암호화 시점에 무작위 256-bit entropy 생성 → 24개 영어 단어로 인코딩
///   2. 사용자에게 화면에 단어 표시 → 종이에 적어 보관 (절대 디지털 저장 X)
///   3. wrap_key = HKDF(BIP39_seed(words, ""), salt, "qsafe-v1-bip39")
///   4. AEAD(wrap_key, nonce, file_key) → encrypted_file_key
///   5. 헤더에는 salt + nonce + 암호문만 저장 (단어 자체는 X)
///
/// 복구 시 사용자가 24단어 입력 → 같은 wrap_key 도출 → 복호화.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bip39Recipient {
    /// HKDF salt (16 bytes, 무작위)
    #[serde(with = "serde_bytes")]
    pub salt: Vec<u8>,
    /// AEAD nonce
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// 암호화된 FileKey
    #[serde(with = "serde_bytes")]
    pub encrypted_file_key: Vec<u8>,
    /// 단어 개수 (보통 24, 가능: 12/15/18/21/24)
    pub word_count: u8,
    /// BIP39 언어 (현재는 "english"만 지원)
    pub language: String,
    /// 식별 라벨
    pub label: Option<String>,
}

/// 시간 잠금 수신자 (drand tlock)
/// 블록체인이 아닌 비콘 네트워크 사용 (consortium, 토큰 無)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelockRecipient {
    /// drand chain hash (예: quicknet)
    pub drand_chain_hash: String,
    /// 잠금 해제 라운드 번호
    pub round: u64,
    /// 잠금 해제 예정 UNIX 시각 (메타데이터, 비콘이 진실)
    pub unlock_at_unix: i64,
    /// tlock IBE 봉투 (FileKey 암호화)
    #[serde(with = "serde_bytes")]
    pub tlock_envelope: Vec<u8>,
}

/// 공개키 수신자 (X25519 + 선택적 ML-KEM-768 하이브리드)
/// 사용자가 보유한 키파일로 봉투 해제. 어떤 체인도 의존 X.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubkeyRecipient {
    /// 수신자의 X25519 공개키 (식별용)
    #[serde(with = "serde_bytes")]
    pub recipient_x25519_pk: Vec<u8>,
    /// 임시 X25519 공개키 (ECDH용)
    #[serde(with = "serde_bytes")]
    pub ephemeral_x25519_pk: Vec<u8>,
    /// (옵션) ML-KEM-768 ciphertext — PQ 하이브리드 시 필요
    #[serde(with = "serde_bytes")]
    pub mlkem768_ct: Vec<u8>,
    /// (옵션) 수신자의 ML-KEM-768 공개키 해시 (식별용)
    #[serde(with = "serde_bytes")]
    pub recipient_mlkem768_pk_hash: Vec<u8>,
    /// 봉투 nonce
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// 암호화된 FileKey
    #[serde(with = "serde_bytes")]
    pub encrypted_file_key: Vec<u8>,
}

/// Shamir 백업 commitment 수신자.
/// 실제 share는 파일 외부에 저장됨 (별도 .share 파일).
/// 이 entry는 "이 파일은 M-of-N Shamir 백업이 있음"을 기록.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShamirCommitmentRecipient {
    pub threshold: u8,
    pub total_shares: u8,
    /// Shamir share set의 고유 ID (충돌 방지 + 식별)
    pub set_id: String,
    /// 각 share의 식별 해시 (어떤 share인지 매칭용)
    pub share_id_hashes: Vec<String>,
}

// ─── 파일 헤더 ───────────────────────────────────────────────

/// 스트리밍 청크 정보 (큰 파일을 청크 단위로 AEAD).
///
/// 사용 시: 100MB+ 큰 파일 → 4MB 청크로 나눠 메모리 효율적 처리.
/// `payload_nonce`는 base_nonce(20 bytes) + reserved(4 bytes)로 해석.
/// 각 청크 nonce = base_nonce || chunk_idx_be(4 bytes).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkInfo {
    /// 청크 하나의 원본 크기 (마지막 제외 모두 동일)
    pub chunk_size: u32,
    /// 총 청크 개수
    pub num_chunks: u32,
    /// 마지막 청크의 원본 크기 (마지막은 chunk_size보다 작을 수 있음)
    pub last_chunk_size: u32,
}

/// 분할 압축 정보 (파일이 여러 part로 나뉜 경우).
///
/// 사용 예: `huge.iso.qs.part1`, `huge.iso.qs.part2`, ...
/// 모든 part는 같은 `series_id`를 공유하며 같은 페이로드의 chunk를 담는다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitInfo {
    /// 같은 시리즈 식별 (랜덤 16 bytes hex)
    pub series_id: String,
    /// 0-based 인덱스 (현재 part 번호)
    pub part_index: u32,
    /// 총 part 개수
    pub total_parts: u32,
    /// part 크기 (마지막 제외 모두 동일)
    pub part_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHeader {
    /// 포맷 버전
    pub version: u16,
    /// 사용된 cipher suite (알고리즘 묶음)
    pub suite: CipherSuite,
    /// 압축 알고리즘
    pub compression: CompressionAlgo,
    /// 무결성 해시 알고리즘
    pub integrity: IntegrityAlgo,
    /// 다중 수신자 (1개 이상 필수, 첫 번째는 보통 Password)
    pub recipients: Vec<Recipient>,
    /// 페이로드 nonce (FileKey + 이 nonce로 payload 암호화)
    #[serde(with = "serde_bytes")]
    pub payload_nonce: Vec<u8>,
    /// 원본 파일 크기 (검증용)
    pub original_size: u64,
    /// 생성 시각 (UNIX, UTC)
    pub created_at_unix: i64,
    /// 사용자 메타데이터 (선택)
    pub label: Option<String>,
    /// 분할 압축 정보 (없으면 단일 파일)
    pub split: Option<SplitInfo>,
    /// 스트리밍 청크 정보 (None = batch 모드, Some = streaming)
    pub chunks: Option<ChunkInfo>,
}

impl FileHeader {
    pub fn new(suite: CipherSuite, compression: CompressionAlgo) -> Self {
        Self {
            version: VERSION,
            suite,
            compression,
            integrity: IntegrityAlgo::Blake3,
            recipients: Vec::new(),
            payload_nonce: Vec::new(),
            original_size: 0,
            created_at_unix: 0,
            label: None,
            split: None,
            chunks: None,
        }
    }

    /// 헤더 일관성 검증.
    pub fn validate(&self) -> crate::Result<()> {
        if self.recipients.is_empty() {
            return Err(crate::CoreError::EmptyRecipients);
        }
        // payload_nonce 길이 검증 — suite별
        let expected_nonce_len = match self.suite {
            CipherSuite::V1Xchacha20Blake3 => 24,
        };
        if self.payload_nonce.len() != expected_nonce_len {
            return Err(crate::CoreError::InvalidHeaderField("payload_nonce length"));
        }
        // version은 decode에서 이미 검증
        Ok(())
    }

    /// CBOR로 직렬화
    pub fn encode(&self) -> crate::Result<Vec<u8>> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .map_err(|e| crate::CoreError::HeaderEncode(e.to_string()))?;
        Ok(buf)
    }

    /// CBOR로 역직렬화
    pub fn decode(bytes: &[u8]) -> crate::Result<Self> {
        let header: FileHeader = ciborium::from_reader(bytes)
            .map_err(|e| crate::CoreError::HeaderDecode(e.to_string()))?;
        if header.version > VERSION {
            return Err(crate::CoreError::UnsupportedVersion(header.version));
        }
        Ok(header)
    }
}
