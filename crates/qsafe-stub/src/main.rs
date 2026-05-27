//! qsafe SFX stub binary — minimal extractor.
//!
//! 사용 흐름 (사용자 관점):
//!   1. `qsafe pack --sfx ... important.txt` 결과로 `important.txt.run` (Unix) 또는
//!      `important.txt.exe` (Windows) 같은 단일 실행파일을 받음.
//!   2. 실행 → password prompt → 자동으로 원본을 풀어 같은 디렉토리에 저장.
//!
//! 보안: payload는 여전히 정상 qsafe AEAD + BLAKE3 검증을 거치므로,
//! payload 부분 변조는 잡힌다. 단 stub 자체 변조는 막을 수 없는 게 SFX 본질이다.

use anyhow::{anyhow, bail, Context, Result};
use qsafe_core::compress::make_compressor;
use qsafe_core::envelope::decrypt_payload;
use qsafe_core::integrity::verify_blake3;
use qsafe_core::io::{read_packed_file, PackedFile};
use qsafe_crypto::unwrap_password;
use qsafe_stub::extract_payload;
use std::fs;
use std::path::PathBuf;
use zeroize::Zeroize;

fn main() {
    if let Err(e) = run() {
        eprintln!("✗ qsafe SFX 풀기 실패: {:#}", e);
        std::process::exit(2);
    }
}

fn run() -> Result<()> {
    eprintln!("qsafe self-extracting archive");
    eprintln!();

    let exe = std::env::current_exe().context("current_exe()")?;
    let file = fs::File::open(&exe).with_context(|| format!("open self {}", exe.display()))?;
    let payload = extract_payload(file).context("SFX payload 추출")?;

    // qsafe .qs 파일로 파싱
    let PackedFile {
        header,
        payload: ct,
        original_hash,
    } = read_packed_file(payload.as_slice()).context("parse qsafe payload")?;

    // password recipient 찾기 (SFX는 단순화: password 만 지원, FIDO2/Pubkey/Shamir는 일반 qsafe cli로 사용 권장)
    let pw_recipient = header
        .recipients
        .iter()
        .find_map(|r| match r {
            qsafe_core::format::Recipient::Password(pr) => Some(pr.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            anyhow!("SFX stub은 패스워드 수신자만 지원합니다. 일반 qsafe로 풀어주세요.")
        })?;

    eprintln!("📦 {} bytes (original)", header.original_size);
    if let Some(label) = &header.label {
        eprintln!("🏷️  {}", label);
    }
    eprintln!();

    // TTY면 hidden prompt, 아니면 stdin에서 한 줄 (CI / 자동화에서 유용).
    let pw = read_password_tty_or_stdin().context("패스워드 입력")?;
    let file_key =
        unwrap_password(&pw, &pw_recipient).map_err(|e| anyhow!("패스워드 오류: {}", e))?;
    let mut pw_z = pw;
    pw_z.zeroize();

    let compressed =
        decrypt_payload(&file_key, &header.payload_nonce, &ct).context("payload AEAD 복호화")?;
    drop(file_key);

    let compressor = make_compressor(header.compression).context("compressor")?;
    let plaintext = compressor
        .decompress(&compressed, Some(header.original_size as usize))
        .context("decompress")?;
    drop(compressed);

    if plaintext.len() as u64 != header.original_size {
        bail!(
            "크기 불일치 (header={}, actual={})",
            header.original_size,
            plaintext.len()
        );
    }
    verify_blake3(&plaintext, &original_hash).context("BLAKE3 검증")?;

    // 출력 위치: 같은 디렉토리에 stem 사용. 출력 파일 충돌 시 -1, -2 등 자동 추가.
    let out_path = pick_output_path(&exe)?;
    fs::write(&out_path, &plaintext)
        .with_context(|| format!("write output {}", out_path.display()))?;

    eprintln!("✓ {} 로 풀렸습니다.", out_path.display());
    Ok(())
}

/// TTY가 있으면 hidden prompt, 없으면 stdin에서 한 줄 (자동화/pipe 호환).
fn read_password_tty_or_stdin() -> Result<String> {
    use std::io::{BufRead, IsTerminal};
    if std::io::stdin().is_terminal() {
        rpassword::prompt_password("패스워드: ").map_err(anyhow::Error::from)
    } else {
        let mut s = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut s)
            .map_err(anyhow::Error::from)?;
        // trailing newline(들) 제거. \r\n 윈도우 호환.
        while s.ends_with('\n') || s.ends_with('\r') {
            s.pop();
        }
        if s.is_empty() {
            anyhow::bail!("stdin에서 빈 패스워드");
        }
        Ok(s)
    }
}

/// SFX 실행파일과 같은 디렉토리에 충돌 없는 출력 경로를 찾는다.
fn pick_output_path(exe: &std::path::Path) -> Result<PathBuf> {
    let dir = exe.parent().unwrap_or(std::path::Path::new("."));
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("qsafe-output");
    let stem = stem
        .strip_suffix(".run")
        .or_else(|| stem.strip_suffix(".exe"))
        .unwrap_or(stem);

    let candidate = dir.join(stem);
    if !candidate.exists() {
        return Ok(candidate);
    }
    for i in 1..1000 {
        let candidate = dir.join(format!("{}-{}", stem, i));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("출력 경로를 찾을 수 없습니다 (같은 이름 1000개 이상 존재)");
}
