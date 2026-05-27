//! Tauri commands exposed to the JavaScript frontend.
//!
//! Each `#[tauri::command]` is callable from the UI via `invoke("name", { args })`.
//! Commands intentionally take JSON-friendly types and return `Result<T, String>`
//! so the frontend gets clean strings on error.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// 0600 권한으로 secret JSON 파일 작성 (Unix). Windows는 일반 write.
fn write_secret_json(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(data)?;
    f.sync_all()?;
    Ok(())
}

#[derive(Serialize)]
pub struct AboutInfo {
    pub version: &'static str,
    pub cipher_suite: &'static str,
    pub features: Vec<&'static str>,
}

/// 사용자 UI 하단의 "About" 패널에 표시할 정보.
#[tauri::command]
pub fn about() -> AboutInfo {
    AboutInfo {
        version: env!("CARGO_PKG_VERSION"),
        cipher_suite: "XChaCha20-Poly1305 + BLAKE3 + Argon2id (V1Xchacha20Blake3)",
        features: vec![
            "Password (Argon2id)",
            "FIDO2 PRF",
            "BIP39 paper backup",
            "Shamir M-of-N",
            "X25519 + ML-KEM-768 hybrid",
            "External archives (RAR/ZIP/7Z/TAR/GZ/XZ/BZ2/LZ4/ZSTD/Brotli)",
        ],
    }
}

#[derive(Serialize)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub kind: String, // "qsafe" | "archive" | "plain"
    pub header_summary: Option<String>,
}

/// 사용자가 파일을 드롭하거나 선택했을 때 메타 정보를 미리 보기.
/// — `.qs` 파일이면 헤더만 읽어 라벨/recipients 요약을 만든다.
/// — 외부 아카이브면 magic byte로 종류를 식별 (수정 안 함).
/// — 평범한 파일이면 크기만 보여준다.
#[tauri::command]
pub fn file_info(path: String) -> Result<FileInfo, String> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("stat: {}", e))?;
    let size = meta.len();

    // .qs 헤더 sniff
    if let Ok(bytes) = std::fs::read(&p) {
        if bytes.len() >= 8 && &bytes[..8] == qsafe_core::format::MAGIC {
            // PackedFile parse는 streaming 헤더와 batch 헤더 두 가지 — 가능한 한 쪽만 시도.
            let summary = match qsafe_core::io::read_packed_file(bytes.as_slice()) {
                Ok(pf) => format!(
                    "{} recipients · {} bytes original · suite={:?}",
                    pf.header.recipients.len(),
                    pf.header.original_size,
                    pf.header.suite
                ),
                Err(_) => "qsafe streaming format (header-only preview not implemented yet)".into(),
            };
            return Ok(FileInfo {
                path,
                size,
                kind: "qsafe".into(),
                header_summary: Some(summary),
            });
        }
    }

    Ok(FileInfo {
        path,
        size,
        kind: "plain".into(),
        header_summary: None,
    })
}

#[derive(Serialize)]
pub struct IdentitySummary {
    pub fingerprint: String,
    pub x25519_pk_len: usize,
    pub mlkem768_pk_len: usize,
    pub path: String,
}

/// 새 identity 키쌍을 만들고 JSON으로 저장한다.
#[tauri::command]
pub fn identity_generate(output_path: String, force: bool) -> Result<IdentitySummary, String> {
    let path = PathBuf::from(&output_path);
    if path.exists() && !force {
        return Err(format!(
            "파일이 이미 존재합니다: {} (force=true 로 덮어쓰기 가능)",
            path.display()
        ));
    }
    let identity = qsafe_identity::Identity::generate();
    let secret = qsafe_identity::IdentitySecretBytes::from_identity(&identity);
    let json = serde_json::to_vec_pretty(&secret).map_err(|e| e.to_string())?;
    // secret JSON은 0600 권한 (Unix). create_new로 race-free.
    write_secret_json(&path, &json).map_err(|e| format!("write: {}", e))?;

    Ok(IdentitySummary {
        fingerprint: identity.fingerprint(),
        x25519_pk_len: identity.x25519_pk_bytes.len(),
        mlkem768_pk_len: identity.mlkem768_pk_bytes.len(),
        path: output_path,
    })
}

/// 이미 저장된 identity JSON 파일에서 요약을 읽어온다 (secret 또는 public 모두 허용).
#[tauri::command]
pub fn identity_show(path: String) -> Result<IdentitySummary, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("read: {}", e))?;

    if let Ok(secret) = serde_json::from_slice::<qsafe_identity::IdentitySecretBytes>(&bytes) {
        let identity = secret.to_identity().map_err(|e| e.to_string())?;
        return Ok(IdentitySummary {
            fingerprint: identity.fingerprint(),
            x25519_pk_len: identity.x25519_pk_bytes.len(),
            mlkem768_pk_len: identity.mlkem768_pk_bytes.len(),
            path,
        });
    }

    let public: qsafe_identity::IdentityPublic =
        serde_json::from_slice(&bytes).map_err(|e| format!("not a qsafe identity JSON: {}", e))?;
    Ok(IdentitySummary {
        fingerprint: public.fingerprint(),
        x25519_pk_len: public.x25519_pk.len(),
        mlkem768_pk_len: public.mlkem768_pk.len(),
        path,
    })
}
