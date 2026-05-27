//! PubkeyWrapper / unwrap_pubkey — X25519+ML-KEM-768 하이브리드 KEM.

use crate::error::{IdentityError, Result};
use crate::identity::{Identity, IdentityPublic};
use crate::HKDF_INFO_PQ_HYBRID_V1;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use ml_kem::array::Array;
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{Ciphertext, EncodedSizeUser, KemCore, MlKem768, SharedKey};
use qsafe_core::envelope::{FileKey, FILE_KEY_LEN};
use qsafe_core::format::{PubkeyRecipient, Recipient};
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519Pk, StaticSecret as X25519Sk};
use zeroize::Zeroize;

const NONCE_LEN: usize = 24;
const WRAP_KEY_LEN: usize = 32;

pub struct PubkeyWrapper {
    recipient: IdentityPublic,
}

impl PubkeyWrapper {
    pub fn new(recipient: IdentityPublic) -> Self {
        Self { recipient }
    }

    /// 수신자에게 FileKey를 봉투화.
    pub fn wrap(&self, file_key: &FileKey) -> Result<Recipient> {
        // ── 입력 검증 ─────────────────────────────────────
        if self.recipient.x25519_pk.len() != 32 {
            return Err(IdentityError::InvalidX25519PkLen(
                self.recipient.x25519_pk.len(),
            ));
        }

        // ── 1. 임시 X25519 키쌍 + ECDH ─────────────────────
        let eph_x25519_sk = X25519Sk::random_from_rng(OsRng);
        let eph_x25519_pk_bytes = X25519Pk::from(&eph_x25519_sk).to_bytes();

        let mut recipient_x25519_pk_arr = [0u8; 32];
        recipient_x25519_pk_arr.copy_from_slice(&self.recipient.x25519_pk);
        let recipient_x25519_pk = X25519Pk::from(recipient_x25519_pk_arr);
        let mut x25519_shared = eph_x25519_sk
            .diffie_hellman(&recipient_x25519_pk)
            .to_bytes();

        // ── 2. ML-KEM 캡슐화 ───────────────────────────────
        let mlkem_pk_arr: Array<u8, _> = Array::try_from(self.recipient.mlkem768_pk.as_slice())
            .map_err(|_| IdentityError::InvalidMlkemPkLen)?;
        let mlkem_pk = <MlKem768 as KemCore>::EncapsulationKey::from_bytes(&mlkem_pk_arr);
        // ml-kem 0.2.3: Encapsulate<EK, SS> is generic over both EK and SS, so we annotate
        // the impl pair (EncodedCiphertext<MlKem768Params>, SharedKey<MlKem768>) explicitly.
        let (mlkem_ct, mut mlkem_shared_arr): (Ciphertext<MlKem768>, SharedKey<MlKem768>) =
            mlkem_pk
                .encapsulate(&mut OsRng)
                .map_err(|_| IdentityError::MlkemDecapFailed)?;
        let mlkem_ct_bytes = mlkem_ct.as_slice().to_vec();

        // ── 3. wrap_key = HKDF(...) ────────────────────────
        let mut wrap_key = derive_wrap_key(
            &x25519_shared,
            mlkem_shared_arr.as_slice(),
            &eph_x25519_pk_bytes,
            &self.recipient.x25519_pk,
            &mlkem_ct_bytes,
            &self.recipient.mlkem768_pk,
        )?;
        x25519_shared.zeroize();
        mlkem_shared_arr.zeroize();

        // ── 4. AEAD ────────────────────────────────────────
        let mut nonce = vec![0u8; NONCE_LEN];
        use rand::RngCore;
        OsRng.fill_bytes(&mut nonce);

        let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
        let xnonce = XNonce::from_slice(&nonce);
        let encrypted = cipher
            .encrypt(xnonce, file_key.as_bytes().as_ref())
            .map_err(|_| IdentityError::Aead)?;
        wrap_key.zeroize();

        // ── 5. PubkeyRecipient 생성 ───────────────────────
        let mlkem_pk_hash = self.recipient.mlkem_pk_hash();

        Ok(Recipient::Pubkey(PubkeyRecipient {
            recipient_x25519_pk: self.recipient.x25519_pk.clone(),
            ephemeral_x25519_pk: eph_x25519_pk_bytes.to_vec(),
            mlkem768_ct: mlkem_ct_bytes,
            recipient_mlkem768_pk_hash: mlkem_pk_hash,
            nonce,
            encrypted_file_key: encrypted,
        }))
    }
}

/// PubkeyRecipient에서 자기 identity로 FileKey 복원.
pub fn unwrap_pubkey(identity: &Identity, recipient: &PubkeyRecipient) -> Result<FileKey> {
    // ── 1. 수신자 검증 ─────────────────────────────────────
    if recipient.recipient_x25519_pk != identity.x25519_pk_bytes {
        return Err(IdentityError::RecipientMismatch);
    }
    let my_mlkem_hash = {
        use sha2::Digest;
        let mut h = Sha256::new();
        h.update(&identity.mlkem768_pk_bytes);
        h.finalize()[..8].to_vec()
    };
    if recipient.recipient_mlkem768_pk_hash != my_mlkem_hash {
        return Err(IdentityError::RecipientMismatch);
    }

    if recipient.ephemeral_x25519_pk.len() != 32 {
        return Err(IdentityError::InvalidEphemeralKey);
    }
    if recipient.nonce.len() != NONCE_LEN {
        return Err(IdentityError::Aead);
    }

    // ── 2. X25519 ECDH ─────────────────────────────────────
    let mut eph_pk_arr = [0u8; 32];
    eph_pk_arr.copy_from_slice(&recipient.ephemeral_x25519_pk);
    let eph_pk = X25519Pk::from(eph_pk_arr);
    let mut x25519_shared = identity.x25519_sk.diffie_hellman(&eph_pk).to_bytes();

    // ── 3. ML-KEM 복호화 ───────────────────────────────────
    let ct_arr: Array<u8, _> = Array::try_from(recipient.mlkem768_ct.as_slice())
        .map_err(|_| IdentityError::InvalidMlkemCtLen)?;
    let mut mlkem_shared = identity
        .mlkem768_sk
        .decapsulate(&ct_arr)
        .map_err(|_| IdentityError::MlkemDecapFailed)?;

    // ── 4. 같은 wrap_key 도출 ──────────────────────────────
    let mut wrap_key = derive_wrap_key(
        &x25519_shared,
        mlkem_shared.as_slice(),
        &recipient.ephemeral_x25519_pk,
        &recipient.recipient_x25519_pk,
        &recipient.mlkem768_ct,
        &identity.mlkem768_pk_bytes,
    )?;
    x25519_shared.zeroize();
    mlkem_shared.zeroize();

    // ── 5. AEAD ─────────────────────────────────────────────
    let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
    let xnonce = XNonce::from_slice(&recipient.nonce);
    let result = cipher.decrypt(xnonce, recipient.encrypted_file_key.as_slice());
    wrap_key.zeroize();

    let mut plaintext = result.map_err(|_| IdentityError::Aead)?;
    if plaintext.len() != FILE_KEY_LEN {
        plaintext.zeroize();
        return Err(IdentityError::InvalidFileKey);
    }
    let mut bytes = [0u8; FILE_KEY_LEN];
    bytes.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(FileKey::from_bytes(bytes))
}

/// HKDF 결합. transcript에 모든 공개 값 포함 → MitM 방어.
#[allow(clippy::too_many_arguments)]
fn derive_wrap_key(
    x25519_shared: &[u8],
    mlkem_shared: &[u8],
    eph_x25519_pk: &[u8],
    recipient_x25519_pk: &[u8],
    mlkem_ct: &[u8],
    recipient_mlkem_pk: &[u8],
) -> Result<[u8; WRAP_KEY_LEN]> {
    // IKM = X25519_shared || MLKEM_shared
    // transcript salt = 모든 공개 값의 해시
    let mut ikm = Vec::with_capacity(x25519_shared.len() + mlkem_shared.len());
    ikm.extend_from_slice(x25519_shared);
    ikm.extend_from_slice(mlkem_shared);

    use sha2::Digest;
    let mut transcript = Sha256::new();
    transcript.update(eph_x25519_pk);
    transcript.update(recipient_x25519_pk);
    transcript.update(mlkem_ct);
    transcript.update(recipient_mlkem_pk);
    let salt = transcript.finalize();

    let hk = Hkdf::<Sha256>::new(Some(salt.as_slice()), &ikm);
    let mut out = [0u8; WRAP_KEY_LEN];
    hk.expand(HKDF_INFO_PQ_HYBRID_V1, &mut out)
        .map_err(|e| IdentityError::Hkdf(format!("expand: {}", e)))?;

    ikm.zeroize();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pubkey_roundtrip() {
        let identity = Identity::generate();
        let public = identity.public();

        let file_key = FileKey::random();
        let original = *file_key.as_bytes();

        let r = PubkeyWrapper::new(public).wrap(&file_key).unwrap();
        let pr = match r {
            Recipient::Pubkey(p) => p,
            _ => panic!(),
        };

        let recovered = unwrap_pubkey(&identity, &pr).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }

    #[test]
    fn wrong_identity_fails() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let fk = FileKey::random();
        let r = PubkeyWrapper::new(alice.public()).wrap(&fk).unwrap();
        let pr = match r {
            Recipient::Pubkey(p) => p,
            _ => panic!(),
        };

        // Bob이 풀려고 하면 recipient mismatch
        assert!(matches!(
            unwrap_pubkey(&bob, &pr),
            Err(IdentityError::RecipientMismatch)
        ));
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        let identity = Identity::generate();
        let fk = FileKey::random();
        let r = PubkeyWrapper::new(identity.public()).wrap(&fk).unwrap();
        let mut pr = match r {
            Recipient::Pubkey(p) => p,
            _ => panic!(),
        };
        pr.encrypted_file_key[0] ^= 1;
        assert!(unwrap_pubkey(&identity, &pr).is_err());
    }

    #[test]
    fn tampered_ephemeral_pk_rejected() {
        let identity = Identity::generate();
        let fk = FileKey::random();
        let r = PubkeyWrapper::new(identity.public()).wrap(&fk).unwrap();
        let mut pr = match r {
            Recipient::Pubkey(p) => p,
            _ => panic!(),
        };
        pr.ephemeral_x25519_pk[0] ^= 1;
        // 다른 x25519_shared → 다른 wrap_key → AEAD 실패
        assert!(unwrap_pubkey(&identity, &pr).is_err());
    }

    #[test]
    fn identity_serialize_roundtrip() {
        use crate::identity::IdentitySecretBytes;
        let identity = Identity::generate();
        let pub_before = identity.public();

        let bytes = IdentitySecretBytes::from_identity(&identity);
        let identity_back = bytes.to_identity().unwrap();
        let pub_after = identity_back.public();

        assert_eq!(pub_before.x25519_pk, pub_after.x25519_pk);
        assert_eq!(pub_before.mlkem768_pk, pub_after.mlkem768_pk);

        // 새 identity로 wrap, 복원된 것으로 unwrap
        let fk = FileKey::random();
        let original = *fk.as_bytes();
        let r = PubkeyWrapper::new(identity.public()).wrap(&fk).unwrap();
        let pr = match r {
            Recipient::Pubkey(p) => p,
            _ => panic!(),
        };
        let recovered = unwrap_pubkey(&identity_back, &pr).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }
}
