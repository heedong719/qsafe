//! 사용자 FIDO2 credential 저장소.
//!
//! 위치: ~/.config/qsafe/credentials.json (Linux/Mac)
//!       %APPDATA%/qsafe/credentials.json (Windows)
//!
//! credential_id는 비밀이 아니지만 (CTAP 표준), 파일 권한은 0600으로 보호.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    pub name: String,
    /// hex-encoded credential ID
    pub credential_id_hex: String,
    pub rp_id: String,
    pub created_at_unix: i64,
    pub label: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CredentialStore {
    pub credentials: Vec<StoredCredential>,
}

impl CredentialStore {
    pub fn default_path() -> Result<PathBuf> {
        let base = if cfg!(windows) {
            std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| dirs_home().join("AppData").join("Roaming"))
        } else {
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| dirs_home().join(".config"))
        };
        Ok(base.join("qsafe").join("credentials.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let store: CredentialStore =
            serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::default_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;

        // POSIX 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)?;
        }
        Ok(())
    }

    #[allow(dead_code)] // fido2-hw feature gate 내에서만 호출됨
    pub fn add(&mut self, cred: StoredCredential) -> Result<()> {
        if self.credentials.iter().any(|c| c.name == cred.name) {
            return Err(anyhow!("이미 존재하는 이름: {}", cred.name));
        }
        self.credentials.push(cred);
        Ok(())
    }

    #[allow(dead_code)] // fido2-hw feature gate 내에서만 호출됨
    pub fn find(&self, name: &str) -> Option<&StoredCredential> {
        self.credentials.iter().find(|c| c.name == name)
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        let len_before = self.credentials.len();
        self.credentials.retain(|c| c.name != name);
        if self.credentials.len() == len_before {
            return Err(anyhow!("이름을 찾을 수 없음: {}", name));
        }
        Ok(())
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
