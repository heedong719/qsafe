//! E2E 통합 테스트. 실제 qsafe 바이너리를 호출하여 pack/unpack roundtrip 검증.
//!
//! `cargo test --release` 권장 (Argon2id 빠름).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// cargo는 통합 테스트 빌드 시 CARGO_BIN_EXE_<name>을 자동으로 설정.
/// env!() 매크로는 컴파일 타임에 검증하므로 stale 바이너리 문제 없음.
fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_qsafe"))
}

fn run(args: &[&str]) -> (bool, String, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("failed to run qsafe");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let mut p = std::env::temp_dir();
        p.push(format!("qsafe-e2e-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self { path: p }
    }

    fn p(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn pack_unpack_roundtrip() {
    let d = TestDir::new("roundtrip");
    let original = d.p("sample.txt");
    let packed = d.p("sample.cl");
    let unpacked = d.p("sample.out");

    fs::write(&original, b"hello qsafe E2E").unwrap();

    let (ok, _out, err) = run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "test1234",
    ]);
    assert!(ok, "pack failed: {}", err);
    assert!(packed.exists());

    let (ok, _, err) = run(&[
        "unpack",
        packed.to_str().unwrap(),
        "-o",
        unpacked.to_str().unwrap(),
        "--password",
        "test1234",
    ]);
    assert!(ok, "unpack failed: {}", err);

    let orig_bytes = fs::read(&original).unwrap();
    let out_bytes = fs::read(&unpacked).unwrap();
    assert_eq!(orig_bytes, out_bytes);
}

#[test]
fn wrong_password_fails() {
    let d = TestDir::new("wrong-pw");
    let original = d.p("a.txt");
    let packed = d.p("a.cl");
    fs::write(&original, b"secret data").unwrap();

    run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "correct",
    ]);

    let (ok, _, err) = run(&[
        "unpack",
        packed.to_str().unwrap(),
        "-o",
        d.p("a.out").to_str().unwrap(),
        "--password",
        "wrong",
    ]);
    assert!(!ok, "should have failed");
    assert!(err.contains("패스워드") || err.contains("aead"));
}

#[test]
fn refuses_overwrite_without_force() {
    let d = TestDir::new("no-overwrite");
    let original = d.p("b.txt");
    let packed = d.p("b.cl");
    fs::write(&original, b"data").unwrap();
    fs::write(&packed, b"existing").unwrap();

    let (ok, _, err) = run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "p",
    ]);
    assert!(!ok);
    assert!(err.contains("이미 존재"));
}

#[test]
fn force_allows_overwrite() {
    let d = TestDir::new("force");
    let original = d.p("c.txt");
    let packed = d.p("c.cl");
    fs::write(&original, b"data").unwrap();
    fs::write(&packed, b"existing").unwrap();

    let (ok, _, err) = run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "p",
        "--force",
    ]);
    assert!(ok, "force pack failed: {}", err);
}

#[test]
fn trailing_garbage_rejected() {
    let d = TestDir::new("trailing");
    let original = d.p("d.txt");
    let packed = d.p("d.cl");
    fs::write(&original, b"hello").unwrap();

    run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "p",
    ]);

    let mut data = fs::read(&packed).unwrap();
    data.extend_from_slice(b"EXTRA");
    fs::write(&packed, data).unwrap();

    let (ok, _, err) = run(&["info", packed.to_str().unwrap()]);
    assert!(!ok);
    assert!(err.contains("trailing") || err.contains("tampered"));
}

#[test]
fn ciphertext_tampering_rejected() {
    let d = TestDir::new("tampered");
    let original = d.p("e.txt");
    let packed = d.p("e.cl");
    fs::write(&original, b"hello qsafe with some content").unwrap();

    run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "p",
    ]);

    // payload 영역의 한 바이트를 뒤집음 (헤더는 보존)
    let mut data = fs::read(&packed).unwrap();
    let len = data.len();
    // 마지막 hash(32B) 직전이 payload — 안전하게 hash 영역도 건드림
    data[len - 50] ^= 1;
    fs::write(&packed, data).unwrap();

    let (ok, _, err) = run(&[
        "unpack",
        packed.to_str().unwrap(),
        "-o",
        d.p("e.out").to_str().unwrap(),
        "--password",
        "p",
    ]);
    assert!(!ok);
    // integrity, aead, 또는 패스워드 오류로 보고됨
    assert!(err.contains("integrity") || err.contains("aead") || err.contains("패스워드"));
}

#[test]
fn empty_file_roundtrip() {
    let d = TestDir::new("empty");
    let original = d.p("z.txt");
    let packed = d.p("z.cl");
    let unpacked = d.p("z.out");
    fs::write(&original, b"").unwrap();

    let (ok, _, err) = run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "p",
    ]);
    assert!(ok, "empty pack failed: {}", err);

    let (ok, _, err) = run(&[
        "unpack",
        packed.to_str().unwrap(),
        "-o",
        unpacked.to_str().unwrap(),
        "--password",
        "p",
    ]);
    assert!(ok, "empty unpack failed: {}", err);

    assert_eq!(fs::read(&unpacked).unwrap(), b"");
}

#[test]
fn same_input_output_rejected() {
    let d = TestDir::new("same-path");
    let f = d.p("x.txt");
    fs::write(&f, b"data").unwrap();

    let (ok, _, err) = run(&[
        "pack",
        f.to_str().unwrap(),
        "-o",
        f.to_str().unwrap(),
        "--password",
        "p",
        "--force",
    ]);
    assert!(!ok);
    assert!(err.contains("같은 파일"));
}

#[test]
fn info_shows_recipient_info() {
    let d = TestDir::new("info");
    let original = d.p("i.txt");
    let packed = d.p("i.cl");
    fs::write(&original, b"info test").unwrap();

    run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "p",
        "--label",
        "원본문서",
    ]);

    let (ok, out, _) = run(&["info", packed.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("format version"));
    assert!(out.contains("cipher suite"));
    assert!(out.contains("password"));
    assert!(out.contains("Argon2id"));
    assert!(out.contains("원본문서"));
    assert!(out.contains("recipients"));
}

#[test]
fn large_random_binary_roundtrip() {
    let d = TestDir::new("large-random");
    let original = d.p("rand.bin");
    let packed = d.p("rand.cl");
    let unpacked = d.p("rand.out");

    // 256KB random
    let mut data = vec![0u8; 256 * 1024];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut data);
    fs::write(&original, &data).unwrap();

    let (ok, _, err) = run(&[
        "pack",
        original.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--password",
        "p",
    ]);
    assert!(ok, "pack failed: {}", err);

    let (ok, _, err) = run(&[
        "unpack",
        packed.to_str().unwrap(),
        "-o",
        unpacked.to_str().unwrap(),
        "--password",
        "p",
    ]);
    assert!(ok, "unpack failed: {}", err);

    assert_eq!(fs::read(&unpacked).unwrap(), data);
}

/// `qsafe identity generate / show / export-pubkey` 라운드트립 (v0.1.5+).
/// 검증: 0600 권한 (Unix), fingerprint 일치, show가 secret/public 모두 인식.
#[test]
fn identity_generate_show_export_roundtrip() {
    let d = TestDir::new("identity-rt");
    let secret = d.p("id.json");
    let public = d.p("id.pub.json");

    let (ok, stdout, err) = run(&["identity", "generate", "-o", secret.to_str().unwrap()]);
    assert!(ok, "identity generate failed: {}", err);
    assert!(
        stdout.contains("fingerprint"),
        "missing fingerprint in: {}",
        stdout
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&secret).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "secret identity must be 0600, got {:o}", mode);
    }

    let (ok, _, err) = run(&[
        "identity",
        "export-pubkey",
        secret.to_str().unwrap(),
        "-o",
        public.to_str().unwrap(),
    ]);
    assert!(ok, "export-pubkey failed: {}", err);
    assert!(public.exists());

    let (ok_s, out_s, _) = run(&["identity", "show", secret.to_str().unwrap()]);
    assert!(ok_s);
    let (ok_p, out_p, _) = run(&["identity", "show", public.to_str().unwrap()]);
    assert!(ok_p);

    let fp_line = |s: &str| -> String {
        s.lines()
            .find(|l| l.contains("fingerprint"))
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(fp_line(&out_s), fp_line(&out_p), "fingerprint mismatch");
}

/// X25519 + ML-KEM-768 하이브리드 pack/unpack 라운드트립 (v0.1.5+).
#[test]
fn pack_unpack_with_pubkey_recipient() {
    let d = TestDir::new("pubkey-rt");
    let recipient_secret = d.p("recipient.json");
    let recipient_public = d.p("recipient.pub.json");
    let plaintext = d.p("secret.txt");
    let packed = d.p("secret.qs");
    let restored = d.p("secret.out");

    let (ok, _, err) = run(&[
        "identity",
        "generate",
        "-o",
        recipient_secret.to_str().unwrap(),
    ]);
    assert!(ok, "generate failed: {}", err);
    let (ok, _, err) = run(&[
        "identity",
        "export-pubkey",
        recipient_secret.to_str().unwrap(),
        "-o",
        recipient_public.to_str().unwrap(),
    ]);
    assert!(ok, "export-pubkey failed: {}", err);

    let data = b"hybrid PQ envelope test \xff\x00\x42";
    fs::write(&plaintext, data).unwrap();

    let (ok, _, err) = run(&[
        "pack",
        plaintext.to_str().unwrap(),
        "-o",
        packed.to_str().unwrap(),
        "--no-password",
        "--pubkey",
        recipient_public.to_str().unwrap(),
    ]);
    assert!(ok, "pack --pubkey failed: {}", err);

    let (ok, _, err) = run(&[
        "unpack",
        packed.to_str().unwrap(),
        "-o",
        restored.to_str().unwrap(),
        "--identity",
        recipient_secret.to_str().unwrap(),
    ]);
    assert!(ok, "unpack --identity failed: {}", err);

    assert_eq!(fs::read(&restored).unwrap(), data, "roundtrip mismatch");
}
