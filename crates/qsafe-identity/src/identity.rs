//! Identity 키쌍 — X25519 + ML-KEM-768.
//!
//! 사용자는 자기 자신만의 identity 하나를 가짐. 직렬화 가능 (저장용).

use crate::error::{IdentityError, Result};
use ml_kem::{
    kem::{Decapsulate, DecapsulationKey, Encapsulate, EncapsulationKey},
    KemCore, MlKem768,
};
use ml_kem::array::Array;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519Pk, StaticSecret as X25519Sk};
use zeroize::Zeroize;

/// 공개 가능한 identity 정보 (공유용).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityPublic {
    /// X25519 공개키 (32 bytes)
    #[serde(with = "serde_bytes")]
    pub x25519_pk: Vec<u8>,
    /// ML-KEM-768 공개키 (1184 bytes)
    #[serde(with = "serde_bytes")]
    pub mlkem768_pk: Vec<u8>,
}

impl IdentityPublic {
    /// 짧은 fingerprint (BLAKE3 처음 8 bytes의 hex).
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.x25519_pk);
        hasher.update(&self.mlkem768_pk);
        let result = hasher.finalize();
        hex::encode(&result[..8])
    }

    /// MLKEM 공개키 hash (8 bytes) — 헤더에 짧게 저장.
    pub fn mlkem_pk_hash(&self) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(&self.mlkem768_pk);
        hasher.finalize()[..8].to_vec()
    }
}

/// 비밀 키 포함 전체 identity. **절대 직렬화/공유 금지** 외부에는 IdentityPublic만.
pub struct Identity {
    pub x25519_sk: X25519Sk,
    pub x25519_pk_bytes: [u8; 32],
    pub mlkem768_sk: DecapsulationKey<MlKem768>,
    pub mlkem768_pk: EncapsulationKey<MlKem768>,
    pub mlkem768_pk_bytes: Vec<u8>,
}

impl Identity {
    /// 새 identity 생성 (CSPRNG).
    pub fn generate() -> Self {
        let mut rng = OsRng;

        // X25519
        let x25519_sk = X25519Sk::random_from_rng(&mut rng);
        let x25519_pk_bytes = X25519Pk::from(&x25519_sk).to_bytes();

        // ML-KEM-768
        let (mlkem768_sk, mlkem768_pk) = MlKem768::generate(&mut rng);
        let mlkem768_pk_bytes = mlkem768_pk.as_bytes().to_vec();

        Self {
            x25519_sk,
            x25519_pk_bytes,
            mlkem768_sk,
            mlkem768_pk,
            mlkem768_pk_bytes,
        }
    }

    /// 공개키 부분만 반환.
    pub fn public(&self) -> IdentityPublic {
        IdentityPublic {
            x25519_pk: self.x25519_pk_bytes.to_vec(),
            mlkem768_pk: self.mlkem768_pk_bytes.clone(),
        }
    }

    pub fn fingerprint(&self) -> String {
        self.public().fingerprint()
    }
}

/// 직렬화 가능한 비밀 키 묶음 (저장용).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentitySecretBytes {
    #[serde(with = "serde_bytes")]
    pub x25519_sk: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub mlkem768_sk: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub mlkem768_pk: Vec<u8>,
}

impl IdentitySecretBytes {
    /// Identity → 직렬화 가능 바이트.
    pub fn from_identity(id: &Identity) -> Self {
        Self {
            x25519_sk: id.x25519_sk.to_bytes().to_vec(),
            mlkem768_sk: id.mlkem768_sk.as_bytes().to_vec(),
            mlkem768_pk: id.mlkem768_pk_bytes.clone(),
        }
    }

    /// 직렬화 바이트 → Identity (역직렬화).
    pub fn to_identity(&self) -> Result<Identity> {
        if self.x25519_sk.len() != 32 {
            return Err(IdentityError::InvalidX25519SkLen(self.x25519_sk.len()));
        }
        let mut sk_arr = [0u8; 32];
        sk_arr.copy_from_slice(&self.x25519_sk);
        let x25519_sk = X25519Sk::from(sk_arr);
        sk_arr.zeroize();
        let x25519_pk_bytes = X25519Pk::from(&x25519_sk).to_bytes();

        // ML-KEM 키 복원
        let mlkem_sk_arr: Array<u8, _> = Array::try_from(self.mlkem768_sk.as_slice())
            .map_err(|_| IdentityError::InvalidMlkemSk)?;
        let mlkem768_sk = <MlKem768 as KemCore>::DecapsulationKey::from_bytes(&mlkem_sk_arr);

        let mlkem_pk_arr: Array<u8, _> = Array::try_from(self.mlkem768_pk.as_slice())
            .map_err(|_| IdentityError::InvalidMlkemPkLen)?;
        let mlkem768_pk = <MlKem768 as KemCore>::EncapsulationKey::from_bytes(&mlkem_pk_arr);

        Ok(Identity {
            x25519_sk,
            x25519_pk_bytes,
            mlkem768_sk,
            mlkem768_pk,
            mlkem768_pk_bytes: self.mlkem768_pk.clone(),
        })
    }
}

impl Drop for IdentitySecretBytes {
    fn drop(&mut self) {
        self.x25519_sk.zeroize();
        self.mlkem768_sk.zeroize();
        // pk는 비밀 아니므로 zeroize 안 함
    }
}
