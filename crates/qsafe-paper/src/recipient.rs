//! Bip39Wrapper: FileKey 봉투화 + 단어 표시
//! unwrap_bip39: 사용자 입력 단어로 FileKey 복원

use crate::error::{PaperError, Result};
use crate::{BIP39_SALT_LEN, HKDF_INFO_BIP39_V1};
use bip39::{Language, Mnemonic};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use qsafe_core::envelope::{FileKey, FILE_KEY_LEN};
use qsafe_core::format::{Bip39Recipient, Recipient};
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use zeroize::Zeroize;

const NONCE_LEN: usize = 24;
const WRAP_KEY_LEN: usize = 32;

/// Generated mnemonic + 표시 가능한 단어 목록.
///
/// **이 구조체는 비밀 단어를 들고 있음**. 화면에 표시하고 즉시 drop.
pub struct GeneratedMnemonic {
    mnemonic: Mnemonic,
    word_count: u8,
}

impl GeneratedMnemonic {
    /// 24개 영어 단어로 새 mnemonic 생성.
    pub fn random_24() -> Result<Self> {
        let mnemonic = Mnemonic::generate_in(Language::English, 24)
            .map_err(|e: bip39::Error| PaperError::InvalidMnemonic(e.to_string()))?;
        Ok(Self {
            mnemonic,
            word_count: 24,
        })
    }

    pub fn random(word_count: u8) -> Result<Self> {
        match word_count {
            12 | 15 | 18 | 21 | 24 => {}
            other => return Err(PaperError::InvalidWordCount(other)),
        }
        let mnemonic = Mnemonic::generate_in(Language::English, word_count as usize)
            .map_err(|e: bip39::Error| PaperError::InvalidMnemonic(e.to_string()))?;
        Ok(Self {
            mnemonic,
            word_count,
        })
    }

    pub fn words(&self) -> Vec<&'static str> {
        self.mnemonic.words().collect()
    }

    pub fn word_count(&self) -> u8 {
        self.word_count
    }
}

/// BIP39 단어 봉투 빌더. 새 mnemonic 생성 + wrap.
pub struct Bip39Wrapper {
    mnemonic: GeneratedMnemonic,
    label: Option<String>,
}

impl Bip39Wrapper {
    /// 24단어 영어 mnemonic 새로 생성.
    pub fn generate() -> Result<Self> {
        Ok(Self {
            mnemonic: GeneratedMnemonic::random_24()?,
            label: None,
        })
    }

    pub fn generate_words(word_count: u8) -> Result<Self> {
        Ok(Self {
            mnemonic: GeneratedMnemonic::random(word_count)?,
            label: None,
        })
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn words(&self) -> Vec<&'static str> {
        self.mnemonic.words()
    }

    /// FileKey를 BIP39 단어로 봉투화.
    pub fn wrap(&self, file_key: &FileKey) -> Result<Recipient> {
        let mut salt = vec![0u8; BIP39_SALT_LEN];
        let mut nonce = vec![0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce);

        let seed = self.mnemonic.mnemonic.to_seed("");
        let mut wrap_key = derive_wrap_key(&seed, &salt)?;

        let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
        let xnonce = XNonce::from_slice(&nonce);
        let encrypted = cipher
            .encrypt(xnonce, file_key.as_bytes().as_ref())
            .map_err(|_| PaperError::Aead)?;
        wrap_key.zeroize();
        // seed는 to_seed 반환값이라 자동 drop, 64 bytes 정도라 큰 위험 X

        Ok(Recipient::Bip39(Bip39Recipient {
            salt,
            nonce,
            encrypted_file_key: encrypted,
            word_count: self.mnemonic.word_count,
            language: "english".to_string(),
            label: self.label.clone(),
        }))
    }
}

/// 사용자 입력 단어로 Bip39Recipient에서 FileKey 복원.
pub fn unwrap_bip39(words: &str, recipient: &Bip39Recipient) -> Result<FileKey> {
    if recipient.salt.len() != BIP39_SALT_LEN {
        return Err(PaperError::InvalidSalt);
    }
    if recipient.nonce.len() != NONCE_LEN {
        return Err(PaperError::InvalidNonce);
    }
    if recipient.language != "english" {
        return Err(PaperError::UnsupportedLanguage(recipient.language.clone()));
    }

    // BIP39 파싱 + 체크섬 검증 (오타 즉시 거부)
    let mnemonic = Mnemonic::parse_in(Language::English, words)
        .map_err(|e: bip39::Error| PaperError::InvalidMnemonic(e.to_string()))?;

    // 단어 개수 일치 확인
    let parsed_count = mnemonic.words().count() as u8;
    if parsed_count != recipient.word_count {
        return Err(PaperError::InvalidMnemonic(format!(
            "expected {} words, got {}",
            recipient.word_count, parsed_count
        )));
    }

    let seed = mnemonic.to_seed("");
    let mut wrap_key = derive_wrap_key(&seed, &recipient.salt)?;

    let cipher = XChaCha20Poly1305::new(wrap_key.as_slice().into());
    let xnonce = XNonce::from_slice(&recipient.nonce);
    let result = cipher.decrypt(xnonce, recipient.encrypted_file_key.as_slice());
    wrap_key.zeroize();

    let mut plaintext = result.map_err(|_| PaperError::Aead)?;
    if plaintext.len() != FILE_KEY_LEN {
        plaintext.zeroize();
        return Err(PaperError::InvalidFileKey);
    }
    let mut bytes = [0u8; FILE_KEY_LEN];
    bytes.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(FileKey::from_bytes(bytes))
}

/// 화면 표시용 24단어 포맷터. 4 단어/행, 1-based 번호.
pub fn display_words(words: &[&str]) -> String {
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if i % 4 == 0 && i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("{:>3}. {:<10} ", i + 1, w));
    }
    out
}

fn derive_wrap_key(seed: &[u8], salt: &[u8]) -> Result<[u8; WRAP_KEY_LEN]> {
    let hk = Hkdf::<Sha256>::new(Some(salt), seed);
    let mut out = [0u8; WRAP_KEY_LEN];
    hk.expand(HKDF_INFO_BIP39_V1, &mut out)
        .map_err(|e| PaperError::Hkdf(format!("expand: {}", e)))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_24_words() {
        let m = GeneratedMnemonic::random_24().unwrap();
        let words = m.words();
        assert_eq!(words.len(), 24);
        // 각 단어가 비어있지 않은지
        for w in words {
            assert!(!w.is_empty());
        }
    }

    #[test]
    fn wrap_unwrap_roundtrip() {
        let wrapper = Bip39Wrapper::generate().unwrap();
        let words = wrapper.words().join(" ");

        let file_key = FileKey::random();
        let original = *file_key.as_bytes();

        let r = wrapper.wrap(&file_key).unwrap();
        let br = match r {
            Recipient::Bip39(b) => b,
            _ => panic!(),
        };

        let recovered = unwrap_bip39(&words, &br).unwrap();
        assert_eq!(recovered.as_bytes(), &original);
    }

    #[test]
    fn wrong_words_fail() {
        let wrapper = Bip39Wrapper::generate().unwrap();
        let _words = wrapper.words().join(" ");
        let file_key = FileKey::random();
        let r = wrapper.wrap(&file_key).unwrap();
        let br = match r {
            Recipient::Bip39(b) => b,
            _ => panic!(),
        };

        // 완전 다른 24단어 (체크섬 통과해야 BIP39 파싱 성공)
        let other = Bip39Wrapper::generate().unwrap();
        let other_words = other.words().join(" ");
        assert!(unwrap_bip39(&other_words, &br).is_err());
    }

    #[test]
    fn invalid_checksum_rejected() {
        let wrapper = Bip39Wrapper::generate().unwrap();
        let file_key = FileKey::random();
        let r = wrapper.wrap(&file_key).unwrap();
        let br = match r {
            Recipient::Bip39(b) => b,
            _ => panic!(),
        };

        // 첫 단어를 다른 단어로 바꿔서 체크섬 깨뜨림
        let mut words: Vec<&str> = wrapper.words();
        words[0] = "zoo"; // 마지막 사전 단어, 거의 확실히 체크섬 깸
        let bad = words.join(" ");
        let err = unwrap_bip39(&bad, &br);
        // BIP39 파서 또는 AEAD에서 실패
        assert!(err.is_err());
    }

    #[test]
    fn one_word_typo_rejected_by_checksum() {
        // BIP39 sample 단어 24개
        let valid = Bip39Wrapper::generate().unwrap();
        let words = valid.words();
        let file_key = FileKey::random();
        let r = valid.wrap(&file_key).unwrap();
        let br = match r {
            Recipient::Bip39(b) => b,
            _ => panic!(),
        };

        // 단어 하나를 사전에 있지만 다른 단어로
        let mut tampered: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        // 다른 사전 단어로 교체 (체크섬 거의 확실히 깸)
        tampered[5] = if tampered[5] == "abandon" {
            "ability".to_string()
        } else {
            "abandon".to_string()
        };
        let bad = tampered.join(" ");
        assert!(unwrap_bip39(&bad, &br).is_err());
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        let wrapper = Bip39Wrapper::generate().unwrap();
        let words = wrapper.words().join(" ");
        let file_key = FileKey::random();
        let r = wrapper.wrap(&file_key).unwrap();
        let mut br = match r {
            Recipient::Bip39(b) => b,
            _ => panic!(),
        };
        br.encrypted_file_key[0] ^= 1;
        assert!(unwrap_bip39(&words, &br).is_err());
    }

    #[test]
    fn display_words_format_correct() {
        let words = vec![
            "one", "two", "three", "four", "five", "six", "seven", "eight",
        ];
        let formatted = display_words(&words);
        // 4단어 후 줄바꿈 있어야 함
        assert!(formatted.contains('\n'));
        // 번호 포함
        assert!(formatted.contains("1."));
        assert!(formatted.contains("8."));
    }
}
