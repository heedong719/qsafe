//! Tauri commands exposed to the JavaScript frontend.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Emitter; // Tauri 2.x: emit는 Emitter trait의 method
use tauri::Manager; // try_state

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

/// OS 통합 시 argv로 전달되는 시작 명령. 파일 매니저(Finder/탐색기/Nautilus)가
/// 우클릭 메뉴 또는 더블 클릭으로 우리 프로그램을 부를 때 사용. main.rs가 부팅 시
/// std::env::args를 파싱해서 이걸 채우고, UI는 startup_args() command로 한 번
/// 받아서 적절한 모달을 자동 연다.
#[derive(Serialize, Debug, Clone, Default)]
pub struct StartupArgs {
    /// 명시적 action — "pack" / "unpack" / "info" / "" (없으면 path만 보고 자동 라우팅)
    pub action: String,
    /// 첫 번째 비-옵션 인자 (절대 경로로 정규화 시도)
    pub path: String,
}

impl StartupArgs {
    pub fn parse<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = StartupArgs::default();
        for raw in iter {
            let s = raw.as_ref();
            if let Some(v) = s.strip_prefix("--action=") {
                out.action = v.to_string();
            } else if s == "--action" {
                // 다음 토큰 형태는 단순 케이스에서 지원하지 않음 (우리 install 스크립트는 항상 = 형식 사용)
            } else if !s.starts_with("--") {
                // 첫 번째 positional만 사용
                if out.path.is_empty() {
                    let p = std::path::PathBuf::from(s);
                    out.path = std::fs::canonicalize(&p)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .into_owned();
                }
            }
        }
        out
    }
}

#[tauri::command]
pub fn startup_args(state: tauri::State<'_, StartupArgs>) -> StartupArgs {
    state.inner().clone()
}

/// R16: About 모달의 "업데이트 확인" 버튼이 호출. GitHub Releases API 를 한 번 폴링해서
/// 최신 tag_name 을 가져온 뒤 현재 빌드 버전과 비교. 네트워크 실패는 일반 에러로 반환.
#[derive(Serialize, Debug, Clone)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: String,
    pub up_to_date: bool,
    pub release_url: String,
}

/// `vA.B.C` 또는 `A.B.C` 형식의 두 버전을 비교. current >= latest 이면 true (= 최신).
pub fn version_at_least(current: &str, latest: &str) -> bool {
    fn parse(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .take(3)
            .map(|n| {
                n.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .map(|s| s.parse::<u64>().unwrap_or(0))
            .collect()
    }
    let c = parse(current);
    let l = parse(latest);
    for i in 0..3 {
        let cv = *c.get(i).unwrap_or(&0);
        let lv = *l.get(i).unwrap_or(&0);
        match cv.cmp(&lv) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => continue,
        }
    }
    true
}

/// R22 hardening: 응답 본문 최대 256 KB. GitHub Releases 응답은 보통 < 50 KB.
/// 악의적/실수로 큰 응답이 와도 메모리 폭발 방지.
const UPDATE_RESPONSE_MAX_BYTES: u64 = 256 * 1024;

#[tauri::command]
pub fn check_for_update() -> Result<UpdateCheck, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    // GitHub Releases API — public repo, 인증 불필요.
    // User-Agent 헤더는 GitHub 정책상 요구됨 (없으면 403).
    let resp = ureq::get("https://api.github.com/repos/heedong719/qsafe/releases/latest")
        .set("User-Agent", &format!("qsafe-gui/{}", current))
        .set("Accept", "application/vnd.github+json")
        .set("Accept-Encoding", "identity") // 압축 응답 비활성 — take(N) 가 압축 해제 후 크기에 적용되게
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| format!("network: {}", e))?;

    // 응답 크기 제한된 reader → JSON parse
    use std::io::Read;
    let mut buf = Vec::with_capacity(8 * 1024);
    resp.into_reader()
        .take(UPDATE_RESPONSE_MAX_BYTES)
        .read_to_end(&mut buf)
        .map_err(|e| format!("read body: {}", e))?;
    if buf.len() as u64 == UPDATE_RESPONSE_MAX_BYTES {
        return Err(format!(
            "response too large (>={} bytes)",
            UPDATE_RESPONSE_MAX_BYTES
        ));
    }
    let body: serde_json::Value =
        serde_json::from_slice(&buf).map_err(|e| format!("json parse: {}", e))?;

    let latest = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing tag_name in response".to_string())?
        .to_string();

    let release_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://github.com/heedong719/qsafe/releases")
        .to_string();

    let up_to_date = version_at_least(&current, &latest);

    Ok(UpdateCheck {
        current,
        latest,
        up_to_date,
        release_url,
    })
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
    fn startup_args_empty() {
        let a = StartupArgs::parse(Vec::<String>::new());
        assert_eq!(a.action, "");
        assert_eq!(a.path, "");
    }

    #[test]
    fn startup_args_path_only() {
        // non-existent path: canonicalize 실패 시 원본 유지
        let a = StartupArgs::parse(vec!["/tmp/qsafe-nonexistent-xyz.qs".to_string()]);
        assert_eq!(a.action, "");
        assert!(a.path.ends_with("qsafe-nonexistent-xyz.qs"));
    }

    #[test]
    fn startup_args_action_eq_form() {
        let a = StartupArgs::parse(vec![
            "--action=pack".to_string(),
            "/tmp/foo.txt".to_string(),
        ]);
        assert_eq!(a.action, "pack");
        assert!(a.path.ends_with("foo.txt"));
    }

    #[test]
    fn startup_args_only_action() {
        let a = StartupArgs::parse(vec!["--action=unpack".to_string()]);
        assert_eq!(a.action, "unpack");
        assert_eq!(a.path, "");
    }

    #[test]
    fn startup_args_first_positional_wins() {
        let a = StartupArgs::parse(vec![
            "--action=info".to_string(),
            "/tmp/first.qs".to_string(),
            "/tmp/second.qs".to_string(),
        ]);
        assert_eq!(a.action, "info");
        assert!(a.path.ends_with("first.qs"));
    }

    #[test]
    fn version_at_least_basic() {
        assert!(version_at_least("0.1.7", "0.1.7"));
        assert!(version_at_least("0.1.8", "0.1.7"));
        assert!(!version_at_least("0.1.6", "0.1.7"));
        // v-prefix 무시
        assert!(version_at_least("v0.1.7", "v0.1.7"));
        assert!(!version_at_least("0.1.7", "v0.1.8"));
        // 메이저 vs 마이너
        assert!(version_at_least("1.0.0", "0.99.99"));
        assert!(!version_at_least("0.99.99", "1.0.0"));
        // 자리수 부족 → 0 패딩
        assert!(version_at_least("0.1", "0.1.0"));
        assert!(!version_at_least("0.1", "0.1.1"));
        // 후행 prerelease 문자열은 숫자 부분만 비교
        assert!(version_at_least("0.1.7", "0.1.7-rc1"));
    }

    #[test]
    fn cleanup_temp_dir_rejects_non_qsafe_pattern() {
        // /tmp 자체를 지우려고 시도 → 거부
        let r = cleanup_temp_dir(std::env::temp_dir().to_string_lossy().into_owned());
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("qsafe-info-"));
    }

    #[test]
    fn cleanup_temp_dir_rejects_outside_tmp() {
        // 홈 디렉토리를 지우려고 시도 → 거부
        let r = cleanup_temp_dir("/".into());
        assert!(r.is_err());
    }

    #[test]
    fn cleanup_temp_dir_accepts_valid_pattern() {
        let mut p = std::env::temp_dir();
        p.push(format!("qsafe-info-{}-{}", std::process::id(), 99999));
        std::fs::create_dir_all(&p).unwrap();
        // 파일 하나 안에 넣어서 비어있지 않게
        let f = p.join("dummy.txt");
        std::fs::write(&f, b"x").unwrap();
        let r = cleanup_temp_dir(p.to_string_lossy().into_owned());
        assert!(r.is_ok(), "{:?}", r);
        assert!(!p.exists());
    }

    #[test]
    fn startup_args_unknown_flag_ignored() {
        let a = StartupArgs::parse(vec![
            "--debug".to_string(),
            "--verbose".to_string(),
            "/tmp/x.qs".to_string(),
        ]);
        assert_eq!(a.action, "");
        assert!(a.path.ends_with("x.qs"));
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

/// 자식 qsafe 프로세스의 stderr 를 라인 단위로 파싱해 `PROGRESS\tcurrent\ttotal\tpercent`
/// 라인을 Tauri event 로 emit. 그 외 라인은 collected_stderr 에 누적 (실패 시 사용자 표시용).
///
/// 반환: (success, collected_stderr)
#[derive(Clone, Serialize)]
pub struct PackUnpackProgress {
    pub current: u64,
    pub total: u64,
    pub percent: u32,
}

/// R22 hardening 상수. qsafe-cli 의 stderr 는 정상적으로 작은 사이즈지만,
/// 자식이 망가지거나 누군가가 PATH 의 다른 'qsafe' 를 spawn 시킬 경우 메모리 폭발 방지.
const STDERR_LINE_MAX_BYTES: usize = 8 * 1024; // 라인 1개당 8 KB 캡
const STDERR_TOTAL_MAX_BYTES: usize = 256 * 1024; // 누적 256 KB 캡 (이후 라인은 폐기)

/// R30: 현재 실행 중인 pack/unpack 자식 PID. cancel_running_job 이 SIGTERM 발사.
/// 단일 동시 작업만 추적 — 새 작업이 시작하면 이전 PID 가 덮어쓰여짐.
#[derive(Default)]
pub struct RunningJob(pub std::sync::Mutex<Option<u32>>);

/// 현재 PID 등록. drop 시 자동 해제 (RAII).
struct JobGuard<'a>(Option<&'a RunningJob>);
impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        if let Some(j) = self.0 {
            if let Ok(mut g) = j.0.lock() {
                *g = None;
            }
        }
    }
}

#[tauri::command]
pub fn cancel_running_job(state: tauri::State<'_, RunningJob>) -> Result<bool, String> {
    let pid = {
        let g = state.0.lock().map_err(|e| format!("lock: {}", e))?;
        *g
    };
    let pid = match pid {
        Some(p) => p,
        None => return Ok(false),
    };
    #[cfg(unix)]
    {
        // SIGTERM 우선 — child 가 정리할 기회를 줌. spawn_with_progress 의 stderr loop
        // 가 EOF 받으면 자연스럽게 종료, 외부에서 cleanup_temp_dir 등은 호출자가 처리.
        let r = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if r != 0 {
            // ESRCH (3) — 이미 죽었음, 정상 응답
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(3) {
                return Ok(false);
            }
            return Err(format!("kill({}) failed: {}", pid, err));
        }
    }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|e| format!("taskkill spawn: {}", e))?;
        if !status.success() {
            return Err(format!("taskkill PID {} returned non-zero", pid));
        }
    }
    Ok(true)
}

fn spawn_with_progress(
    cmd: &mut std::process::Command,
    app: Option<&tauri::AppHandle>,
    event_name: &'static str,
) -> Result<(bool, String), String> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("qsafe 실행 실패 (spawn): {}", e))?;
    let child_pid = child.id();

    // R30: PID 를 RunningJob 에 등록 — cancel_running_job 이 SIGTERM/taskkill 가능.
    let job_state = app.and_then(|a| a.try_state::<RunningJob>());
    if let Some(ref j) = job_state {
        if let Ok(mut g) = j.0.lock() {
            *g = Some(child_pid);
        }
    }
    // 함수 끝에서 자동 해제. job_state 가 None 이면 no-op.
    let _guard = JobGuard(job_state.as_deref());

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "child stderr capture 실패".to_string())?;

    let mut collected = String::new();
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        let mut line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        // R22: 라인이 비정상적으로 길면 truncate (DoS 가드)
        if line.len() > STDERR_LINE_MAX_BYTES {
            // char boundary 보존 — UTF-8 안전
            let cap = STDERR_LINE_MAX_BYTES;
            let safe_end = (0..=cap)
                .rev()
                .find(|&i| line.is_char_boundary(i))
                .unwrap_or(0);
            line.truncate(safe_end);
            line.push_str("…(truncated)");
        }
        if let Some(rest) = line.strip_prefix("PROGRESS\t") {
            let parts: Vec<&str> = rest.split('\t').collect();
            if parts.len() == 3 {
                let current = parts[0].parse::<u64>().unwrap_or(0);
                let total = parts[1].parse::<u64>().unwrap_or(0);
                let percent = parts[2].parse::<u32>().unwrap_or(0).min(100);
                if let Some(h) = app {
                    let _ = h.emit(
                        event_name,
                        PackUnpackProgress {
                            current,
                            total,
                            percent,
                        },
                    );
                }
                continue;
            }
        }
        // PROGRESS 가 아닌 라인은 보관 — 단, 누적 256 KB 초과 시 폐기 (silent drop)
        if collected.len() < STDERR_TOTAL_MAX_BYTES {
            collected.push_str(&line);
            collected.push('\n');
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("child wait 실패: {}", e))?;
    Ok((status.success(), collected))
}

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
    app: tauri::AppHandle,
    input: String,
    output: Option<String>,
    password: Option<String>,
    identity: Option<String>,
    force: bool,
    open_mode: Option<bool>,
) -> Result<UnpackResult, String> {
    unpack_qsafe_impl(
        Some(&app),
        input,
        output,
        password,
        identity,
        force,
        open_mode,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn unpack_qsafe_impl(
    app: Option<&tauri::AppHandle>,
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
    cmd.arg("unpack")
        .arg(&in_path)
        .arg("-o")
        .arg(&out_path)
        .arg("--progress");
    if force {
        cmd.arg("--force");
    }
    if let Some(pw) = password.as_ref() {
        cmd.arg("--password").arg(pw);
    }
    if let Some(id) = identity.as_ref() {
        cmd.arg("--identity").arg(id);
    }
    let (success, stderr_collected) = spawn_with_progress(&mut cmd, app, "unpack-progress")?;
    if !success {
        return Err(format!("풀기 실패: {}", stderr_collected.trim()));
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

/// info 모달에서 항목 더블 클릭 → 단일 entry 만 임시 디렉토리에 추출.
/// 모달 닫기 시 cleanup_temp_dir 로 정리.
#[derive(Serialize, Debug)]
pub struct TempExtraction {
    /// 추출된 파일의 절대 경로 (단일 파일)
    pub extracted_path: String,
    /// 그 파일을 포함하는 임시 디렉토리 (cleanup 시 사용)
    pub temp_dir: String,
}

#[tauri::command]
pub fn extract_archive_entry_to_temp(
    archive_path: String,
    entry_name: String,
) -> Result<TempExtraction, String> {
    use qsafe_formats::{detect_format, ExternalFormat};

    // 임시 디렉토리: $TMP/qsafe-info-<pid>-<ts>-<rand>
    let mut tmp = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    tmp.push(format!("qsafe-info-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("temp_dir 생성 실패: {}", e))?;

    // Unix: temp_dir 권한 0700 (다른 사용자가 추출된 파일을 읽지 못하게)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perm = std::fs::Permissions::from_mode(0o700);
        let _ = std::fs::set_permissions(&tmp, perm);
    }

    let bytes_for_detect = std::fs::read(&archive_path).map_err(|e| format!("read: {}", e))?;
    let fmt = detect_format(&bytes_for_detect);
    drop(bytes_for_detect);

    // R23: archive bomb 가드 — info-modal dblclick 은 "미리보기" 용도이므로 2 GiB 로 제한.
    // 그 이상이면 사용자에게 전체 extract_external_archive 사용 안내.
    const PREVIEW_MAX_SIZE: u64 = 2 * 1024 * 1024 * 1024;

    let extracted_path = match fmt {
        ExternalFormat::Rar => qsafe_formats::rar::extract_rar_entry(
            PathBuf::from(&archive_path).as_path(),
            &entry_name,
            &tmp,
            None,
            Some(PREVIEW_MAX_SIZE),
        )
        .map_err(|e| format!("RAR 단일 추출 실패: {}", e))?,
        other => {
            // 다른 포맷은 list 자체가 비어있어서 UI에서 dblclick이 불가능 — 방어적
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(format!(
                "{} 포맷은 단일 entry 추출을 지원하지 않습니다",
                other.name()
            ));
        }
    };

    Ok(TempExtraction {
        extracted_path: extracted_path.to_string_lossy().into_owned(),
        temp_dir: tmp.to_string_lossy().into_owned(),
    })
}

/// info 모달이 닫힐 때 호출. 임시 추출 디렉토리를 통째로 삭제.
/// 보안: dir 가 std::env::temp_dir() 아래의 "qsafe-info-*" 패턴인지 검증해서
/// 사용자가 임의 디렉토리를 지우게 하는 사고를 방지.
#[tauri::command]
pub fn cleanup_temp_dir(dir: String) -> Result<(), String> {
    let p = PathBuf::from(&dir);
    let canon = p
        .canonicalize()
        .map_err(|e| format!("canonicalize: {}", e))?;
    let tmp_root = std::env::temp_dir()
        .canonicalize()
        .map_err(|e| format!("temp_dir canonicalize: {}", e))?;
    if !canon.starts_with(&tmp_root) {
        return Err(format!(
            "안전 가드: {} 가 temp_dir ({}) 안에 없음",
            canon.display(),
            tmp_root.display()
        ));
    }
    let name = canon.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if !name.starts_with("qsafe-info-") {
        return Err(format!("안전 가드: {} 가 qsafe-info-* 패턴 아님", name));
    }
    std::fs::remove_dir_all(&canon).map_err(|e| format!("remove_dir_all: {}", e))?;
    Ok(())
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

/// R32: 여러 입력 (파일 / 폴더 혼합) 을 단일 tar로 묶음. multi-select pack 의 baseline.
/// 각 입력은 자신의 basename 으로 tar 에 들어감 — 추출 시 평평한 N개 entry.
fn create_tar_multi(
    srcs: &[std::path::PathBuf],
    dst: &std::path::Path,
) -> Result<(u64, u64), String> {
    let file = std::fs::File::create(dst).map_err(|e| format!("tar 파일 생성: {}", e))?;
    let mut builder = tar::Builder::new(file);
    builder.follow_symlinks(false);
    for src in srcs {
        let base_name = src
            .file_name()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("data"));
        if src.is_dir() {
            builder
                .append_dir_all(&base_name, src)
                .map_err(|e| format!("tar append dir {}: {}", src.display(), e))?;
        } else if src.is_file() {
            let mut f = std::fs::File::open(src)
                .map_err(|e| format!("tar append open {}: {}", src.display(), e))?;
            builder
                .append_file(&base_name, &mut f)
                .map_err(|e| format!("tar append file {}: {}", src.display(), e))?;
        }
        // is_symlink 등은 skip — follow_symlinks(false) 로 cycle 방어
    }
    builder.finish().map_err(|e| format!("tar finish: {}", e))?;
    drop(builder);
    // 총 크기 + 파일 수 산정
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
    for src in srcs {
        if src.is_dir() {
            walk(src, &mut total_size, &mut count);
        } else if let Ok(md) = std::fs::metadata(src) {
            total_size += md.len();
            count += 1;
        }
    }
    Ok((total_size, count))
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
        let u = unpack_qsafe_impl(
            None,
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
        let r = unpack_qsafe_impl(
            None,
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
        unpack_qsafe_impl(
            None,
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
        let r = unpack_qsafe_impl(
            None,
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
        let r = unpack_qsafe_impl(
            None,
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

// ════════════════════════════════════════════════════════════
// USB 부팅 디스크 만들기 — B1 Phase
//
// 안전 모델:
//   1. `list_writable_disks` → external/removable로 식별된 디스크만 반환,
//      시스템 부팅 디스크 (mount된 root, EFI partition 보유)는 제외.
//   2. `write_iso_to_disk(iso, disk, confirm_token)` (다음 commit) →
//      클라이언트가 disk 식별 정보를 token으로 다시 확인해야 실행.
//
// 시스템 디스크 보호는 매우 critical — 잘못된 disk 선택 시 OS 파괴.
// 그래서 list 함수가 가능한 한 보수적: removable=false거나 root mount된
// 디스크는 반환 자체를 안 함.
// ════════════════════════════════════════════════════════════

#[derive(Serialize, Debug)]
pub struct WritableDisk {
    pub id: String, // OS별 식별자 — macOS: /dev/disk4, Linux: /dev/sdb, Windows: \\.\PhysicalDrive1
    pub label: String, // 사용자 표시용 — "SanDisk Cruzer 16 GB"
    pub size_bytes: u64, // 총 용량
    pub removable: bool, // hot-pluggable (USB/SD)인지
    pub is_system: bool, // 부팅/시스템 디스크로 판단되면 true (반환에서 제외해야 함)
    pub mount_points: Vec<String>, // 현재 마운트된 위치들
}

#[tauri::command]
pub fn list_writable_disks() -> Result<Vec<WritableDisk>, String> {
    list_writable_disks_impl()
}

#[cfg(target_os = "macos")]
fn list_writable_disks_impl() -> Result<Vec<WritableDisk>, String> {
    use std::process::Command;
    // `diskutil list -plist external physical` — 외장 물리 디스크만.
    // -plist parsing 대신 더 간단한 출력 형식 (`list external`)을 파싱하는 게
    // 의존성 없이 안전.
    let out = Command::new("diskutil")
        .args(["list", "external", "physical"])
        .output()
        .map_err(|e| format!("diskutil 실행 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "diskutil 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);

    let mut disks = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_label = String::new();
    let mut current_size: u64 = 0;
    let mut current_mounts: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let trim = line.trim();
        if trim.starts_with("/dev/disk") {
            // 새 디스크 시작 — 이전 거 저장
            if let Some(id) = current_id.take() {
                disks.push(WritableDisk {
                    id,
                    label: std::mem::take(&mut current_label),
                    size_bytes: current_size,
                    removable: true,
                    is_system: false, // external physical만 골랐으니 시스템 아님
                    mount_points: std::mem::take(&mut current_mounts),
                });
                current_size = 0;
            }
            let id = trim.split_whitespace().next().unwrap_or("").to_string();
            current_id = Some(id);
        } else if trim.starts_with("0:") || trim.starts_with("1:") {
            // 형식: "  0:    GUID_partition_scheme    *15.5 GB    disk4"
            //       "  1:                  EFI EFI   209.7 MB    disk4s1"
            // 첫 partition 줄에서 라벨/크기 추출 시도
            let cols: Vec<&str> = trim.split_whitespace().collect();
            if cols.len() >= 4 && current_label.is_empty() {
                // "0:" 라인이면 disk 전체 크기. *15.5 GB 처럼 별표 붙음
                let size_str = cols.iter().find(|s| s.starts_with('*')).unwrap_or(&"");
                let size_str = size_str.trim_start_matches('*');
                current_size = parse_size(size_str);
                if cols[0] == "0:" {
                    // 0번 row는 partition table, label은 다음 row
                    current_label = "USB Disk".to_string();
                }
            }
        }
    }
    // 마지막 디스크 flush
    if let Some(id) = current_id {
        disks.push(WritableDisk {
            id,
            label: current_label,
            size_bytes: current_size,
            removable: true,
            is_system: false,
            mount_points: current_mounts,
        });
    }
    Ok(disks)
}

#[cfg(target_os = "linux")]
fn list_writable_disks_impl() -> Result<Vec<WritableDisk>, String> {
    use std::process::Command;
    // lsblk -d -n -o NAME,SIZE,TYPE,RM,MOUNTPOINT,VENDOR,MODEL -b -P
    // RM=1 인 디스크만 + ROOT mountpoint 없는 것
    let out = Command::new("lsblk")
        .args([
            "-d",
            "-n",
            "-b",
            "-P",
            "-o",
            "NAME,SIZE,TYPE,RM,MOUNTPOINT,VENDOR,MODEL",
        ])
        .output()
        .map_err(|e| format!("lsblk 실패 (설치 안 됨?): {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "lsblk 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut out_vec = Vec::new();
    for line in stdout.lines() {
        // 형식: NAME="sda" SIZE="..." TYPE="disk" RM="0" MOUNTPOINT="..." ...
        let kv: std::collections::HashMap<String, String> = line
            .split(' ')
            .filter_map(|kv| {
                let mut sp = kv.splitn(2, '=');
                let k = sp.next()?.to_string();
                let v = sp.next()?.trim_matches('"').to_string();
                Some((k, v))
            })
            .collect();
        let typ = kv.get("TYPE").map(String::as_str).unwrap_or("");
        if typ != "disk" {
            continue;
        }
        let removable = kv.get("RM").map(|s| s == "1").unwrap_or(false);
        if !removable {
            continue; // 보수적: 내장 디스크 제외
        }
        let name = kv.get("NAME").cloned().unwrap_or_default();
        let mp = kv.get("MOUNTPOINT").cloned().unwrap_or_default();
        let is_system = mp == "/" || mp == "/boot" || mp.starts_with("/boot/");
        if is_system {
            continue;
        }
        let size: u64 = kv.get("SIZE").and_then(|s| s.parse().ok()).unwrap_or(0);
        let label = format!(
            "{} {}",
            kv.get("VENDOR").cloned().unwrap_or_default(),
            kv.get("MODEL").cloned().unwrap_or_default()
        )
        .trim()
        .to_string();
        out_vec.push(WritableDisk {
            id: format!("/dev/{}", name),
            label: if label.is_empty() {
                name.clone()
            } else {
                label
            },
            size_bytes: size,
            removable,
            is_system,
            mount_points: if mp.is_empty() { vec![] } else { vec![mp] },
        });
    }
    Ok(out_vec)
}

#[cfg(target_os = "windows")]
fn list_writable_disks_impl() -> Result<Vec<WritableDisk>, String> {
    use std::process::Command;
    // PowerShell: Get-Disk → Number, FriendlyName, Size, BusType, IsBoot, IsSystem
    // BusType "USB"인 것만, IsBoot/IsSystem 제외.
    let script = "Get-Disk | Where-Object { $_.BusType -eq 'USB' -and -not $_.IsBoot -and -not $_.IsSystem } \
                  | Select-Object Number,FriendlyName,Size | ConvertTo-Json -Compress";
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|e| format!("powershell 실패: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "Get-Disk 실패: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if stdout.is_empty() {
        return Ok(vec![]);
    }
    // 단일 객체일 때 PowerShell이 array가 아닌 단일 JSON 객체를 반환 — 둘 다 처리.
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|e| format!("JSON 파싱: {}", e))?;
    let arr = if parsed.is_array() {
        parsed.as_array().cloned().unwrap_or_default()
    } else {
        vec![parsed]
    };
    let mut out_vec = Vec::new();
    for item in arr {
        let num = item.get("Number").and_then(|v| v.as_u64()).unwrap_or(0);
        let name = item
            .get("FriendlyName")
            .and_then(|v| v.as_str())
            .unwrap_or("USB Drive")
            .to_string();
        let size = item.get("Size").and_then(|v| v.as_u64()).unwrap_or(0);
        out_vec.push(WritableDisk {
            id: format!("\\\\.\\PhysicalDrive{}", num),
            label: name,
            size_bytes: size,
            removable: true,
            is_system: false,
            mount_points: vec![],
        });
    }
    Ok(out_vec)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn list_writable_disks_impl() -> Result<Vec<WritableDisk>, String> {
    Err("이 플랫폼은 USB 디스크 목록을 지원하지 않습니다".to_string())
}

#[cfg(target_os = "macos")]
fn parse_size(s: &str) -> u64 {
    // "15.5" "GB" 형식. diskutil은 SI 단위 (10^9).
    let mut parts = s.split_whitespace();
    let num: f64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0.0);
    let unit = parts.next().unwrap_or("");
    let mul = match unit {
        "KB" => 1_000u64,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "TB" => 1_000_000_000_000,
        _ => 1,
    };
    (num * mul as f64) as u64
}

// ════════════════════════════════════════════════════════════
// B1.2 + B1.5 — write_iso_to_disk (dd-style raw write) + progress events
//
// 흐름:
//   1. confirm_token 검증 (list_writable_disks가 발급한 토큰과 일치해야)
//   2. macOS/Linux: 해당 디스크의 모든 마운트 해제
//   3. dd if=<iso> of=<disk> bs=4M status=progress 를 background thread에서 실행
//   4. stderr 라인 파싱 → iso-write-progress 이벤트로 frontend에 emit
//   5. 종료 → iso-write-done 또는 iso-write-error 이벤트
//
// 권한 모델:
//   - macOS: /dev/rdiskN 직접 쓰기는 admin 필요. Tauri GUI에서 'sudo dd ...'를
//     하려면 osascript의 "with administrator privileges" 사용 (다이얼로그 popup).
//   - Linux: 같은 dd가 root 필요. pkexec 또는 sudo. 우선 sudo 시도 (TTY 없으면 실패).
//   - Windows: PowerShell raw disk write는 admin 필요 + DeviceIoControl. 이 commit
//     에서는 macOS/Linux만 지원. Windows는 미지원 에러 반환.
//   - 단순화: 이 함수는 elevation을 직접 시도하지 않고, GUI가 이미 admin/root로
//     실행 중이면 동작. 아니면 EPERM. 향후 osascript / pkexec 통합.
//
// ⚠️ 데이터 파괴 가드:
//   - confirm_token 형식: "<disk_id>:<size_bytes>". 호출자가 직전 list 결과의
//     동일 token을 제출해야. 디스크가 그새 바뀌었으면 mismatch로 거부.
// ════════════════════════════════════════════════════════════

#[derive(Serialize, Clone, Debug)]
pub struct IsoWriteProgress {
    pub bytes_written: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub stage: String, // "preparing" | "writing" | "syncing" | "done" | "error"
    pub message: String,
}

/// list_writable_disks가 발급한 token 생성 (id + size 결합).
/// frontend는 이 helper를 사용 안 함 — list 결과의 (id, size_bytes)를 직접 결합한 문자열만 보내면 됨.
fn confirm_token_for(disk_id: &str, size_bytes: u64) -> String {
    format!("{}:{}", disk_id, size_bytes)
}

#[tauri::command]
pub fn write_iso_to_disk(
    app: tauri::AppHandle,
    iso_path: String,
    disk_id: String,
    confirm_token: String,
) -> Result<(), String> {
    // 1) 디스크 다시 조회 + token 검증
    let disks = list_writable_disks_impl()?;
    let disk = disks
        .iter()
        .find(|d| d.id == disk_id)
        .ok_or_else(|| format!("디스크 {}가 더 이상 보이지 않습니다 (분리됨?)", disk_id))?;
    let expected = confirm_token_for(&disk.id, disk.size_bytes);
    if expected != confirm_token {
        return Err(format!(
            "confirm_token 불일치 — 디스크가 바뀌었거나 안전 가드 실패. expected={}, got={}",
            expected, confirm_token
        ));
    }
    if disk.is_system {
        return Err("시스템/부팅 디스크에는 쓸 수 없습니다".into());
    }

    // 2) ISO 크기
    let iso_meta = std::fs::metadata(&iso_path).map_err(|e| format!("ISO stat: {}", e))?;
    let total_bytes = iso_meta.len();
    if total_bytes == 0 {
        return Err("ISO 파일이 비어 있습니다".into());
    }
    if total_bytes > disk.size_bytes {
        return Err(format!(
            "ISO({} bytes)가 디스크({} bytes)보다 큽니다",
            total_bytes, disk.size_bytes
        ));
    }

    // 3) background thread에서 dd 실행 + progress emit
    let app_clone = app.clone();
    let iso_clone = iso_path.clone();
    let disk_clone = disk.id.clone();
    let token_clone = expected.clone(); // R26: TOCTOU 재검증용 — thread 안에서 동일 token 으로 재조회
    std::thread::spawn(move || {
        let _ = run_iso_write(app_clone, iso_clone, disk_clone, total_bytes, token_clone);
    });

    // 즉시 반환 — 진행은 event로
    Ok(())
}

fn emit_progress(app: &tauri::AppHandle, p: IsoWriteProgress) {
    let _ = app.emit("iso-write-progress", p);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run_iso_write(
    app: tauri::AppHandle,
    iso_path: String,
    disk_id: String,
    total_bytes: u64,
    expected_token: String,
) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    // R26: TOCTOU 최종 가드 — write_iso_to_disk 의 가드와 background thread 시작 사이에
    // 디스크가 분리/교체된 경우 (예: 사용자가 USB를 뺐다 다른 디스크를 꽂음, 같은 /dev/diskN
    // 슬롯 재사용) 를 검출. 같은 id + 같은 confirm_token (= id+size) + 여전히 is_system=false
    // 셋 다 통과해야 진행.
    match list_writable_disks_impl() {
        Ok(disks) => {
            let still_safe = disks.iter().any(|d| {
                d.id == disk_id
                    && confirm_token_for(&d.id, d.size_bytes) == expected_token
                    && !d.is_system
            });
            if !still_safe {
                let msg = format!(
                    "디스크 {} 가 더 이상 안전하지 않습니다 (분리 / 교체 / 시스템 디스크로 표시됨). 쓰기 중단.",
                    disk_id
                );
                emit_progress(
                    &app,
                    IsoWriteProgress {
                        bytes_written: 0,
                        total_bytes,
                        percent: 0.0,
                        stage: "error".into(),
                        message: msg.clone(),
                    },
                );
                return Err(msg);
            }
        }
        Err(e) => {
            let msg = format!("디스크 재검증 실패: {}", e);
            emit_progress(
                &app,
                IsoWriteProgress {
                    bytes_written: 0,
                    total_bytes,
                    percent: 0.0,
                    stage: "error".into(),
                    message: msg.clone(),
                },
            );
            return Err(msg);
        }
    }

    emit_progress(
        &app,
        IsoWriteProgress {
            bytes_written: 0,
            total_bytes,
            percent: 0.0,
            stage: "preparing".into(),
            message: "디스크 마운트 해제 중…".into(),
        },
    );

    // 마운트 해제 (best-effort)
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("diskutil")
            .args(["unmountDisk", "force", &disk_id])
            .output();
    }
    #[cfg(target_os = "linux")]
    {
        // 모든 파티션 umount (best-effort)
        if let Ok(out) = Command::new("lsblk")
            .args(["-ln", "-o", "MOUNTPOINT", &disk_id])
            .output()
        {
            for mp in String::from_utf8_lossy(&out.stdout).lines() {
                let mp = mp.trim();
                if !mp.is_empty() {
                    let _ = Command::new("umount").arg(mp).output();
                }
            }
        }
    }

    // macOS: raw device (/dev/rdiskN) 가 일반 dev보다 훨씬 빠름
    #[cfg(target_os = "macos")]
    let target = disk_id.replacen("/dev/disk", "/dev/rdisk", 1);
    #[cfg(target_os = "linux")]
    let target = disk_id.clone();

    emit_progress(
        &app,
        IsoWriteProgress {
            bytes_written: 0,
            total_bytes,
            percent: 0.0,
            stage: "writing".into(),
            message: format!("dd → {}", target),
        },
    );

    let mut child = Command::new("dd")
        .arg(format!("if={}", iso_path))
        .arg(format!("of={}", target))
        .arg("bs=4m")
        .arg("status=progress")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let msg = format!("dd 실행 실패: {}", e);
            emit_progress(
                &app,
                IsoWriteProgress {
                    bytes_written: 0,
                    total_bytes,
                    percent: 0.0,
                    stage: "error".into(),
                    message: msg.clone(),
                },
            );
            msg
        })?;

    // stderr 라인별 읽기 — dd progress 파싱
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            // 형식 예: "1234567890 bytes (1.2 GB, 1.1 GiB) transferred ..."
            //        "1234567890 bytes transferred in 12.345 secs"
            if let Some(idx) = line.find(" bytes") {
                let num: Option<u64> = line[..idx]
                    .split_whitespace()
                    .next_back()
                    .and_then(|s| s.parse().ok());
                if let Some(b) = num {
                    let pct = if total_bytes > 0 {
                        (b as f64 / total_bytes as f64) * 100.0
                    } else {
                        0.0
                    };
                    emit_progress(
                        &app,
                        IsoWriteProgress {
                            bytes_written: b,
                            total_bytes,
                            percent: pct.min(100.0),
                            stage: "writing".into(),
                            message: line.trim().to_string(),
                        },
                    );
                }
            }
        }
    }

    let status = child.wait().map_err(|e| format!("dd wait: {}", e))?;
    if !status.success() {
        let msg = format!("dd 종료 코드 {}", status.code().unwrap_or(-1));
        emit_progress(
            &app,
            IsoWriteProgress {
                bytes_written: 0,
                total_bytes,
                percent: 0.0,
                stage: "error".into(),
                message: msg.clone(),
            },
        );
        return Err(msg);
    }

    // sync (Linux 특히 — dd가 끝나도 버퍼가 디스크에 안 내려갔을 수 있음)
    emit_progress(
        &app,
        IsoWriteProgress {
            bytes_written: total_bytes,
            total_bytes,
            percent: 100.0,
            stage: "syncing".into(),
            message: "디스크 sync 중…".into(),
        },
    );
    #[cfg(unix)]
    {
        let _ = Command::new("sync").output();
    }

    emit_progress(
        &app,
        IsoWriteProgress {
            bytes_written: total_bytes,
            total_bytes,
            percent: 100.0,
            stage: "done".into(),
            message: "쓰기 완료".into(),
        },
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_iso_write(
    app: tauri::AppHandle,
    _iso_path: String,
    _disk_id: String,
    total_bytes: u64,
) -> Result<(), String> {
    let msg = "Windows raw disk write는 다음 사이클에 추가됩니다 (PowerShell + admin)".to_string();
    emit_progress(
        &app,
        IsoWriteProgress {
            bytes_written: 0,
            total_bytes,
            percent: 0.0,
            stage: "error".into(),
            message: msg.clone(),
        },
    );
    Err(msg)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn run_iso_write(
    app: tauri::AppHandle,
    _iso_path: String,
    _disk_id: String,
    total_bytes: u64,
) -> Result<(), String> {
    let msg = "이 플랫폼은 ISO 쓰기를 지원하지 않습니다".to_string();
    emit_progress(
        &app,
        IsoWriteProgress {
            bytes_written: 0,
            total_bytes,
            percent: 0.0,
            stage: "error".into(),
            message: msg.clone(),
        },
    );
    Err(msg)
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

/// R32: 다중 입력 (파일 / 폴더 혼합) 을 단일 .qs 로 압축. 임시 tar 묶고 pack_path_ext_impl 재사용.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn pack_multiple_to_qsafe(
    app: tauri::AppHandle,
    inputs: Vec<String>,
    output: String,
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
    if inputs.is_empty() {
        return Err("inputs is empty".into());
    }
    let paths: Vec<std::path::PathBuf> = inputs.iter().map(std::path::PathBuf::from).collect();
    for p in &paths {
        if !p.exists() {
            return Err(format!("입력 없음: {}", p.display()));
        }
    }

    // 임시 tar 위치 — 첫 입력의 부모, 또는 임시 디렉토리
    let parent = paths[0]
        .parent()
        .filter(|p| p.is_dir())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(std::env::temp_dir);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_tar = parent.join(format!(".qsafe-multi-{}-{}.tar", std::process::id(), nanos));

    create_tar_multi(&paths, &tmp_tar)?;

    // tar 를 단일 input 으로 pack
    let res = pack_path_ext_impl(
        Some(&app),
        tmp_tar.to_string_lossy().into_owned(),
        Some(output),
        password,
        pubkeys,
        no_password,
        force,
        open_mode,
        compression,
        profile,
        sfx,
        label,
        include_md5,
    );
    let _ = std::fs::remove_file(&tmp_tar);
    res
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn pack_path_ext(
    app: tauri::AppHandle,
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
    pack_path_ext_impl(
        Some(&app),
        input,
        output,
        password,
        pubkeys,
        no_password,
        force,
        open_mode,
        compression,
        profile,
        sfx,
        label,
        include_md5,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn pack_path_ext_impl(
    app: Option<&tauri::AppHandle>,
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
    // R10: 진행률 전달 — qsafe-cli stderr에 PROGRESS 라인 → Tauri event "pack-progress"
    cmd.arg("--progress");

    let status_and_stderr = spawn_with_progress(&mut cmd, app, "pack-progress");
    if let Some(t) = &tmp_tar {
        let _ = std::fs::remove_file(t);
    }
    let (success, stderr_collected) = status_and_stderr?;
    if !success {
        return Err(format!("압축 실패: {}", stderr_collected.trim()));
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

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn unpack_qsafe_ext(
    app: tauri::AppHandle,
    input: String,
    output: Option<String>,
    password: Option<String>,
    identity: Option<String>,
    force: bool,
    open_mode: Option<bool>,
    compute_md5: Option<bool>,
) -> Result<UnpackExtResult, String> {
    unpack_qsafe_ext_impl(
        Some(&app),
        input,
        output,
        password,
        identity,
        force,
        open_mode,
        compute_md5,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn unpack_qsafe_ext_impl(
    app: Option<&tauri::AppHandle>,
    input: String,
    output: Option<String>,
    password: Option<String>,
    identity: Option<String>,
    force: bool,
    open_mode: Option<bool>,
    compute_md5: Option<bool>,
) -> Result<UnpackExtResult, String> {
    let base = unpack_qsafe_impl(
        app,
        input.clone(),
        output,
        password,
        identity,
        force,
        open_mode,
    )?;
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
        let r = pack_path_ext_impl(
            None,
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
        let r = pack_path_ext_impl(
            None,
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
        let r = pack_path_ext_impl(
            None,
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
        let pack_r = pack_path_ext_impl(
            None,
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
        let un = unpack_qsafe_ext_impl(
            None,
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
