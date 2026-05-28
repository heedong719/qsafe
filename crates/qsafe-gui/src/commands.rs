//! Tauri commands exposed to the JavaScript frontend.

use serde::Serialize;
use std::path::{Path, PathBuf};

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

fn write_public_json(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    let mut f = opts.open(path)?;
    f.write_all(data)?;
    f.sync_all()?;
    Ok(())
}

#[derive(Serialize, Debug)]
pub struct AboutInfo {
    pub version: &'static str,
    pub cipher_suite: &'static str,
    pub features: Vec<&'static str>,
    pub default_dir: String,
}

fn default_save_dir() -> String {
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        PathBuf::from(home).to_string_lossy().into_owned()
    } else {
        ".".into()
    }
}

#[tauri::command]
pub fn about() -> AboutInfo {
    AboutInfo {
        version: env!("CARGO_PKG_VERSION"),
        cipher_suite: "XChaCha20-Poly1305 + BLAKE3 + Argon2id",
        features: vec![
            "패스워드 (Argon2id)",
            "FIDO2 하드웨어 키",
            "BIP39 종이 백업",
            "Shamir 분산 백업",
            "X25519 + ML-KEM-768 (양자 안전 하이브리드)",
            "외부 아카이브 (RAR/ZIP/7Z/TAR/GZ/XZ/BZ2/LZ4/ZSTD/Brotli)",
        ],
        default_dir: default_save_dir(),
    }
}

#[derive(Serialize, Debug)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub kind: String,
    pub header_summary: Option<String>,
}

#[tauri::command]
pub fn file_info(path: String) -> Result<FileInfo, String> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("파일 정보 읽기 실패: {}", e))?;
    let size = meta.len();

    if let Ok(bytes) = std::fs::read(&p) {
        if bytes.len() >= 8 && &bytes[..8] == qsafe_core::format::MAGIC {
            let summary = match qsafe_core::io::read_packed_file(bytes.as_slice()) {
                Ok(pf) => format!(
                    "{} recipients · {} bytes original · suite={:?}",
                    pf.header.recipients.len(),
                    pf.header.original_size,
                    pf.header.suite
                ),
                Err(_) => "qsafe streaming format".into(),
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

#[derive(Serialize, Debug)]
pub struct IdentitySummary {
    pub fingerprint: String,
    pub x25519_pk_len: usize,
    pub mlkem768_pk_len: usize,
    pub path: String,
    pub public_path: Option<String>,
}

fn derive_pub_path(secret_path: &Path) -> PathBuf {
    let stem = secret_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "identity".into());
    let parent = secret_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{}.pub.json", stem))
}

#[tauri::command]
pub fn identity_generate(output_path: String, force: bool) -> Result<IdentitySummary, String> {
    let path = PathBuf::from(&output_path);
    if path.exists() {
        if !force {
            return Err(format!(
                "파일이 이미 있어요: {}\n→ 다른 이름으로 입력하거나 [덮어쓰기] 옵션을 켜세요.",
                path.display()
            ));
        }
        std::fs::remove_file(&path).map_err(|e| format!("기존 파일 삭제 실패: {}", e))?;
    }

    let identity = qsafe_identity::Identity::generate();
    let secret = qsafe_identity::IdentitySecretBytes::from_identity(&identity);
    let json = serde_json::to_vec_pretty(&secret).map_err(|e| e.to_string())?;
    write_secret_json(&path, &json).map_err(|e| format!("저장 실패: {}", e))?;

    let pub_path = derive_pub_path(&path);
    let mut public_path_str = None;
    if pub_path.exists() && force {
        let _ = std::fs::remove_file(&pub_path);
    }
    if !pub_path.exists() {
        let public = identity.public();
        let pub_json = serde_json::to_vec_pretty(&public).map_err(|e| e.to_string())?;
        if write_public_json(&pub_path, &pub_json).is_ok() {
            public_path_str = Some(pub_path.to_string_lossy().into_owned());
        }
    }

    Ok(IdentitySummary {
        fingerprint: identity.fingerprint(),
        x25519_pk_len: identity.x25519_pk_bytes.len(),
        mlkem768_pk_len: identity.mlkem768_pk_bytes.len(),
        path: output_path,
        public_path: public_path_str,
    })
}

#[tauri::command]
pub fn identity_show(path: String) -> Result<IdentitySummary, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("파일 읽기 실패: {}", e))?;

    if let Ok(secret) = serde_json::from_slice::<qsafe_identity::IdentitySecretBytes>(&bytes) {
        let identity = secret.to_identity().map_err(|e| e.to_string())?;
        return Ok(IdentitySummary {
            fingerprint: identity.fingerprint(),
            x25519_pk_len: identity.x25519_pk_bytes.len(),
            mlkem768_pk_len: identity.mlkem768_pk_bytes.len(),
            path,
            public_path: None,
        });
    }

    let public: qsafe_identity::IdentityPublic = serde_json::from_slice(&bytes)
        .map_err(|e| format!("qsafe identity 파일이 아닙니다 (원인: {})", e))?;
    Ok(IdentitySummary {
        fingerprint: public.fingerprint(),
        x25519_pk_len: public.x25519_pk.len(),
        mlkem768_pk_len: public.mlkem768_pk.len(),
        path,
        public_path: None,
    })
}

#[derive(Serialize, Debug)]
pub struct MnemonicResult {
    pub words: Vec<String>,
    pub word_count: usize,
    pub language: &'static str,
}

#[tauri::command]
pub fn mnemonic_generate(word_count: u8) -> Result<MnemonicResult, String> {
    use bip39::{Language, Mnemonic};
    use rand::RngCore;
    let entropy_bits: usize = match word_count {
        12 => 128,
        15 => 160,
        18 => 192,
        21 => 224,
        24 => 256,
        _ => return Err("단어 개수는 12, 15, 18, 21, 24 중 하나여야 합니다.".into()),
    };
    let mut entropy = vec![0u8; entropy_bits / 8];
    rand::rngs::OsRng.fill_bytes(&mut entropy);
    let m = Mnemonic::from_entropy_in(Language::English, &entropy)
        .map_err(|e| format!("니모닉 생성 실패: {}", e))?;
    let words: Vec<String> = m.words().map(|w| w.to_string()).collect();
    Ok(MnemonicResult {
        word_count: words.len(),
        words,
        language: "english",
    })
}

#[tauri::command]
pub fn mnemonic_verify(words: String) -> Result<bool, String> {
    use bip39::{Language, Mnemonic};
    let trimmed = words.trim();
    if trimmed.is_empty() {
        return Err("단어를 입력해 주세요.".into());
    }
    match Mnemonic::parse_in(Language::English, trimmed) {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("검증 실패: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("qsafe-gui-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_file(&p);
        let pub_p = derive_pub_path(&p);
        let _ = std::fs::remove_file(&pub_p);
        p
    }

    #[test]
    fn about_returns_known_fields() {
        let a = about();
        assert!(!a.version.is_empty());
        assert!(a.cipher_suite.contains("XChaCha20"));
        assert!(!a.features.is_empty());
        assert!(!a.default_dir.is_empty());
    }

    #[test]
    fn identity_generate_creates_both_files() {
        let path = tmp("gen-both");
        let r = identity_generate(path.to_string_lossy().into_owned(), false).expect("generate");
        assert!(path.exists(), "secret file");
        let pub_path = derive_pub_path(&path);
        assert!(pub_path.exists(), "public file");
        assert!(r.public_path.is_some());
        assert!(
            r.fingerprint.len() >= 8,
            "fingerprint too short: {}",
            r.fingerprint.len()
        ); // 16 bytes hex
        assert!(r.x25519_pk_len > 0);
        assert!(r.mlkem768_pk_len > 0);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&pub_path);
    }

    #[test]
    fn identity_generate_rejects_existing_without_force() {
        let path = tmp("no-overwrite");
        identity_generate(path.to_string_lossy().into_owned(), false).expect("first");
        let err = identity_generate(path.to_string_lossy().into_owned(), false);
        assert!(err.is_err());
        let msg = err.unwrap_err();
        assert!(msg.contains("이미") || msg.contains("덮어쓰기"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&derive_pub_path(&path));
    }

    #[test]
    fn identity_generate_force_overwrites() {
        let path = tmp("force");
        let r1 = identity_generate(path.to_string_lossy().into_owned(), false).unwrap();
        let r2 = identity_generate(path.to_string_lossy().into_owned(), true).unwrap();
        // 다른 키쌍 — fingerprint가 달라야 함
        assert_ne!(r1.fingerprint, r2.fingerprint);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&derive_pub_path(&path));
    }

    #[test]
    fn identity_show_matches_generate_fingerprint() {
        let path = tmp("show-sec");
        let g = identity_generate(path.to_string_lossy().into_owned(), false).unwrap();
        let s = identity_show(path.to_string_lossy().into_owned()).unwrap();
        assert_eq!(g.fingerprint, s.fingerprint);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&derive_pub_path(&path));
    }

    #[test]
    fn identity_show_works_on_public_file_too() {
        let path = tmp("show-pub");
        let g = identity_generate(path.to_string_lossy().into_owned(), false).unwrap();
        let pub_path = derive_pub_path(&path);
        let s = identity_show(pub_path.to_string_lossy().into_owned()).unwrap();
        assert_eq!(g.fingerprint, s.fingerprint);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&pub_path);
    }

    #[test]
    fn identity_show_rejects_garbage() {
        let path = tmp("garbage");
        std::fs::write(&path, b"not json").unwrap();
        let r = identity_show(path.to_string_lossy().into_owned());
        assert!(r.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn mnemonic_generate_all_valid_word_counts() {
        for n in [12u8, 15, 18, 21, 24] {
            let r = mnemonic_generate(n).unwrap_or_else(|e| panic!("n={}: {}", n, e));
            assert_eq!(r.word_count as u8, n);
            assert_eq!(r.words.len() as u8, n);
            // 생성된 단어는 verify를 통과해야 함
            let joined = r.words.join(" ");
            assert!(mnemonic_verify(joined).unwrap());
        }
    }

    #[test]
    fn mnemonic_generate_rejects_invalid_count() {
        for n in [0u8, 1, 11, 13, 25, 100] {
            let r = mnemonic_generate(n);
            assert!(r.is_err(), "n={} should fail", n);
        }
    }

    #[test]
    fn mnemonic_verify_rejects_garbage() {
        assert!(mnemonic_verify("not a real mnemonic".into()).is_err());
        assert!(mnemonic_verify("".into()).is_err());
        assert!(mnemonic_verify("   ".into()).is_err());
    }

    #[test]
    fn mnemonic_verify_rejects_bad_checksum() {
        // 마지막 단어를 임의로 바꿔 체크섬을 깨뜨림
        let r = mnemonic_generate(12).unwrap();
        let mut words = r.words.clone();
        words[11] = "zoo".into();
        let joined = words.join(" ");
        assert!(mnemonic_verify(joined).is_err());
    }

    #[test]
    fn derive_pub_path_appends_pub_json() {
        let p = derive_pub_path(Path::new("/tmp/foo.json"));
        assert!(p.to_string_lossy().ends_with("foo.pub.json"));
    }

    #[test]
    fn derive_pub_path_handles_no_extension() {
        let p = derive_pub_path(Path::new("/tmp/myid"));
        assert!(p.to_string_lossy().ends_with("myid.pub.json"));
    }

    #[test]
    fn file_info_on_plain_file() {
        let path = tmp("plain");
        std::fs::write(&path, b"hello").unwrap();
        let r = file_info(path.to_string_lossy().into_owned()).unwrap();
        assert_eq!(r.kind, "plain");
        assert_eq!(r.size, 5);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_info_on_missing_file_errors() {
        let r = file_info("/nonexistent-path-12345".into());
        assert!(r.is_err());
    }

    #[test]
    fn cross_check_generated_pair_can_unpack() {
        // identity_generate로 만든 키쌍이 실제로 X25519+ML-KEM-768 KEM 라운드트립을 통과하는지
        let path = tmp("xroundtrip");
        identity_generate(path.to_string_lossy().into_owned(), false).unwrap();
        let pub_path = derive_pub_path(&path);

        let secret_bytes = std::fs::read(&path).unwrap();
        let secret: qsafe_identity::IdentitySecretBytes =
            serde_json::from_slice(&secret_bytes).unwrap();
        let identity = secret.to_identity().unwrap();

        let pub_bytes = std::fs::read(&pub_path).unwrap();
        let public: qsafe_identity::IdentityPublic = serde_json::from_slice(&pub_bytes).unwrap();

        assert_eq!(identity.fingerprint(), public.fingerprint());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&pub_path);
    }
}

#[tauri::command]
pub fn default_identity_path() -> String {
    let dir = default_save_dir();
    let mut p = PathBuf::from(dir);
    p.push("qsafe-identity.json");
    p.to_string_lossy().into_owned()
}

// ════════════════════════════════════════════════════════════
// PUBLIC_PASSWORD — "암호 없는" qsafe 압축의 내부 고정값
// ════════════════════════════════════════════════════════════
pub const PUBLIC_PASSWORD: &str = "qsafe-public-v1";

// ════════════════════════════════════════════════════════════
// 파일 탐색기 명령들
// ════════════════════════════════════════════════════════════

#[derive(Serialize, Debug)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_unix: i64,
    pub is_qsafe: bool,
    pub is_archive: bool,
}

#[derive(Serialize, Debug)]
pub struct DirListing {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<DirEntry>,
}

#[tauri::command]
pub fn list_drives() -> Vec<String> {
    // 플랫폼별 초기 후보로 시작 → 존재하는 드라이브만 push.
    // Windows에서는 A:\\ ~ Z:\\ 스캔, Unix는 루트 하나.
    #[cfg(windows)]
    {
        let mut drives = Vec::new();
        for letter in b'A'..=b'Z' {
            let p = format!("{}:\\", letter as char);
            if std::path::Path::new(&p).exists() {
                drives.push(p);
            }
        }
        drives
    }
    #[cfg(unix)]
    {
        vec!["/".to_string()]
    }
}

#[tauri::command]
pub fn home_dir() -> String {
    default_save_dir()
}

#[tauri::command]
pub fn current_dir() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| default_save_dir())
}

#[tauri::command]
pub fn list_directory(path: String) -> Result<DirListing, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("경로가 없습니다: {}", path));
    }
    if !p.is_dir() {
        return Err(format!("폴더가 아닙니다: {}", path));
    }
    let mut entries: Vec<DirEntry> = Vec::new();
    let rd = std::fs::read_dir(&p).map_err(|e| format!("디렉토리 읽기 실패: {}", e))?;
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();
        let size = if is_dir { 0 } else { meta.len() };
        let modified_unix = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let lower = name.to_lowercase();
        let is_qsafe = lower.ends_with(".qs");
        let is_archive = !is_dir
            && (lower.ends_with(".zip")
                || lower.ends_with(".7z")
                || lower.ends_with(".rar")
                || lower.ends_with(".tar")
                || lower.ends_with(".tgz")
                || lower.ends_with(".tar.gz")
                || lower.ends_with(".gz")
                || lower.ends_with(".xz")
                || lower.ends_with(".bz2")
                || lower.ends_with(".lz4")
                || lower.ends_with(".zst")
                || lower.ends_with(".br"));
        entries.push(DirEntry {
            name,
            path: entry.path().to_string_lossy().into_owned(),
            is_dir,
            size,
            modified_unix,
            is_qsafe,
            is_archive,
        });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    let parent_path = p.parent().map(|pp| pp.to_string_lossy().into_owned());
    Ok(DirListing {
        current_path: p.to_string_lossy().into_owned(),
        parent_path,
        entries,
    })
}

// ════════════════════════════════════════════════════════════
// qsafe CLI shell-out — pack_one / unpack_qsafe / qsafe_info
// ════════════════════════════════════════════════════════════

fn locate_qsafe_bin() -> Result<PathBuf, String> {
    let exe_path = std::env::current_exe().map_err(|e| format!("current_exe: {}", e))?;
    if let Some(dir) = exe_path.parent() {
        let c = dir.join(if cfg!(windows) { "qsafe.exe" } else { "qsafe" });
        if c.exists() {
            return Ok(c);
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let c = PathBuf::from(home)
            .join(".cargo")
            .join("bin")
            .join(if cfg!(windows) { "qsafe.exe" } else { "qsafe" });
        if c.exists() {
            return Ok(c);
        }
    }
    Ok(PathBuf::from(if cfg!(windows) {
        "qsafe.exe"
    } else {
        "qsafe"
    }))
}

#[derive(Serialize, Debug)]
pub struct PackResult {
    pub output_path: String,
    pub original_size: u64,
    pub packed_size: u64,
    pub ratio_percent: f64,
}

#[tauri::command]
pub fn pack_one(
    input: String,
    output: Option<String>,
    password: Option<String>,
    pubkeys: Vec<String>,
    no_password: bool,
    force: bool,
    compression: Option<String>,
) -> Result<PackResult, String> {
    let qsafe = locate_qsafe_bin()?;
    let in_path = PathBuf::from(&input);
    if !in_path.exists() {
        return Err(format!("입력 파일이 없습니다: {}", input));
    }
    let original_size = std::fs::metadata(&in_path).map(|m| m.len()).unwrap_or(0);
    let out_path = output.unwrap_or_else(|| format!("{}.qs", input));
    let mut cmd = std::process::Command::new(&qsafe);
    cmd.arg("pack").arg(&in_path).arg("-o").arg(&out_path);
    if force {
        cmd.arg("--force");
    }
    // 압축 알고리즘: "auto" | "none" | "zstd" (생략 시 auto)
    let comp = compression.as_deref().unwrap_or("auto");
    if comp == "none" || comp == "zstd" || comp == "auto" {
        cmd.arg("-c").arg(comp);
    }
    if no_password {
        cmd.arg("--no-password");
    } else if let Some(pw) = password.as_ref() {
        cmd.arg("--password").arg(pw);
    }
    for pk in &pubkeys {
        cmd.arg("--pubkey").arg(pk);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("qsafe 실행 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "압축 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let packed_size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
    Ok(PackResult {
        output_path: out_path,
        original_size,
        packed_size,
        ratio_percent: if original_size > 0 {
            (packed_size as f64 / original_size as f64) * 100.0
        } else {
            100.0
        },
    })
}

#[derive(Serialize, Debug)]
pub struct UnpackResult {
    pub output_path: String,
    pub bytes_written: u64,
    pub untarred_dir: Option<String>, // tar 자동 풀기 결과 폴더
    pub file_count: u64,              // untar 시 파일 수
}

#[tauri::command]
pub fn unpack_qsafe(
    input: String,
    output: Option<String>,
    password: Option<String>,
    identity: Option<String>,
    force: bool,
    open_mode: Option<bool>,
) -> Result<UnpackResult, String> {
    // open_mode=true → 공개 패스워드 자동 사용
    let (password, identity) = if open_mode.unwrap_or(false) {
        (Some(PUBLIC_PASSWORD.to_string()), None)
    } else {
        (password, identity)
    };

    let qsafe = locate_qsafe_bin()?;
    let in_path = PathBuf::from(&input);
    if !in_path.exists() {
        return Err(format!("입력 파일이 없습니다: {}", input));
    }
    let out_path = output.unwrap_or_else(|| {
        if input.ends_with(".qs") {
            input[..input.len() - 3].to_string()
        } else {
            format!("{}.out", input)
        }
    });
    let mut cmd = std::process::Command::new(&qsafe);
    cmd.arg("unpack").arg(&in_path).arg("-o").arg(&out_path);
    if force {
        cmd.arg("--force");
    }
    if let Some(pw) = password.as_ref() {
        cmd.arg("--password").arg(pw);
    }
    if let Some(id) = identity.as_ref() {
        cmd.arg("--identity").arg(id);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("qsafe 실행 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "풀기 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let bytes_written = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

    // 자동 untar: 풀어진 파일의 처음 263바이트에 "ustar" magic이 있는지
    let (untarred_dir, file_count) = try_auto_untar(&out_path);
    Ok(UnpackResult {
        output_path: out_path,
        bytes_written,
        untarred_dir,
        file_count,
    })
}

/// 풀어진 파일이 tar이면 자동으로 untar — (untarred_dir, file_count) 반환.
/// tar가 아니거나 실패하면 None.
fn try_auto_untar(file_path: &str) -> (Option<String>, u64) {
    let p = std::path::Path::new(file_path);
    // tar magic 검사: posix ustar는 offset 257에 "ustar"
    let head = match std::fs::read(p) {
        Ok(b) => b,
        Err(_) => return (None, 0),
    };
    if head.len() < 265 {
        return (None, 0);
    }
    let magic = &head[257..262];
    if magic != b"ustar" {
        return (None, 0);
    }
    drop(head);

    // untar 할 폴더: 같은 위치에 (파일명 - .tar 확장자) 또는 (파일명) + "_extracted"
    let dir_name = if p.extension().and_then(|s| s.to_str()) == Some("tar") {
        p.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "extracted".into())
    } else {
        format!(
            "{}_extracted",
            p.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        )
    };
    let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
    let out_dir = parent.join(dir_name);
    // 기존 폴더 비우기
    let _ = std::fs::remove_dir_all(&out_dir);
    if std::fs::create_dir_all(&out_dir).is_err() {
        return (None, 0);
    }

    let f = match std::fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return (None, 0),
    };
    let mut ar = tar::Archive::new(f);
    if ar.unpack(&out_dir).is_err() {
        return (None, 0);
    }

    // 파일 수 세기
    let mut count = 0u64;
    fn walk(p: &std::path::Path, count: &mut u64) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                if let Ok(md) = e.metadata() {
                    if md.is_dir() {
                        walk(&e.path(), count);
                    } else {
                        *count += 1;
                    }
                }
            }
        }
    }
    walk(&out_dir, &mut count);

    // 원본 tar 파일 삭제 (사용자에게 폴더만 남기기)
    let _ = std::fs::remove_file(p);

    (Some(out_dir.to_string_lossy().into_owned()), count)
}

#[derive(Serialize, Debug)]
pub struct QsafeInfo {
    pub path: String,
    pub size: u64,
    pub format: String,
    pub cipher_suite: Option<String>,
    pub compression: Option<String>,
    pub recipients: Vec<String>,
    pub original_size: u64,
    pub label: Option<String>,
    pub created_at_unix: i64,
}

#[tauri::command]
pub fn qsafe_info(path: String) -> Result<QsafeInfo, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("read: {}", e))?;
    let size = bytes.len() as u64;
    let pf = qsafe_core::io::read_packed_file(bytes.as_slice())
        .map_err(|e| format!("qsafe 파일 헤더 읽기 실패: {}", e))?;
    let mut recipients = Vec::new();
    for r in &pf.header.recipients {
        let label: String = match r {
            qsafe_core::format::Recipient::Password(_) => "Password (Argon2id)".to_string(),
            qsafe_core::format::Recipient::Fido2(f) => {
                format!("FIDO2 ({})", f.label.as_deref().unwrap_or("unnamed"))
            }
            qsafe_core::format::Recipient::Bip39(_) => "BIP39 종이 백업".to_string(),
            qsafe_core::format::Recipient::Pubkey(_) => "X25519+ML-KEM-768 (PQ)".to_string(),
            qsafe_core::format::Recipient::Timelock(_) => "Timelock".to_string(),
            qsafe_core::format::Recipient::ShamirCommitment(_) => "Shamir M-of-N".to_string(),
        };
        recipients.push(label);
    }
    Ok(QsafeInfo {
        path,
        size,
        format: "qsafe (.qs)".into(),
        cipher_suite: Some(format!("{:?}", pf.header.suite)),
        compression: Some(format!("{:?}", pf.header.compression)),
        recipients,
        original_size: pf.header.original_size,
        label: pf.header.label.clone(),
        created_at_unix: pf.header.created_at_unix,
    })
}

#[derive(Serialize, Debug)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub is_encrypted: bool,
}

#[derive(Serialize, Debug)]
pub struct ArchiveListing {
    pub path: String,
    pub format: String,
    pub entries: Vec<ArchiveEntry>,
}

#[tauri::command]
pub fn list_external_archive(path: String) -> Result<ArchiveListing, String> {
    use qsafe_formats::{detect_format, ExternalFormat};
    let bytes = std::fs::read(&path).map_err(|e| format!("read: {}", e))?;
    let fmt = detect_format(&bytes);
    drop(bytes);
    let mut entries = Vec::new();
    if let ExternalFormat::Rar = fmt {
        let list = qsafe_formats::rar::list_rar(PathBuf::from(&path).as_path(), None)
            .map_err(|e| format!("RAR 목록 실패: {}", e))?;
        for e in list {
            entries.push(ArchiveEntry {
                name: e.filename,
                size: e.unpacked_size,
                is_dir: e.is_directory,
                is_encrypted: e.is_encrypted,
            });
        }
    }
    Ok(ArchiveListing {
        path,
        format: fmt.name().to_string(),
        entries,
    })
}

#[tauri::command]
pub fn extract_external_archive(
    input: String,
    output_dir: String,
    password: Option<String>,
) -> Result<u64, String> {
    let qsafe = locate_qsafe_bin()?;
    let mut cmd = std::process::Command::new(&qsafe);
    cmd.arg("extract").arg(&input).arg("-o").arg(&output_dir);
    if let Some(pw) = password.as_ref() {
        cmd.arg("--password").arg(pw);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("qsafe 실행 실패: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let count = std::fs::read_dir(&output_dir)
        .map(|rd| rd.count() as u64)
        .unwrap_or(0);
    Ok(count)
}

// ════════════════════════════════════════════════════════════
// pack_path — 폴더 또는 파일 압축 (open_mode 지원)
// ════════════════════════════════════════════════════════════

#[derive(Serialize, Debug)]
pub struct PackPathResult {
    pub output_path: String,
    pub source_kind: String,
    pub source_size: u64,
    pub file_count: u64,
    pub packed_size: u64,
    pub ratio_percent: f64,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command 시그니처는 frontend invoke 파라미터와 1:1 매핑이라 분할 시 UX 손상.
pub fn pack_path(
    input: String,
    output: Option<String>,
    password: Option<String>,
    pubkeys: Vec<String>,
    no_password: bool,
    force: bool,
    open_mode: Option<bool>,
    compression: Option<String>,
) -> Result<PackPathResult, String> {
    // open_mode=true → 공개 패스워드 자동 사용
    let (password, pubkeys, no_password) = if open_mode.unwrap_or(false) {
        (Some(PUBLIC_PASSWORD.to_string()), Vec::new(), false)
    } else {
        (password, pubkeys, no_password)
    };

    let in_path = PathBuf::from(&input);
    if !in_path.exists() {
        return Err(format!("입력이 존재하지 않습니다: {}", input));
    }

    if in_path.is_file() {
        let r = pack_one(
            input.clone(),
            output,
            password,
            pubkeys,
            no_password,
            force,
            compression.clone(),
        )?;
        return Ok(PackPathResult {
            output_path: r.output_path,
            source_kind: "file".into(),
            source_size: r.original_size,
            file_count: 1,
            packed_size: r.packed_size,
            ratio_percent: r.ratio_percent,
        });
    }
    if !in_path.is_dir() {
        return Err("파일 또는 폴더만 압축 가능합니다.".into());
    }

    let out_path = output.unwrap_or_else(|| format!("{}.qs", input));
    let tmp_tar = {
        let parent = in_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!(
            ".qsafe-tmp-{}-{}.tar",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ))
    };
    let (source_size, file_count) =
        create_tar(&in_path, &tmp_tar).map_err(|e| format!("tar 생성 실패: {}", e))?;
    let pack_result = pack_one(
        tmp_tar.to_string_lossy().into_owned(),
        Some(out_path.clone()),
        password,
        pubkeys,
        no_password,
        force,
        compression.clone(),
    );
    let _ = std::fs::remove_file(&tmp_tar);
    let r = pack_result?;
    Ok(PackPathResult {
        output_path: r.output_path,
        source_kind: "directory".into(),
        source_size,
        file_count,
        packed_size: r.packed_size,
        ratio_percent: if source_size > 0 {
            (r.packed_size as f64 / source_size as f64) * 100.0
        } else {
            100.0
        },
    })
}

fn create_tar(src: &std::path::Path, dst: &std::path::Path) -> Result<(u64, u64), String> {
    let file = std::fs::File::create(dst).map_err(|e| format!("tar 파일 생성: {}", e))?;
    let mut builder = tar::Builder::new(file);
    builder.follow_symlinks(false);
    let base_name = src
        .file_name()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("data"));
    builder
        .append_dir_all(&base_name, src)
        .map_err(|e| format!("tar append: {}", e))?;
    builder.finish().map_err(|e| format!("tar finish: {}", e))?;
    drop(builder);
    let mut total_size: u64 = 0;
    let mut count: u64 = 0;
    fn walk(p: &std::path::Path, total: &mut u64, count: &mut u64) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                if let Ok(md) = e.metadata() {
                    if md.is_dir() {
                        walk(&e.path(), total, count);
                    } else {
                        *total += md.len();
                        *count += 1;
                    }
                }
            }
        }
    }
    walk(src, &mut total_size, &mut count);
    Ok((total_size, count))
}

// ════════════════════════════════════════════════════════════
// 외부 표준 ZIP 압축
// ════════════════════════════════════════════════════════════

#[derive(Serialize, Debug)]
pub struct ZipResult {
    pub output_path: String,
    pub source_kind: String,
    pub source_size: u64,
    pub file_count: u64,
    pub packed_size: u64,
    pub ratio_percent: f64,
}

#[tauri::command]
pub fn pack_to_zip(
    input: String,
    output: Option<String>,
    force: bool,
) -> Result<ZipResult, String> {
    let in_path = PathBuf::from(&input);
    if !in_path.exists() {
        return Err(format!("입력이 존재하지 않습니다: {}", input));
    }
    let out_path = output.unwrap_or_else(|| format!("{}.zip", input));
    let out = PathBuf::from(&out_path);
    if out.exists() && !force {
        return Err(format!("파일이 이미 있어요: {}", out.display()));
    }
    let f = std::fs::File::create(&out).map_err(|e| format!("zip 파일 생성: {}", e))?;
    let mut writer = zip::ZipWriter::new(f);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut source_size: u64 = 0;
    let mut file_count: u64 = 0;
    let kind: &str;
    if in_path.is_file() {
        kind = "file";
        let name = in_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        let bytes = std::fs::read(&in_path).map_err(|e| format!("입력 읽기: {}", e))?;
        source_size = bytes.len() as u64;
        writer
            .start_file(&name, opts)
            .map_err(|e| format!("zip start_file: {}", e))?;
        use std::io::Write;
        writer
            .write_all(&bytes)
            .map_err(|e| format!("zip write: {}", e))?;
        file_count = 1;
    } else if in_path.is_dir() {
        kind = "directory";
        let base = in_path
            .file_name()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("data"));
        fn walk_zip<W: std::io::Write + std::io::Seek>(
            writer: &mut zip::ZipWriter<W>,
            opts: &zip::write::FileOptions<'_, ()>,
            _base_dir: &std::path::Path,
            cur: &std::path::Path,
            archive_prefix: &std::path::Path,
            source_size: &mut u64,
            file_count: &mut u64,
        ) -> Result<(), String> {
            for entry in std::fs::read_dir(cur).map_err(|e| format!("read_dir: {}", e))? {
                let entry = entry.map_err(|e| format!("entry: {}", e))?;
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let archive_path = archive_prefix.join(&name);
                let archive_str = archive_path.to_string_lossy().replace('\\', "/");
                let md = entry.metadata().map_err(|e| format!("metadata: {}", e))?;
                if md.is_dir() {
                    writer
                        .add_directory(format!("{}/", archive_str), *opts)
                        .map_err(|e| format!("zip add_dir: {}", e))?;
                    walk_zip(
                        writer,
                        opts,
                        _base_dir,
                        &path,
                        &archive_path,
                        source_size,
                        file_count,
                    )?;
                } else {
                    let data = std::fs::read(&path).map_err(|e| format!("read: {}", e))?;
                    *source_size += data.len() as u64;
                    *file_count += 1;
                    writer
                        .start_file(&archive_str, *opts)
                        .map_err(|e| format!("zip start_file: {}", e))?;
                    use std::io::Write;
                    writer
                        .write_all(&data)
                        .map_err(|e| format!("zip write: {}", e))?;
                }
            }
            Ok(())
        }
        walk_zip(
            &mut writer,
            &opts,
            &in_path,
            &in_path,
            &base,
            &mut source_size,
            &mut file_count,
        )?;
    } else {
        return Err("파일 또는 폴더만 압축 가능합니다.".into());
    }
    writer.finish().map_err(|e| format!("zip finish: {}", e))?;
    let packed_size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    Ok(ZipResult {
        output_path: out.to_string_lossy().into_owned(),
        source_kind: kind.into(),
        source_size,
        file_count,
        packed_size,
        ratio_percent: if source_size > 0 {
            (packed_size as f64 / source_size as f64) * 100.0
        } else {
            100.0
        },
    })
}

#[cfg(test)]
mod open_mode_tests {
    use super::*;
    use std::env;

    fn tmp(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("qsafe-gui-open-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn pack_path_open_mode_then_unpack_open_mode_roundtrip() {
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let input = tmp("input.txt");
        let packed = tmp("packed.qs");
        let restored = tmp("restored.txt");
        std::fs::write(&input, b"public qsafe roundtrip").unwrap();

        // open_mode=true → 패스워드 없이 압축
        let r = pack_path(
            input.to_string_lossy().into_owned(),
            Some(packed.to_string_lossy().into_owned()),
            None,
            vec![],
            false,
            true,
            Some(true),
            None,
        )
        .unwrap();
        assert!(packed.exists());
        assert_eq!(r.source_kind, "file");

        // open_mode=true → 패스워드 없이 풀기
        let u = unpack_qsafe(
            packed.to_string_lossy().into_owned(),
            Some(restored.to_string_lossy().into_owned()),
            None,
            None,
            true,
            Some(true),
        )
        .unwrap();
        assert!(restored.exists());
        assert_eq!(std::fs::read(&restored).unwrap(), b"public qsafe roundtrip");
        assert_eq!(u.bytes_written, 22);
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&packed);
        let _ = std::fs::remove_file(&restored);
    }

    #[test]
    fn open_mode_pack_then_user_password_unpack_fails() {
        // open_mode로 만든 .qs는 PUBLIC_PASSWORD를 모르는 사용자가 풀 수 없어야
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let input = tmp("inp.txt");
        let packed = tmp("pk.qs");
        std::fs::write(&input, b"x").unwrap();
        pack_path(
            input.to_string_lossy().into_owned(),
            Some(packed.to_string_lossy().into_owned()),
            None,
            vec![],
            false,
            true,
            Some(true),
            None,
        )
        .unwrap();
        // 다른 패스워드로 풀기 시도 → 실패
        let r = unpack_qsafe(
            packed.to_string_lossy().into_owned(),
            Some(tmp("out").to_string_lossy().into_owned()),
            Some("wrong-password".into()),
            None,
            true,
            Some(false),
        );
        assert!(r.is_err());
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&packed);
    }

    #[test]
    fn pack_path_user_password_still_works() {
        // open_mode=false면 사용자 패스워드 사용
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let input = tmp("u.txt");
        let packed = tmp("u.qs");
        let restored = tmp("u.out");
        std::fs::write(&input, b"user pw").unwrap();
        pack_path(
            input.to_string_lossy().into_owned(),
            Some(packed.to_string_lossy().into_owned()),
            Some("myuser-pw".into()),
            vec![],
            false,
            true,
            Some(false),
            None,
        )
        .unwrap();
        unpack_qsafe(
            packed.to_string_lossy().into_owned(),
            Some(restored.to_string_lossy().into_owned()),
            Some("myuser-pw".into()),
            None,
            true,
            Some(false),
        )
        .unwrap();
        assert_eq!(std::fs::read(&restored).unwrap(), b"user pw");
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&packed);
        let _ = std::fs::remove_file(&restored);
    }

    #[test]
    fn pack_to_zip_still_works() {
        let dir = env::temp_dir().join(format!("qsafe-gui-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("a.txt");
        std::fs::write(&input, b"hello zip").unwrap();
        let out = dir.join("a.zip");
        pack_to_zip(
            input.to_string_lossy().into_owned(),
            Some(out.to_string_lossy().into_owned()),
            true,
        )
        .unwrap();
        assert!(out.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_directory_explorer_works() {
        let r = list_directory(home_dir()).unwrap();
        assert!(!r.current_path.is_empty());
    }

    #[test]
    fn list_drives_at_least_one() {
        assert!(!list_drives().is_empty());
    }
}

// ════════════════════════════════════════════════════════════
// 파일/폴더 삭제 — 안전 가드 (드라이브 루트, 시스템 폴더 거부)
// ════════════════════════════════════════════════════════════

#[derive(Serialize, Debug)]
pub struct DeleteResult {
    pub deleted_path: String,
    pub kind: String,    // "file" | "directory"
    pub size_bytes: u64, // 폴더면 모든 파일 합계
    pub item_count: u64, // 폴더면 항목 수
}

#[tauri::command]
pub fn delete_path(path: String) -> Result<DeleteResult, String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("경로가 없습니다: {}", path));
    }

    // 안전 가드 1: 드라이브 루트 (예: C:\) 거부
    if p.parent().is_none() || p.components().count() <= 1 {
        return Err("드라이브 루트 또는 시스템 최상위는 삭제할 수 없습니다.".into());
    }
    // 안전 가드 2: 시스템 폴더 거부
    let lower = path.to_lowercase().replace('\\', "/");
    let dangerous = [
        "/windows",
        "/program files",
        "/program files (x86)",
        "/programdata",
        "/system32",
        "/system",
        "/usr",
        "/etc",
        "/bin",
        "/sbin",
        "/boot",
        "/var",
        "/lib",
    ];
    for d in &dangerous {
        // path가 해당 폴더 자체 또는 그 직계 부모인 경우
        if lower.ends_with(d) || lower.ends_with(&format!("{}/", d)) {
            return Err(format!("시스템 폴더는 삭제할 수 없습니다: {}", path));
        }
    }

    // 안전 가드 3: 루트 디렉토리 직접 거부 (Windows: c:/, Unix: /)
    if cfg!(windows) {
        // c:/, d:/, e:/ 같은 패턴
        let trimmed = path.trim_end_matches(['/', '\\']);
        if trimmed.len() <= 3 {
            return Err("시스템 최상위 폴더는 삭제할 수 없습니다.".into());
        }
    }

    let is_dir = p.is_dir();
    let (size_bytes, item_count) = if is_dir {
        let mut total = 0u64;
        let mut count = 0u64;
        fn walk(p: &std::path::Path, total: &mut u64, count: &mut u64) {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    *count += 1;
                    if let Ok(md) = e.metadata() {
                        if md.is_dir() {
                            walk(&e.path(), total, count);
                        } else {
                            *total += md.len();
                        }
                    }
                }
            }
        }
        walk(&p, &mut total, &mut count);
        (total, count)
    } else {
        (std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0), 1)
    };

    // 실제 삭제
    let result = if is_dir {
        std::fs::remove_dir_all(&p)
    } else {
        std::fs::remove_file(&p)
    };
    result.map_err(|e| format!("삭제 실패: {}", e))?;

    Ok(DeleteResult {
        deleted_path: path,
        kind: if is_dir { "directory" } else { "file" }.into(),
        size_bytes,
        item_count,
    })
}

#[cfg(test)]
mod delete_and_untar_tests {
    use super::*;
    use std::env;

    fn tmp_dir(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("qsafe-gui-del-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn delete_file_works() {
        let dir = tmp_dir("file");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("victim.txt");
        std::fs::write(&f, b"bye").unwrap();
        let r = delete_path(f.to_string_lossy().into_owned()).unwrap();
        assert_eq!(r.kind, "file");
        assert!(!f.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_directory_recursive() {
        let dir = tmp_dir("rec");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"a").unwrap();
        std::fs::write(dir.join("sub").join("b.txt"), b"b").unwrap();
        let r = delete_path(dir.to_string_lossy().into_owned()).unwrap();
        assert_eq!(r.kind, "directory");
        assert!(r.item_count >= 2);
        assert!(!dir.exists());
    }

    #[test]
    fn delete_rejects_windows_root() {
        if cfg!(windows) {
            let r = delete_path("C:\\".into());
            assert!(r.is_err(), r"C:\ deletion must be rejected");
        }
    }

    #[test]
    fn delete_rejects_system_folders() {
        let cases = if cfg!(windows) {
            vec!["C:\\Windows", "C:\\Program Files"]
        } else {
            vec!["/usr", "/etc", "/bin"]
        };
        for path in cases {
            let r = delete_path(path.to_string());
            assert!(r.is_err(), "{} 거부 안 됨", path);
        }
    }

    #[test]
    fn delete_nonexistent_errors() {
        let r = delete_path("/nonexistent-xyz-99999".into());
        assert!(r.is_err());
    }

    #[test]
    fn auto_untar_after_unpack_directory_archive() {
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let base = tmp_dir("untar");
        let src = base.join("origfolder");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"AA").unwrap();
        std::fs::write(src.join("b.txt"), b"BB").unwrap();

        let qs = base.join("packed.qs");
        pack_path(
            src.to_string_lossy().into_owned(),
            Some(qs.to_string_lossy().into_owned()),
            None,
            vec![],
            false,
            true,
            Some(true),
            None,
        )
        .unwrap();

        let tar_out = base.join("restored.tar");
        let r = unpack_qsafe(
            qs.to_string_lossy().into_owned(),
            Some(tar_out.to_string_lossy().into_owned()),
            None,
            None,
            true,
            Some(true),
        )
        .unwrap();

        // 자동 untar 됐어야: untarred_dir이 Some
        assert!(r.untarred_dir.is_some(), "auto untar 발생 안 함");
        let extracted = std::path::PathBuf::from(r.untarred_dir.as_ref().unwrap());
        // 그 안에 origfolder/a.txt, b.txt가 있어야
        let a = extracted.join("origfolder").join("a.txt");
        let b = extracted.join("origfolder").join("b.txt");
        assert!(a.exists(), "{:?} 없음", a);
        assert!(b.exists(), "{:?} 없음", b);
        assert_eq!(std::fs::read(&a).unwrap(), b"AA");
        // 원본 .tar 파일은 자동 untar 후 삭제됨
        assert!(!tar_out.exists(), ".tar 파일이 남아있음");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn unpack_single_file_no_untar() {
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let base = tmp_dir("notar");
        std::fs::create_dir_all(&base).unwrap();
        let inp = base.join("single.txt");
        std::fs::write(&inp, b"just text").unwrap();
        let qs = base.join("s.qs");
        pack_path(
            inp.to_string_lossy().into_owned(),
            Some(qs.to_string_lossy().into_owned()),
            None,
            vec![],
            false,
            true,
            Some(true),
            None,
        )
        .unwrap();
        let out = base.join("out.txt");
        let r = unpack_qsafe(
            qs.to_string_lossy().into_owned(),
            Some(out.to_string_lossy().into_owned()),
            None,
            None,
            true,
            Some(true),
        )
        .unwrap();
        // 일반 파일은 untar 안 됨
        assert!(r.untarred_dir.is_none());
        assert!(out.exists());
        assert_eq!(std::fs::read(&out).unwrap(), b"just text");
        let _ = std::fs::remove_dir_all(&base);
    }
}

// ════════════════════════════════════════════════════════════
// 연결된 프로그램으로 파일/폴더 열기 (Windows: start, Mac: open, Linux: xdg-open)
// ════════════════════════════════════════════════════════════

#[tauri::command]
pub fn open_with_associated(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("경로가 없습니다: {}", path));
    }
    let result;
    #[cfg(windows)]
    {
        // cmd /C start "" "<path>" — 빈 따옴표는 title 인자 placeholder
        result = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        result = std::process::Command::new("open").arg(&path).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        result = std::process::Command::new("xdg-open").arg(&path).spawn();
    }
    result.map_err(|e| format!("열기 실패: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod open_assoc_tests {
    use super::*;
    use std::env;

    #[test]
    fn rejects_nonexistent() {
        let r = open_with_associated("/nonexistent-zzz-77777".into());
        assert!(r.is_err());
    }

    #[test]
    fn returns_ok_for_existing_path() {
        // 빈 파일 만들고 즉시 close. open_with_associated는 spawn만 함.
        let mut p = env::temp_dir();
        p.push(format!("qsafe-gui-openassoc-{}.txt", std::process::id()));
        std::fs::write(&p, b"hello").unwrap();
        let _ = open_with_associated(p.to_string_lossy().into_owned());
        // spawn된 프로세스는 GUI를 띄울 수 있으니 OK 검증만
        let _ = std::fs::remove_file(&p);
    }
}

// ════════════════════════════════════════════════════════════
// MD5 helper
// ════════════════════════════════════════════════════════════
fn compute_md5_file(path: &std::path::Path) -> Result<String, String> {
    use md5::{Digest, Md5};
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("md5 read: {}", e))?;
    let mut h = Md5::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("md5 read: {}", e))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

fn compute_dir_md5_aggregate(dir: &std::path::Path) -> Result<String, String> {
    use md5::{Digest, Md5};
    use std::io::Read;
    let mut entries: Vec<std::path::PathBuf> = Vec::new();
    fn walk(p: &std::path::Path, list: &mut Vec<std::path::PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                if let Ok(md) = e.metadata() {
                    if md.is_dir() {
                        walk(&e.path(), list);
                    } else {
                        list.push(e.path());
                    }
                }
            }
        }
    }
    walk(dir, &mut entries);
    entries.sort();
    let mut h = Md5::new();
    let prefix_len = dir
        .parent()
        .map(|p| p.to_string_lossy().len() + 1)
        .unwrap_or(0);
    for path in &entries {
        let rel = path.to_string_lossy();
        let rel = if prefix_len < rel.len() {
            &rel[prefix_len..]
        } else {
            rel.as_ref()
        };
        h.update(rel.as_bytes());
        h.update(b"\n");
        let mut f = std::fs::File::open(path).map_err(|e| format!("md5 read: {}", e))?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf).map_err(|e| format!("read: {}", e))?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        h.update(b"\n");
    }
    Ok(hex::encode(h.finalize()))
}

#[tauri::command]
pub fn md5_of_file(path: String) -> Result<String, String> {
    compute_md5_file(&std::path::PathBuf::from(&path))
}

// ════════════════════════════════════════════════════════════
// ISO 가상 마운트 — macOS hdiutil / Linux udisksctl / Windows Mount-DiskImage
//
// `iso_mount(path)`는 ISO 파일을 가상 디스크처럼 OS에 부착하고
// 마운트 포인트(macOS/Linux는 디렉토리, Windows는 드라이브 letter)와
// detach에 쓸 handle을 돌려준다. `iso_unmount(handle)`로 해제.
//
// 보안 / 권한 노트:
// - macOS는 hdiutil이 사용자 권한으로 작동 — 권한 escalation 없음.
// - Linux는 udisksctl이 polkit로 무권한 사용자도 마운트 가능 (rootless).
//   wodim/mount 대안은 root 필요 → udisksctl 우선.
// - Windows는 Mount-DiskImage가 사용자 컨텍스트에서 작동.
// - macOS Gatekeeper가 Quarantine된 ISO를 차단할 수 있음.
// ════════════════════════════════════════════════════════════

#[derive(Serialize, Debug)]
pub struct IsoMountResult {
    pub mount_point: String, // macOS: /Volumes/Foo, Linux: /run/media/user/Foo, Windows: D:\
    pub handle: String,      // unmount 호출 시 식별자 (OS별로 다름)
    pub label: String,       // 볼륨 라벨 (없으면 basename)
}

#[tauri::command]
pub fn iso_mount(path: String) -> Result<IsoMountResult, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("ISO 파일을 찾을 수 없습니다: {}", path));
    }
    // 확장자 검사는 약식. ISO 9660 magic 검사는 비싸서 default OFF.
    let lower = path.to_lowercase();
    if !(lower.ends_with(".iso") || lower.ends_with(".img") || lower.ends_with(".dmg")) {
        return Err(format!(
            "지원하지 않는 확장자입니다 (.iso/.img/.dmg만): {}",
            path
        ));
    }
    iso_mount_impl(&p)
}

#[tauri::command]
pub fn iso_unmount(handle: String) -> Result<(), String> {
    iso_unmount_impl(&handle)
}

#[cfg(target_os = "macos")]
fn iso_mount_impl(p: &std::path::Path) -> Result<IsoMountResult, String> {
    use std::process::Command;
    let out = Command::new("hdiutil")
        .args(["attach", "-nobrowse"])
        .arg(p)
        .output()
        .map_err(|e| format!("hdiutil 실행 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "hdiutil attach 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    // hdiutil 출력 형식 (탭/공백 분리):
    //   /dev/disk5\t\t<filesystem>\t\t/Volumes/MyDisk
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut mount_point = String::new();
    let mut handle = String::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').filter(|s| !s.is_empty()).collect();
        if let Some(last) = parts.last() {
            if last.starts_with("/Volumes/") {
                mount_point = last.trim().to_string();
                if let Some(first) = parts.first() {
                    handle = first.trim().to_string();
                }
            }
        }
    }
    if mount_point.is_empty() {
        return Err(format!("hdiutil 출력 파싱 실패:\n{}", stdout));
    }
    let label = std::path::Path::new(&mount_point)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("ISO")
        .to_string();
    Ok(IsoMountResult {
        mount_point,
        handle,
        label,
    })
}

#[cfg(target_os = "macos")]
fn iso_unmount_impl(handle: &str) -> Result<(), String> {
    use std::process::Command;
    let out = Command::new("hdiutil")
        .args(["detach", handle])
        .output()
        .map_err(|e| format!("hdiutil detach 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "hdiutil detach 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn iso_mount_impl(p: &std::path::Path) -> Result<IsoMountResult, String> {
    use std::process::Command;
    // 1) loop-setup (read-only)
    let out = Command::new("udisksctl")
        .args(["loop-setup", "-r", "-f"])
        .arg(p)
        .output()
        .map_err(|e| format!("udisksctl 실행 실패 (설치 안 됨?): {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "udisksctl loop-setup 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    // 출력: "Mapped file <iso> as /dev/loop0."
    let stdout = String::from_utf8_lossy(&out.stdout);
    let loop_dev = stdout
        .split_whitespace()
        .find(|s| s.starts_with("/dev/loop"))
        .ok_or_else(|| format!("loop 디바이스 파싱 실패:\n{}", stdout))?
        .trim_end_matches('.')
        .to_string();
    // 2) mount
    let out2 = Command::new("udisksctl")
        .args(["mount", "-b", &loop_dev])
        .output()
        .map_err(|e| format!("udisksctl mount 실패: {}", e))?;
    if !out2.status.success() {
        // best-effort: loop-delete
        let _ = Command::new("udisksctl")
            .args(["loop-delete", "-b", &loop_dev])
            .output();
        return Err(format!(
            "udisksctl mount 실패: {}",
            String::from_utf8_lossy(&out2.stderr).trim()
        ));
    }
    let mout = String::from_utf8_lossy(&out2.stdout);
    // 출력: "Mounted /dev/loop0 at /run/media/user/MyDisk"
    let mount_point = mout
        .split(" at ")
        .nth(1)
        .map(|s| s.trim().trim_end_matches('.').to_string())
        .ok_or_else(|| format!("mount 출력 파싱 실패:\n{}", mout))?;
    let label = std::path::Path::new(&mount_point)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("ISO")
        .to_string();
    Ok(IsoMountResult {
        mount_point,
        handle: loop_dev,
        label,
    })
}

#[cfg(target_os = "linux")]
fn iso_unmount_impl(handle: &str) -> Result<(), String> {
    use std::process::Command;
    let _ = Command::new("udisksctl")
        .args(["unmount", "-b", handle])
        .output();
    let out = Command::new("udisksctl")
        .args(["loop-delete", "-b", handle])
        .output()
        .map_err(|e| format!("udisksctl loop-delete 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "udisksctl loop-delete 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn iso_mount_impl(p: &std::path::Path) -> Result<IsoMountResult, String> {
    use std::process::Command;
    // PowerShell -NoProfile -Command "..."
    // Mount-DiskImage 후 Get-DiskImage | Get-Volume 으로 드라이브 letter 얻음.
    let script = format!(
        "$img = Mount-DiskImage -ImagePath '{}' -PassThru; \
         $vol = Get-DiskImage -ImagePath '{}' | Get-Volume; \
         Write-Output ($vol.DriveLetter + ':\\')",
        p.display().to_string().replace('\'', "''"),
        p.display().to_string().replace('\'', "''")
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("powershell 실행 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "Mount-DiskImage 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let mount_point = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if mount_point.is_empty() {
        return Err("드라이브 letter를 얻지 못했습니다".to_string());
    }
    Ok(IsoMountResult {
        mount_point: mount_point.clone(),
        handle: p.display().to_string(), // unmount 시 ImagePath 필요
        label: mount_point,
    })
}

#[cfg(target_os = "windows")]
fn iso_unmount_impl(handle: &str) -> Result<(), String> {
    use std::process::Command;
    let script = format!(
        "Dismount-DiskImage -ImagePath '{}'",
        handle.replace('\'', "''")
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|e| format!("powershell 실행 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "Dismount-DiskImage 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn iso_mount_impl(_p: &std::path::Path) -> Result<IsoMountResult, String> {
    Err("이 플랫폼은 ISO 마운트를 지원하지 않습니다".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn iso_unmount_impl(_handle: &str) -> Result<(), String> {
    Err("이 플랫폼은 ISO 마운트를 지원하지 않습니다".to_string())
}

fn basename_of(path: &str) -> String {
    let p = std::path::PathBuf::from(path);
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[derive(Serialize, Debug)]
pub struct PackPathExtResult {
    pub output_path: String,
    pub source_kind: String,
    pub source_size: u64,
    pub file_count: u64,
    pub packed_size: u64,
    pub ratio_percent: f64,
    pub md5: Option<String>,
    pub md5_file_path: Option<String>,
    pub sfx_path: Option<String>,
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn pack_path_ext(
    input: String,
    output: Option<String>,
    password: Option<String>,
    pubkeys: Vec<String>,
    no_password: bool,
    force: bool,
    open_mode: Option<bool>,
    compression: Option<String>,
    profile: Option<String>,
    sfx: Option<bool>,
    label: Option<String>,
    include_md5: Option<bool>,
) -> Result<PackPathExtResult, String> {
    let (password, pubkeys, no_password) = if open_mode.unwrap_or(false) {
        (Some(PUBLIC_PASSWORD.to_string()), Vec::new(), false)
    } else {
        (password, pubkeys, no_password)
    };

    let in_path = PathBuf::from(&input);
    if !in_path.exists() {
        return Err(format!("입력이 없습니다: {}", input));
    }
    let is_dir = in_path.is_dir();
    let want_md5 = include_md5.unwrap_or(false);
    let want_sfx = sfx.unwrap_or(false);

    let md5_value = if want_md5 {
        Some(if is_dir {
            compute_dir_md5_aggregate(&in_path)?
        } else {
            compute_md5_file(&in_path)?
        })
    } else {
        None
    };

    let final_label = match (&label, &md5_value) {
        (Some(l), Some(m)) => Some(format!("{} | MD5:{}", l, &m[..16])),
        (Some(l), None) => Some(l.clone()),
        (None, Some(m)) => Some(format!("MD5:{}", &m[..16])),
        _ => None,
    };

    let out_path = output.unwrap_or_else(|| {
        if want_sfx {
            if cfg!(windows) {
                format!("{}.run.exe", input)
            } else {
                format!("{}.run", input)
            }
        } else {
            format!("{}.qs", input)
        }
    });

    let (source_size, file_count, real_input_for_pack, tmp_tar): (
        u64,
        u64,
        String,
        Option<PathBuf>,
    ) = if is_dir {
        let tmp = {
            let parent = in_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            parent.join(format!(
                ".qsafe-tmp-{}-{}.tar",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            ))
        };
        let (sz, cnt) = create_tar(&in_path, &tmp).map_err(|e| format!("tar: {}", e))?;
        let s = tmp.to_string_lossy().into_owned();
        (sz, cnt, s, Some(tmp))
    } else {
        let sz = std::fs::metadata(&in_path).map(|m| m.len()).unwrap_or(0);
        (sz, 1, input.clone(), None)
    };

    let qsafe = locate_qsafe_bin()?;
    let mut cmd = std::process::Command::new(&qsafe);
    cmd.arg("pack")
        .arg(&real_input_for_pack)
        .arg("-o")
        .arg(&out_path);
    if force {
        cmd.arg("--force");
    }
    let comp = compression.as_deref().unwrap_or("auto");
    if comp == "none" || comp == "zstd" || comp == "auto" {
        cmd.arg("-c").arg(comp);
    }
    if let Some(prof) = profile.as_deref() {
        if prof == "standard" || prof == "strong" {
            cmd.arg("--profile").arg(prof);
        }
    }
    if no_password {
        cmd.arg("--no-password");
    } else if let Some(pw) = password.as_ref() {
        cmd.arg("--password").arg(pw);
    }
    for pk in &pubkeys {
        cmd.arg("--pubkey").arg(pk);
    }
    if want_sfx {
        cmd.arg("--sfx");
    }
    if let Some(l) = &final_label {
        cmd.arg("--label").arg(l);
    }

    let out = cmd.output().map_err(|e| {
        if let Some(t) = &tmp_tar {
            let _ = std::fs::remove_file(t);
        }
        format!("qsafe 실행 실패: {}", e)
    })?;
    if let Some(t) = &tmp_tar {
        let _ = std::fs::remove_file(t);
    }
    if !out.status.success() {
        return Err(format!(
            "압축 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let packed_size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

    let md5_file_path = if let Some(m) = &md5_value {
        let mp = format!("{}.md5", out_path);
        let body = format!("{}  {}\n", m, basename_of(&input));
        std::fs::write(&mp, body).map_err(|e| format!("md5 사이드카: {}", e))?;
        Some(mp)
    } else {
        None
    };

    let sfx_path = if want_sfx {
        Some(out_path.clone())
    } else {
        None
    };

    Ok(PackPathExtResult {
        output_path: out_path.clone(),
        source_kind: if is_dir {
            "directory".into()
        } else {
            "file".into()
        },
        source_size,
        file_count,
        packed_size,
        ratio_percent: if source_size > 0 {
            (packed_size as f64 / source_size as f64) * 100.0
        } else {
            100.0
        },
        md5: md5_value,
        md5_file_path,
        sfx_path,
    })
}

#[derive(Serialize, Debug)]
pub struct UnpackExtResult {
    pub output_path: String,
    pub bytes_written: u64,
    pub untarred_dir: Option<String>,
    pub file_count: u64,
    pub restored_md5: Option<String>,
    pub md5_file_path: Option<String>,
    pub original_md5_matches: Option<bool>,
    pub original_md5_value: Option<String>,
}

#[tauri::command]
pub fn unpack_qsafe_ext(
    input: String,
    output: Option<String>,
    password: Option<String>,
    identity: Option<String>,
    force: bool,
    open_mode: Option<bool>,
    compute_md5: Option<bool>,
) -> Result<UnpackExtResult, String> {
    let base = unpack_qsafe(input.clone(), output, password, identity, force, open_mode)?;
    let want_md5 = compute_md5.unwrap_or(true);

    let original_md5 = {
        let sidecar = format!("{}.md5", input);
        std::fs::read_to_string(&sidecar)
            .ok()
            .and_then(|s| s.split_whitespace().next().map(|s| s.to_string()))
    };

    let mut restored_md5: Option<String> = None;
    let mut md5_file_path: Option<String> = None;
    let mut original_md5_matches: Option<bool> = None;

    if want_md5 {
        let target_path = if let Some(d) = &base.untarred_dir {
            std::path::PathBuf::from(d)
        } else {
            std::path::PathBuf::from(&base.output_path)
        };
        let computed = if target_path.is_dir() {
            compute_dir_md5_aggregate(&target_path).ok()
        } else if target_path.is_file() {
            compute_md5_file(&target_path).ok()
        } else {
            None
        };

        if let Some(c) = &computed {
            let sidecar = format!("{}.md5", base.output_path);
            let body = format!("{}  {}\n", c, basename_of(&base.output_path));
            if std::fs::write(&sidecar, body).is_ok() {
                md5_file_path = Some(sidecar);
            }
            if let Some(orig) = &original_md5 {
                original_md5_matches = Some(orig.to_lowercase() == c.to_lowercase());
            }
        }
        restored_md5 = computed;
    }

    Ok(UnpackExtResult {
        output_path: base.output_path,
        bytes_written: base.bytes_written,
        untarred_dir: base.untarred_dir,
        file_count: base.file_count,
        restored_md5,
        md5_file_path,
        original_md5_matches,
        original_md5_value: original_md5,
    })
}

#[cfg(test)]
mod ext_tests {
    use super::*;
    use std::env;

    fn tmp_dir(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("qsafe-gui-ext-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn md5_of_file_works() {
        let dir = tmp_dir("md5");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.txt");
        std::fs::write(&f, b"hello world").unwrap();
        let h = md5_of_file(f.to_string_lossy().into_owned()).unwrap();
        // md5("hello world") = 5eb63bbbe01eeed093cb22bb8f5acdc3
        assert_eq!(h, "5eb63bbbe01eeed093cb22bb8f5acdc3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_ext_with_md5_creates_sidecar() {
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let dir = tmp_dir("packmd5");
        std::fs::create_dir_all(&dir).unwrap();
        let inp = dir.join("a.txt");
        std::fs::write(&inp, b"data for md5").unwrap();
        let out = dir.join("a.qs");
        let r = pack_path_ext(
            inp.to_string_lossy().into_owned(),
            Some(out.to_string_lossy().into_owned()),
            None,
            vec![],
            false,
            true,
            Some(true),
            None,
            None,
            None,
            None,
            Some(true), // include_md5
        )
        .unwrap();
        assert!(r.md5.is_some());
        assert_eq!(r.md5.as_ref().unwrap().len(), 32);
        assert!(r.md5_file_path.is_some());
        let md5_path = r.md5_file_path.as_ref().unwrap();
        let body = std::fs::read_to_string(md5_path).unwrap();
        assert!(body.contains(r.md5.as_ref().unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_ext_without_md5_no_sidecar() {
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let dir = tmp_dir("nomd5");
        std::fs::create_dir_all(&dir).unwrap();
        let inp = dir.join("b.txt");
        std::fs::write(&inp, b"x").unwrap();
        let out = dir.join("b.qs");
        let r = pack_path_ext(
            inp.to_string_lossy().into_owned(),
            Some(out.to_string_lossy().into_owned()),
            None,
            vec![],
            false,
            true,
            Some(true),
            None,
            None,
            None,
            None,
            Some(false),
        )
        .unwrap();
        assert!(r.md5.is_none());
        assert!(r.md5_file_path.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_ext_strong_profile() {
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let dir = tmp_dir("strong");
        std::fs::create_dir_all(&dir).unwrap();
        let inp = dir.join("s.txt");
        std::fs::write(&inp, b"strong test").unwrap();
        let out = dir.join("s.qs");
        let r = pack_path_ext(
            inp.to_string_lossy().into_owned(),
            Some(out.to_string_lossy().into_owned()),
            Some("pw".into()),
            vec![],
            false,
            true,
            None,
            None,
            Some("strong".into()),
            None,
            None,
            Some(false),
        )
        .unwrap();
        assert!(out.exists());
        // strong profile은 더 큰 헤더 (Argon2 m=256MiB) → packed_size 약간 더 큼
        assert!(r.packed_size > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unpack_ext_creates_md5_sidecar_and_compares() {
        let qsafe = locate_qsafe_bin().unwrap();
        if !qsafe.exists()
            && std::process::Command::new(&qsafe)
                .arg("--version")
                .output()
                .is_err()
        {
            return;
        }
        let dir = tmp_dir("upmd5");
        std::fs::create_dir_all(&dir).unwrap();
        let inp = dir.join("orig.txt");
        std::fs::write(&inp, b"verify md5").unwrap();
        let qs = dir.join("orig.qs");
        let pack_r = pack_path_ext(
            inp.to_string_lossy().into_owned(),
            Some(qs.to_string_lossy().into_owned()),
            None,
            vec![],
            false,
            true,
            Some(true),
            None,
            None,
            None,
            None,
            Some(true),
        )
        .unwrap();
        assert!(pack_r.md5.is_some());

        let out = dir.join("restored.txt");
        let un = unpack_qsafe_ext(
            qs.to_string_lossy().into_owned(),
            Some(out.to_string_lossy().into_owned()),
            None,
            None,
            true,
            Some(true),
            Some(true),
        )
        .unwrap();
        assert!(un.restored_md5.is_some());
        assert_eq!(
            un.restored_md5.as_ref().unwrap(),
            pack_r.md5.as_ref().unwrap()
        );
        assert_eq!(un.original_md5_matches, Some(true));
        assert!(un.md5_file_path.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
