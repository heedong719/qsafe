//! qsafe CLI — pack/unpack/info.
//!
//! v0.1 MVP: 단일 패스워드 수신자, zstd 압축 (자동 fallback), BLAKE3 무결성.
//! 향후: FIDO2/BIP39/Shamir 수신자, GUI, OS 통합, 보호 실행파일.

mod config;
mod credentials;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
#[allow(unused_imports)]
use credentials::{CredentialStore, StoredCredential};
use qsafe_core::compress::make_compressor;
use qsafe_core::envelope::{
    decrypt_payload, encrypt_payload, random_payload_nonce, random_stream_base_nonce, FileKey,
    STREAM_BASE_NONCE_LEN,
};
use qsafe_core::format::{
    ChunkInfo, CipherSuite, CompressionAlgo, Fido2Recipient, FileHeader, IntegrityAlgo, Recipient,
};
use qsafe_core::integrity::{blake3_hash, verify_blake3};
use qsafe_core::io::{read_packed_file, write_packed_file, PackedFile};
use qsafe_core::stream::{
    read_stream_header, stream_decrypt_with_hash, stream_encrypt_with_hash, write_stream_header,
    STREAM_THRESHOLD,
};
use qsafe_crypto::{unwrap_password, PasswordWrapper};
#[allow(unused_imports)]
use qsafe_hardware::{unwrap_fido2_with, Fido2Wrapper, PrfBackend};
use qsafe_paper::{display_words, GeneratedMnemonic};
use std::fs;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;
use zeroize::Zeroize;

/// qsafe — quantum-safe compression and encryption
#[derive(Parser, Debug)]
#[command(name = "qsafe", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,

    /// 자세한 로그 (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// 파일을 압축 + 암호화하여 .qs 파일 생성
    Pack {
        /// 입력 파일
        input: PathBuf,
        /// 출력 .qs 파일 (생략 시 <input>.qs)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 압축 알고리즘 (auto = 압축이 이득일 때만 적용)
        #[arg(short = 'c', long, value_enum, default_value_t = CompressArg::Auto)]
        compress: CompressArg,
        /// 보안 레벨 (Argon2id 매개변수)
        #[arg(long, value_enum, default_value_t = Profile::Standard)]
        profile: Profile,
        /// 패스워드 (스크립트용. 대화형 사용 시 비권장.)
        #[arg(long, hide = true)]
        password: Option<String>,
        /// 패스워드 수신자 사용 안 함 (--fido2와 함께 쓰면 FIDO2만)
        #[arg(long)]
        no_password: bool,
        /// FIDO2 수신자 추가. credential 이름 (`fido2 enroll`로 등록한 이름).
        /// 복수 지정 가능. 어느 하나로든 풀 수 있음 (OR 논리).
        #[arg(long)]
        fido2: Vec<String>,
        /// FIDO2 키 PIN (PIN 설정된 키 전용)
        #[arg(long, hide = true)]
        fido2_pin: Option<String>,
        /// 수신자의 public identity JSON 파일 — X25519+ML-KEM-768 하이브리드 봉투 추가.
        /// 복수 지정 가능 (OR 논리, 한 명에게만 풀어도 됨).
        #[arg(long)]
        pubkey: Vec<PathBuf>,
        /// 자기 압축 해제(SFX) 실행파일로 결과 묶기.
        /// 수신자가 받아 더블 클릭/실행만 하면 패스워드 입력 후 풀림.
        /// ⚠️ SFX는 신뢰 모델 위험: codesign / notarization 권장.
        #[arg(long)]
        sfx: bool,
        /// SFX stub 바이너리 경로 (생략 시 conventional: `target/release/qsafe-stub`).
        /// 다른 OS 타깃의 stub을 쓰려면 명시.
        #[arg(long, requires = "sfx")]
        sfx_stub: Option<PathBuf>,
        /// 사람이 읽을 라벨 (선택). 출력 파일명에는 영향 없음.
        #[arg(long)]
        label: Option<String>,
        /// 출력 파일이 이미 있어도 덮어씀
        #[arg(long)]
        force: bool,
        /// 원본 파일을 zero-fill 후 삭제 (SSD에서는 잔존 가능 — 보장 X)
        #[arg(long)]
        shred: bool,
    },
    /// .qs 파일을 복호화 + 압축 해제
    Unpack {
        /// 입력 .qs 파일
        input: PathBuf,
        /// 출력 파일 (생략 시 .qs 확장자 제거)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 패스워드 (스크립트용)
        #[arg(long, hide = true)]
        password: Option<String>,
        /// FIDO2로 풀기 (어느 FIDO2 수신자가 있으면 시도)
        #[arg(long)]
        fido2: bool,
        /// FIDO2 PIN
        #[arg(long, hide = true)]
        fido2_pin: Option<String>,
        /// 자신의 secret identity JSON 파일 — Pubkey recipient 풀기에 사용
        #[arg(long)]
        identity: Option<PathBuf>,
        /// 출력 파일이 이미 있어도 덮어씀
        #[arg(long)]
        force: bool,
    },
    /// .qs 파일 정보 조회 (복호화 없이 헤더만)
    Info {
        /// 입력 .qs 파일
        input: PathBuf,
    },
    /// FIDO2 하드웨어 키 관리
    Fido2 {
        #[command(subcommand)]
        cmd: Fido2Cmd,
    },
    /// BIP39 mnemonic (니모닉) 생성/검증 유틸리티
    Mnemonic {
        #[command(subcommand)]
        cmd: MnemonicCmd,
    },
    /// 외부 압축 포맷 풀기 (RAR/ZIP/7Z/TAR/GZ/XZ/BZ2/LZ4/ZSTD/BR 자동 감지)
    Extract {
        /// 입력 아카이브
        input: PathBuf,
        /// 출력 디렉토리 (생략 시 입력 stem)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 패스워드 (암호화된 RAR/ZIP/7Z)
        #[arg(long, hide = true)]
        password: Option<String>,
    },
    /// 외부 아카이브 내용 목록
    Ls {
        /// 입력 아카이브
        input: PathBuf,
        #[arg(long, hide = true)]
        password: Option<String>,
    },
    /// 환경 설정 (기본 패스워드 등)
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// 큰 .qs 파일을 N MB 단위로 분할 (이메일/USB 전송용)
    Split {
        /// 입력 .qs 파일
        input: PathBuf,
        /// part 크기 (예: 100M, 1G, 500K)
        #[arg(short, long)]
        size: String,
        /// 출력 prefix (생략 시 입력 파일명)
        #[arg(short, long)]
        output_prefix: Option<PathBuf>,
    },
    /// 분할된 .qs.partN 들을 다시 합치기
    Merge {
        /// 첫 part 파일 (.qs.part1 또는 .qs.part01)
        input: PathBuf,
        /// 합쳐진 출력 파일
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Shamir M-of-N 분산 백업 (가족/팀 분산)
    Shamir {
        #[command(subcommand)]
        cmd: ShamirCmd,
    },
    /// .qs 안의 실행파일을 디스크에 풀지 않고 메모리에서 직접 실행
    /// (Linux: memfd_create + execveat / macOS: secure temp + posix_spawn + 즉시 unlink)
    Run {
        /// 보호된 .qs 파일 (압축된 실행 파일)
        input: PathBuf,
        /// 패스워드
        #[arg(long, hide = true)]
        password: Option<String>,
        /// 자식 프로세스에 전달할 추가 인자
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// 옛 chainlock (.cl/.clk) 파일을 qsafe (.qs) 포맷으로 변환
    /// (또는 algorithm-upgrade — 새 cipher suite로 재암호화)
    Migrate {
        /// 입력 파일 (.cl, .clk, 또는 옛 .qs)
        input: PathBuf,
        /// 출력 .qs 파일
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 옛 파일 패스워드
        #[arg(long = "old-password", hide = true)]
        old_password: Option<String>,
        /// 새 패스워드 (기본: 옛 패스워드 그대로)
        #[arg(long = "new-password", hide = true)]
        new_password: Option<String>,
    },
    /// 내장 벤치마크 — 압축/암호화/해시 처리량 측정
    Bench {
        /// 측정 파일 크기 (MB)
        #[arg(short, long, default_value_t = 100)]
        size_mb: u32,
        /// 데이터 유형
        #[arg(short, long, value_enum, default_value_t = BenchData::Random)]
        data: BenchData,
    },
    /// X25519+ML-KEM-768 하이브리드 identity 키 관리
    Identity {
        #[command(subcommand)]
        cmd: IdentityCmd,
    },
    /// 파일 매니저 썸네일 생성 — Linux `.thumbnailer` 통합용 (XDG 표준).
    ///
    /// Nautilus / Dolphin / Thunar 등이 .qs 파일에 대해 자동 호출하는 hook.
    /// 입력이 valid qsafe 헤더면 우리 lock 아이콘을, 아니면 broken 아이콘을 출력.
    /// 크기 인자는 받아두지만 우리는 임베드된 256x256 PNG 를 그대로 씀
    /// (Nautilus 등이 표시 시 자동 resize).
    Thumbnail {
        /// 입력 .qs 파일
        input: PathBuf,
        /// 출력 PNG 경로
        output: PathBuf,
        /// 요청된 크기 (Nautilus 가 전달, 우리는 무시)
        #[arg(short, long, default_value_t = 256)]
        size: u32,
    },
}

#[derive(Subcommand, Debug)]
enum IdentityCmd {
    /// 새 identity 키쌍 생성 → JSON (secret + public) 저장
    Generate {
        /// 출력 파일 경로 (생략 시 ./qsafe-identity.json)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 같은 경로에 파일이 있어도 덮어씀
        #[arg(long)]
        force: bool,
    },
    /// 저장된 identity 파일의 fingerprint + 메타 출력
    Show {
        /// identity JSON 파일 (secret 또는 public 모두 허용)
        input: PathBuf,
    },
    /// secret identity → 공개키만 추출하여 별도 JSON 저장 (공유용)
    ExportPubkey {
        /// secret identity JSON 파일
        input: PathBuf,
        /// 출력 public identity 파일 (생략 시 `<input>.pub.json`)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// 같은 경로에 파일이 있어도 덮어씀
        #[arg(long)]
        force: bool,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum BenchData {
    /// 랜덤 (압축 안 됨)
    Random,
    /// 텍스트 (압축 잘 됨)
    Text,
    /// 제로 (극단)
    Zero,
}

#[derive(Subcommand, Debug)]
enum ShamirCmd {
    /// 비밀(임의 데이터)을 M-of-N share로 분할
    Split {
        /// 비밀 파일 (예: 패스워드, 키 등 임의 데이터)
        input: PathBuf,
        /// 임계값 M (최소 복구 share 수)
        #[arg(short = 'm', long)]
        threshold: u8,
        /// 총 share 수 N
        #[arg(short = 'n', long)]
        total: u8,
        /// 출력 디렉토리 (각 share를 share-01.txt 등으로)
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
    },
    /// share들을 모아 원본 복구
    Combine {
        /// share 파일들 (최소 M개)
        shares: Vec<PathBuf>,
        /// 출력 파일
        #[arg(short, long)]
        output: PathBuf,
    },
}

/// `config` 서브명령 — 모든 variant가 패스워드 관련이라 의도적으로 동일 postfix 사용.
#[allow(clippy::enum_variant_names)]
#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// 기본 패스워드 OS 키링에 저장 (pack 시 자동 사용)
    SetPassword,
    /// 기본 패스워드 설정 여부 확인
    ShowPassword,
    /// 기본 패스워드 삭제
    ClearPassword,
}

#[derive(Subcommand, Debug)]
enum MnemonicCmd {
    /// 새 BIP39 mnemonic 생성 (기본 24단어)
    Generate {
        /// 단어 개수 (12, 15, 18, 21, 24)
        #[arg(short = 'n', long, default_value_t = 24)]
        words: u8,
        /// 한 줄로 출력 (스크립트용). 기본은 번호 매긴 표 형식.
        #[arg(long)]
        oneline: bool,
        /// 추가로 추출되는 BIP39 seed (hex 64 bytes) 도 표시
        #[arg(long)]
        show_seed: bool,
        /// 추가로 추출되는 entropy (hex 16~32 bytes) 도 표시
        #[arg(long)]
        show_entropy: bool,
    },
    /// 입력한 mnemonic이 BIP39 체크섬 통과하는지 검증
    Verify {
        /// 24개 (또는 12/15/18/21) 단어, 공백 구분
        words: String,
    },
    /// 입력한 mnemonic의 정보 (단어수, language, fingerprint) 표시
    Info { words: String },
}

#[derive(Subcommand, Debug)]
enum Fido2Cmd {
    /// 연결된 FIDO2 키 확인
    Devices,
    /// 새 FIDO2 credential 등록
    Enroll {
        /// 식별 이름 (예: yubikey-main)
        name: String,
        /// PIN (필요 시)
        #[arg(long, hide = true)]
        pin: Option<String>,
    },
    /// 등록된 credential 목록
    List,
    /// credential 제거 (qsafe 측에서만 — 키 자체는 그대로)
    Forget { name: String },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CompressArg {
    /// 자동: 작은 파일은 압축 skip, 이득 있으면 zstd
    Auto,
    None,
    Zstd,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Profile {
    /// Argon2id m=64MiB, t=3, p=4 (~1초)
    Standard,
    /// Argon2id m=256MiB, t=4, p=4 (~3초)
    Strong,
}

/// 압축 임계값 — 이보다 작으면 압축 효과 의문, 봉투 오버헤드 + KDF 비용이 큼.
const COMPRESS_THRESHOLD_BYTES: usize = 256;

fn main() {
    let cli = Cli::parse();

    let level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| level.into());
    tracing_subscriber::fmt().with_env_filter(filter).init();

    if let Err(e) = run(cli.cmd) {
        eprintln!("error: {:#}", e);
        std::process::exit(1);
    }
}

fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Pack {
            input,
            output,
            compress,
            profile,
            password,
            no_password,
            fido2,
            fido2_pin,
            pubkey,
            sfx,
            sfx_stub,
            label,
            force,
            shred,
        } => cmd_pack(PackOptions {
            input,
            output,
            compress,
            profile,
            password,
            no_password,
            fido2,
            fido2_pin,
            pubkey,
            sfx,
            sfx_stub,
            label,
            force,
            shred,
        }),
        Cmd::Unpack {
            input,
            output,
            password,
            fido2,
            fido2_pin,
            identity,
            force,
        } => cmd_unpack(input, output, password, fido2, fido2_pin, identity, force),
        Cmd::Info { input } => cmd_info(input),
        Cmd::Fido2 { cmd } => cmd_fido2(cmd),
        Cmd::Mnemonic { cmd } => cmd_mnemonic(cmd),
        Cmd::Extract {
            input,
            output,
            password,
        } => cmd_extract(input, output, password),
        Cmd::Ls { input, password } => cmd_ls(input, password),
        Cmd::Config { cmd } => cmd_config(cmd),
        Cmd::Split {
            input,
            size,
            output_prefix,
        } => cmd_split(input, size, output_prefix),
        Cmd::Merge { input, output } => cmd_merge(input, output),
        Cmd::Shamir { cmd } => cmd_shamir(cmd),
        Cmd::Run {
            input,
            password,
            args,
        } => cmd_run(input, password, args),
        Cmd::Migrate {
            input,
            output,
            old_password,
            new_password,
        } => cmd_migrate(input, output, old_password, new_password),
        Cmd::Bench { size_mb, data } => cmd_bench(size_mb, data),
        Cmd::Identity { cmd } => cmd_identity(cmd),
        Cmd::Thumbnail {
            input,
            output,
            size,
        } => cmd_thumbnail(input, output, size),
    }
}

fn cmd_identity(cmd: IdentityCmd) -> Result<()> {
    match cmd {
        IdentityCmd::Generate { output, force } => cmd_identity_generate(output, force),
        IdentityCmd::Show { input } => cmd_identity_show(input),
        IdentityCmd::ExportPubkey {
            input,
            output,
            force,
        } => cmd_identity_export_pubkey(input, output, force),
    }
}

fn cmd_identity_generate(output: Option<PathBuf>, force: bool) -> Result<()> {
    use qsafe_identity::{Identity, IdentitySecretBytes};

    let output = output.unwrap_or_else(|| PathBuf::from("qsafe-identity.json"));
    refuse_overwrite_unless_force(&output, force)?;

    let identity = Identity::generate();
    let secret = IdentitySecretBytes::from_identity(&identity);
    let json = serde_json::to_vec_pretty(&secret)?;

    // 0600 — 다른 사용자 읽기 금지 (secret 포함)
    write_atomic(&output, |w| {
        use std::io::Write;
        w.write_all(&json).map_err(anyhow::Error::from)
    })?;

    println!("✓ identity 생성: {}", output.display());
    println!("  fingerprint  : {}", identity.fingerprint());
    println!("  x25519_pk    : {} bytes", identity.x25519_pk_bytes.len());
    println!(
        "  mlkem768_pk  : {} bytes (ML-KEM-768 hybrid)",
        identity.mlkem768_pk_bytes.len()
    );
    println!();
    println!("⚠️  이 파일은 secret 키를 포함합니다. 외부 공유 금지.");
    println!(
        "    공유용 공개키: `qsafe identity export-pubkey {}`",
        output.display()
    );
    Ok(())
}

fn cmd_identity_show(input: PathBuf) -> Result<()> {
    use qsafe_identity::{IdentityPublic, IdentitySecretBytes};

    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;

    // secret 먼저 시도 → public fallback (둘 다 호환)
    if let Ok(secret) = serde_json::from_slice::<IdentitySecretBytes>(&bytes) {
        let identity = secret
            .to_identity()
            .map_err(|e| anyhow!("invalid secret identity: {}", e))?;
        println!("qsafe identity (secret + public):");
        println!("  file         : {}", input.display());
        println!("  fingerprint  : {}", identity.fingerprint());
        println!("  x25519_pk    : {} bytes", identity.x25519_pk_bytes.len());
        println!(
            "  mlkem768_pk  : {} bytes",
            identity.mlkem768_pk_bytes.len()
        );
        return Ok(());
    }

    let public: IdentityPublic = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow!("not a valid qsafe identity JSON: {}", e))?;
    println!("qsafe identity (public only):");
    println!("  file         : {}", input.display());
    println!("  fingerprint  : {}", public.fingerprint());
    println!("  x25519_pk    : {} bytes", public.x25519_pk.len());
    println!("  mlkem768_pk  : {} bytes", public.mlkem768_pk.len());
    Ok(())
}

fn cmd_identity_export_pubkey(input: PathBuf, output: Option<PathBuf>, force: bool) -> Result<()> {
    use qsafe_identity::IdentitySecretBytes;

    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let secret: IdentitySecretBytes =
        serde_json::from_slice(&bytes).map_err(|e| anyhow!("not a secret identity JSON: {}", e))?;
    let identity = secret
        .to_identity()
        .map_err(|e| anyhow!("invalid secret identity: {}", e))?;
    let public = identity.public();

    let output = output.unwrap_or_else(|| {
        let mut p = input.clone();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("identity");
        p.set_file_name(format!("{}.pub.json", stem));
        p
    });
    refuse_overwrite_unless_force(&output, force)?;

    let json = serde_json::to_vec_pretty(&public)?;
    // public 키 자체는 비밀이 아니지만 일관성 위해 atomic write 사용.
    write_atomic(&output, |w| {
        use std::io::Write;
        w.write_all(&json).map_err(anyhow::Error::from)
    })?;
    println!(
        "✓ public identity 추출: {} → {}",
        input.display(),
        output.display()
    );
    println!("  fingerprint: {}", public.fingerprint());
    Ok(())
}

fn cmd_migrate(
    input: PathBuf,
    output: Option<PathBuf>,
    old_password: Option<String>,
    new_password: Option<String>,
) -> Result<()> {
    eprintln!("ℹ️  Migrate: 옛 포맷 → qsafe v0.1 (.qs)");

    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let PackedFile {
        header,
        payload,
        original_hash,
    } = read_packed_file(bytes.as_slice())?;

    eprintln!("  옛 버전: v{} ({:?})", header.version, header.suite);

    // 옛 패스워드로 복호화
    let file_key = try_unwrap_password(&header.recipients, old_password.clone())?;
    let compressed = decrypt_payload(&file_key, &header.payload_nonce, &payload)?;
    drop(file_key);

    let compressor = make_compressor(header.compression)?;
    let plaintext = compressor.decompress(&compressed, Some(header.original_size as usize))?;
    verify_blake3(&plaintext, &original_hash)?;

    eprintln!("  ✓ 옛 포맷 복호화 + 무결성 검증 완료");

    let output = output.unwrap_or_else(|| {
        let mut p = input.clone();
        let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("migrated");
        p.set_file_name(format!("{}.migrated.qs", name));
        p
    });
    refuse_overwrite_unless_force(&output, false)?;

    // 새 패스워드 = 명시되었으면 그것, 없으면 옛 것 그대로
    let new_pw = new_password.or(old_password).ok_or_else(|| {
        anyhow!("새 또는 옛 패스워드 중 하나 필요 (--new-password 또는 --old-password)")
    })?;

    let tmp_in = std::env::temp_dir().join(format!("qsafe-migrate-{}.tmp", std::process::id()));
    fs::write(&tmp_in, &plaintext)?;

    let result = cmd_pack(PackOptions {
        input: tmp_in.clone(),
        output: Some(output.clone()),
        compress: CompressArg::Auto,
        profile: Profile::Standard,
        password: Some(new_pw),
        no_password: false,
        fido2: Vec::new(),
        fido2_pin: None,
        pubkey: Vec::new(),
        sfx: false,
        sfx_stub: None,
        label: header.label,
        force: true,
        shred: false,
    });

    let _ = secure_delete(&tmp_in);
    drop(plaintext);

    result?;

    println!(
        "✓ 마이그레이션 완료: {} → {}",
        input.display(),
        output.display()
    );
    println!("  qsafe v0.1 표준 알고리즘으로 재암호화됨");
    Ok(())
}

fn cmd_bench(size_mb: u32, data: BenchData) -> Result<()> {
    use std::time::Instant;

    let size = (size_mb as usize) * 1024 * 1024;
    let plaintext = match data {
        BenchData::Random => {
            let mut buf = vec![0u8; size];
            rand_fill(&mut buf);
            buf
        }
        BenchData::Text => {
            let pattern = b"The quick brown fox jumps over the lazy dog. ";
            let mut buf = Vec::with_capacity(size);
            while buf.len() < size {
                buf.extend_from_slice(pattern);
            }
            buf.truncate(size);
            buf
        }
        BenchData::Zero => vec![0u8; size],
    };

    let data_type = match data {
        BenchData::Random => "random",
        BenchData::Text => "text",
        BenchData::Zero => "zeros",
    };

    println!("=== qsafe Bench ({} MB {}) ===", size_mb, data_type);
    println!();

    // BLAKE3
    let t = Instant::now();
    let hash = blake3_hash(&plaintext);
    let elapsed = t.elapsed();
    let throughput = size as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "  BLAKE3 hash     : {:>6.2} ms  ({:>7.1} MB/s)  {}",
        elapsed.as_secs_f64() * 1000.0,
        throughput,
        hex::encode(&hash[..8])
    );

    // zstd 압축 (멀티스레드)
    let t = Instant::now();
    let compressor = make_compressor(CompressionAlgo::Zstd)?;
    let compressed = compressor.compress(&plaintext)?;
    let elapsed = t.elapsed();
    let throughput = size as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
    let ratio = compressed.len() as f64 / size as f64 * 100.0;
    println!(
        "  zstd compress   : {:>6.2} ms  ({:>7.1} MB/s)  ratio: {:>5.1}%",
        elapsed.as_secs_f64() * 1000.0,
        throughput,
        ratio
    );

    // zstd 해제
    let t = Instant::now();
    let decompressed = compressor.decompress(&compressed, Some(size))?;
    let elapsed = t.elapsed();
    let throughput = size as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "  zstd decompress : {:>6.2} ms  ({:>7.1} MB/s)",
        elapsed.as_secs_f64() * 1000.0,
        throughput
    );
    assert_eq!(decompressed.len(), size);
    drop(decompressed);

    // AEAD batch
    let file_key = FileKey::random();
    let nonce = random_payload_nonce();
    let t = Instant::now();
    let ct = encrypt_payload(&file_key, &nonce, &compressed)?;
    let elapsed = t.elapsed();
    let throughput = compressed.len() as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "  XChaCha20 enc   : {:>6.2} ms  ({:>7.1} MB/s)  on compressed payload",
        elapsed.as_secs_f64() * 1000.0,
        throughput
    );

    let t = Instant::now();
    let pt = decrypt_payload(&file_key, &nonce, &ct)?;
    let elapsed = t.elapsed();
    let throughput = compressed.len() as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "  XChaCha20 dec   : {:>6.2} ms  ({:>7.1} MB/s)",
        elapsed.as_secs_f64() * 1000.0,
        throughput
    );
    assert_eq!(pt.len(), compressed.len());
    drop(pt);
    drop(ct);

    // Argon2id (default)
    use qsafe_crypto::PasswordWrapper;
    let t = Instant::now();
    let wrapper = PasswordWrapper::new("bench-pw");
    let _ = wrapper.wrap(&file_key);
    let elapsed = t.elapsed();
    println!(
        "  Argon2id 64MiB/3/4 (1회): {:>6.2} s  ({}회 시도 / 초)",
        elapsed.as_secs_f64(),
        (1.0 / elapsed.as_secs_f64()) as u32
    );

    println!();
    println!(
        "환경: {} cores",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    );

    Ok(())
}

fn rand_fill(buf: &mut [u8]) {
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(buf);
}

/// `.qs` 안의 실행파일을 디스크에 평문 풀지 않고 메모리/임시 fd 에서 실행.
fn cmd_run(input: PathBuf, password: Option<String>, args: Vec<String>) -> Result<()> {
    // 1. 보호된 파일 → 복호화 (메모리에)
    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let PackedFile {
        header,
        payload,
        original_hash,
    } = read_packed_file(bytes.as_slice())?;

    let pw_recipient = header
        .recipients
        .iter()
        .find_map(|r| match r {
            Recipient::Password(p) => Some(p.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("이 .qs 파일은 password 수신자가 없음"))?;

    let pw = match password {
        Some(p) => p,
        None => {
            if let Ok(Some(stored)) = config::load_default_password() {
                stored
            } else {
                rpassword::prompt_password("Password: ")?
            }
        }
    };
    if pw.is_empty() {
        bail!("empty password");
    }
    let file_key =
        unwrap_password(&pw, &pw_recipient).map_err(|e| anyhow!("패스워드 오류: {}", e))?;
    let mut pw_z = pw;
    pw_z.zeroize();

    let compressed = decrypt_payload(&file_key, &header.payload_nonce, &payload)?;
    drop(file_key);

    let compressor = make_compressor(header.compression)?;
    let plaintext = compressor.decompress(&compressed, Some(header.original_size as usize))?;
    drop(compressed);

    if plaintext.len() as u64 != header.original_size {
        bail!("size mismatch");
    }
    verify_blake3(&plaintext, &original_hash)?;

    // 2. 실행 (OS별)
    run_in_memory(&plaintext, &args)
}

#[cfg(target_os = "linux")]
fn run_in_memory(executable: &[u8], args: &[String]) -> Result<()> {
    eprintln!("ℹ️  Linux: memfd_create + execveat (디스크 거치지 않음)");

    // memfd_create 익명 메모리 fd 생성
    // SAFETY: libc 호출
    let name = std::ffi::CString::new("qsafe-mem-exec").unwrap();
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(anyhow!(
            "memfd_create 실패: {}",
            std::io::Error::last_os_error()
        ));
    }
    // fd 보호 RAII
    struct FdGuard(i32);
    impl Drop for FdGuard {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.0);
            }
        }
    }
    let _guard = FdGuard(fd);

    // 바이너리 fd에 write
    let mut remaining = executable;
    while !remaining.is_empty() {
        let written = unsafe {
            libc::write(
                fd,
                remaining.as_ptr() as *const libc::c_void,
                remaining.len(),
            )
        };
        if written < 0 {
            return Err(anyhow!(
                "write to memfd: {}",
                std::io::Error::last_os_error()
            ));
        }
        remaining = &remaining[written as usize..];
    }

    // 자식 프로세스 인자 준비. argv[0]는 관습적인 self-name.
    let mut argv: Vec<std::ffi::CString> = vec![std::ffi::CString::new("qsafe-exec")?];
    for a in args {
        argv.push(std::ffi::CString::new(a.as_str())?);
    }
    // c_char는 플랫폼별로 i8(x86_64)/u8(aarch64). libc::c_char 사용으로 ARM Linux 호환.
    let mut argv_ptrs: Vec<*const libc::c_char> = argv.iter().map(|c| c.as_ptr()).collect();
    argv_ptrs.push(std::ptr::null());

    // execveat(fd, "", argv, envp, AT_EMPTY_PATH)
    let empty_path = std::ffi::CString::new("")?;
    unsafe {
        libc::syscall(
            libc::SYS_execveat,
            fd,
            empty_path.as_ptr(),
            argv_ptrs.as_ptr(),
            std::ptr::null::<*const libc::c_char>(),
            libc::AT_EMPTY_PATH,
        );
    }
    // 도달 시 execveat 실패
    Err(anyhow!(
        "execveat 실패: {}",
        std::io::Error::last_os_error()
    ))
}

#[cfg(target_os = "macos")]
fn run_in_memory(executable: &[u8], args: &[String]) -> Result<()> {
    use std::io::Write;
    eprintln!("ℹ️  macOS: 보안 임시 파일 + posix_spawn + 즉시 unlink");

    // macOS는 memfd 없음 → 임시 디렉토리에 0700 디렉토리 + 0500 실행파일
    let tmp_dir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());
    // 시스템 시계가 UNIX_EPOCH 이전이면 0 nanos로 대체 (희박하지만 panic 회피).
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let unique = format!("{}/qsafe-exec-{}-{}", tmp_dir, std::process::id(), nanos);
    let exec_path = PathBuf::from(unique);

    // 0700 으로 임시 파일 생성
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o700); // owner-only rwx
        let mut f = opts
            .open(&exec_path)
            .with_context(|| format!("create exec file: {}", exec_path.display()))?;
        f.write_all(executable)?;
        f.sync_all()?;
    }

    // RAII guard — 종료 시 강제 삭제
    struct PathGuard(PathBuf);
    impl Drop for PathGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _guard = PathGuard(exec_path.clone());

    let status = std::process::Command::new(&exec_path)
        .args(args)
        .status()
        .with_context(|| format!("spawn: {}", exec_path.display()))?;

    // exit code 전달
    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_in_memory(executable: &[u8], args: &[String]) -> Result<()> {
    use std::io::Write;
    eprintln!("ℹ️  Windows: %TEMP% 보안 임시 파일 + CreateProcess + 즉시 삭제");

    let tmp_dir = std::env::var("TEMP")
        .or_else(|_| std::env::var("TMP"))
        .unwrap_or_else(|_| ".".into());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let unique = format!(
        "{}\\qsafe-exec-{}-{}.exe",
        tmp_dir,
        std::process::id(),
        nanos
    );
    let exec_path = PathBuf::from(unique);

    {
        let mut f = fs::File::create(&exec_path)
            .with_context(|| format!("create: {}", exec_path.display()))?;
        f.write_all(executable)?;
        f.sync_all()?;
    }

    struct PathGuard(PathBuf);
    impl Drop for PathGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _guard = PathGuard(exec_path.clone());

    let status = std::process::Command::new(&exec_path)
        .args(args)
        .status()
        .with_context(|| format!("spawn: {}", exec_path.display()))?;

    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn run_in_memory(_executable: &[u8], _args: &[String]) -> Result<()> {
    bail!("이 OS는 in-memory 실행 미지원");
}

fn cmd_shamir(cmd: ShamirCmd) -> Result<()> {
    use qsafe_shamir::{combine_secret, split_secret, EncodedShare};
    use std::str::FromStr;

    match cmd {
        ShamirCmd::Split {
            input,
            threshold,
            total,
            output_dir,
        } => {
            let secret = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
            if secret.is_empty() {
                bail!("입력 파일이 비어있음");
            }
            let shares =
                split_secret(&secret, threshold, total).map_err(|e| anyhow!("split: {}", e))?;

            let out_dir = output_dir.unwrap_or_else(|| {
                let mut p = input.clone();
                p.set_extension("shares");
                p
            });
            fs::create_dir_all(&out_dir)?;

            for (i, share) in shares.iter().enumerate() {
                let path = out_dir.join(format!("share-{:02}.txt", i + 1));
                let mut content = String::new();
                content.push_str(&format!(
                    "=== qsafe Shamir Share {} of {} ===\n",
                    i + 1,
                    total
                ));
                content.push_str(&format!("임계값: {} share 필요\n", threshold));
                content.push_str(&format!("이 share 번호: {}\n", share.index));
                content.push_str("\n⚠️  종이에 적어 안전한 곳에 보관하세요.\n");
                content.push_str("⚠️  파일로 저장 시 다른 share와 같은 곳에 두지 마세요.\n\n");
                content.push_str("--- 아래 한 줄이 share 데이터 ---\n");
                content.push_str(&share.to_string());
                content.push('\n');
                fs::write(&path, content.as_bytes())?;
                eprintln!("  ✓ {}", path.display());
            }
            let mut secret_z = secret;
            secret_z.zeroize();
            println!(
                "✓ {} 개 share 생성 → {} ({} 개 모이면 복구 가능)",
                total,
                out_dir.display(),
                threshold
            );
            println!();
            println!("⚠️  중요:");
            println!("   - 각 share를 다른 장소에 분산 보관 (집/은행/친구 등)");
            println!("   - {} 개 미만으로는 절대 복구 불가능", threshold);
            println!("   - share 파일 자체는 비밀 — 안전하게 다룰 것");
            Ok(())
        }
        ShamirCmd::Combine { shares, output } => {
            if shares.len() < 2 {
                bail!("최소 2개 share 필요");
            }
            refuse_overwrite_unless_force(&output, false)?;

            let mut parsed = Vec::with_capacity(shares.len());
            for path in &shares {
                let content =
                    fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
                // share 라인은 "qs1-"으로 시작
                let share_line = content
                    .lines()
                    .find(|l| l.trim().starts_with("qs1-"))
                    .ok_or_else(|| {
                        anyhow!("'{}' 에서 'qs1-' share 라인을 찾을 수 없음", path.display())
                    })?;
                let share = EncodedShare::from_str(share_line.trim())
                    .map_err(|e| anyhow!("'{}' 파싱: {}", path.display(), e))?;
                parsed.push(share);
            }

            let secret = combine_secret(&parsed).map_err(|e| anyhow!("combine: {}", e))?;

            write_atomic(&output, |w| {
                std::io::Write::write_all(w, &secret).map_err(anyhow::Error::from)
            })?;
            println!("✓ 복구됨 → {} ({} bytes)", output.display(), secret.len());
            let mut secret_z = secret;
            secret_z.zeroize();
            Ok(())
        }
    }
}

fn cmd_split(input: PathBuf, size: String, output_prefix: Option<PathBuf>) -> Result<()> {
    let part_size = parse_size(&size)?;
    if part_size < 1024 {
        bail!("part 크기는 최소 1 KB");
    }

    let total = fs::metadata(&input)?.len();
    let total_parts = total.div_ceil(part_size);
    if total_parts > 9999 {
        bail!(
            "part 수가 너무 많음 ({}): part 크기를 키우세요",
            total_parts
        );
    }

    let prefix = output_prefix.unwrap_or_else(|| input.clone());
    let total_parts_u32 = total_parts as u32;

    eprintln!(
        "총 {} 바이트 → {} 개 part로 분할 ({}/part)",
        total,
        total_parts,
        format_size(part_size)
    );

    let mut input_file = fs::File::open(&input)?;
    use std::io::{Read, Write};

    let mut buf = vec![0u8; 64 * 1024];
    for part_idx in 0..total_parts_u32 {
        let part_name = format!("{}.part{:03}", prefix.display(), part_idx + 1);
        let mut out = fs::File::create(&part_name)?;

        // POSIX 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = out.metadata()?.permissions();
            perms.set_mode(0o600);
            out.set_permissions(perms)?;
        }

        let mut remaining = part_size;
        while remaining > 0 {
            let n = remaining.min(buf.len() as u64) as usize;
            let read = input_file.read(&mut buf[..n])?;
            if read == 0 {
                break;
            }
            out.write_all(&buf[..read])?;
            remaining -= read as u64;
        }
        out.sync_all()?;
        eprintln!("  ✓ {}", part_name);
    }
    println!("✓ {} 개 part 생성", total_parts);
    Ok(())
}

fn cmd_merge(first_part: PathBuf, output: Option<PathBuf>) -> Result<()> {
    use std::io::{Read, Write};

    // 첫 part 파일명에서 .partNNN 추출
    let stem = first_part.to_str().ok_or_else(|| anyhow!("invalid path"))?;
    let part_pos = stem
        .rfind(".part")
        .ok_or_else(|| anyhow!("입력이 .partXXX 형식이 아님"))?;
    let base = &stem[..part_pos];
    let output = output.unwrap_or_else(|| PathBuf::from(base));

    refuse_overwrite_unless_force(&output, false)?;

    // 모든 part 찾기
    let mut parts: Vec<PathBuf> = (1..=9999)
        .map(|i| PathBuf::from(format!("{}.part{:03}", base, i)))
        .take_while(|p| p.exists())
        .collect();

    if parts.is_empty() {
        bail!("part 파일을 찾을 수 없음 (예상: {}.part001)", base);
    }
    parts.sort();

    eprintln!("합치는 중: {} 개 part → {}", parts.len(), output.display());

    let mut out = fs::File::create(&output)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = out.metadata()?.permissions();
        perms.set_mode(0o600);
        out.set_permissions(perms)?;
    }

    let mut buf = vec![0u8; 64 * 1024];
    for part_path in &parts {
        let mut part = fs::File::open(part_path)?;
        loop {
            let n = part.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
        }
    }
    out.sync_all()?;
    println!(
        "✓ {} 합쳐짐 ({} 바이트)",
        output.display(),
        fs::metadata(&output)?.len()
    );
    Ok(())
}

fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();
    let (num_str, mult) = if let Some(num) = s.strip_suffix('G') {
        (num, 1024u64 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix('M') {
        (num, 1024u64 * 1024)
    } else if let Some(num) = s.strip_suffix('K') {
        (num, 1024u64)
    } else {
        (s.as_str(), 1u64)
    };
    let n: u64 = num_str
        .parse()
        .map_err(|_| anyhow!("크기 파싱 실패: '{}' (예: 100M, 1G, 500K)", s))?;
    Ok(n * mult)
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn cmd_config(cmd: ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::SetPassword => {
            let pw = rpassword::prompt_password("새 기본 패스워드: ")?;
            if pw.is_empty() {
                bail!("빈 패스워드 거부");
            }
            let pw2 = rpassword::prompt_password("확인: ")?;
            if !constant_time_eq(pw.as_bytes(), pw2.as_bytes()) {
                bail!("패스워드가 일치하지 않음");
            }
            config::save_default_password(&pw)?;
            let mut pw_z = pw;
            pw_z.zeroize();
            let mut pw2_z = pw2;
            pw2_z.zeroize();
            println!("✓ 기본 패스워드가 OS 키링에 저장됨");
            println!("  이후 pack 시 --password 옵션 없이도 자동 사용됨");
            println!("  삭제하려면: qsafe config clear-password");
            Ok(())
        }
        ConfigCmd::ShowPassword => {
            if config::has_default_password() {
                println!("✓ 기본 패스워드 설정됨 (OS 키링)");
                println!("  pack 시 자동 사용");
                println!("  --password로 override 가능");
            } else {
                println!("× 기본 패스워드 미설정");
                println!("  설정하려면: qsafe config set-password");
            }
            Ok(())
        }
        ConfigCmd::ClearPassword => {
            config::clear_default_password()?;
            println!("✓ 기본 패스워드 삭제됨");
            Ok(())
        }
    }
}

fn cmd_extract(input: PathBuf, output: Option<PathBuf>, password: Option<String>) -> Result<()> {
    use qsafe_formats::{detect_format, ExternalFormat};
    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let format = detect_format(&bytes);
    drop(bytes);

    eprintln!("감지된 포맷: {}", format.name());
    if !format.can_extract() {
        bail!("지원하지 않는 포맷");
    }

    let out_dir = output.unwrap_or_else(|| {
        let stem = input
            .file_stem()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("extracted"));
        stem
    });
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("디렉토리 생성: {}", out_dir.display()))?;

    let pw = password.as_deref();
    let count =
        match format {
            ExternalFormat::Rar => qsafe_formats::rar::extract_rar(&input, &out_dir, pw)
                .map_err(|e| anyhow!("RAR: {}", e))?,
            ExternalFormat::Zip => qsafe_formats::zipformat::extract_zip(&input, &out_dir, pw)
                .map_err(|e| anyhow!("ZIP: {}", e))?,
            ExternalFormat::Gzip => qsafe_formats::gz::extract_gz(&input, &out_dir, pw)
                .map_err(|e| anyhow!("GZ: {}", e))?,
            ExternalFormat::Xz => qsafe_formats::xz::extract_xz(&input, &out_dir, pw)
                .map_err(|e| anyhow!("XZ: {}", e))?,
            ExternalFormat::Bzip2 => qsafe_formats::bz2::extract_bz2(&input, &out_dir, pw)
                .map_err(|e| anyhow!("BZ2: {}", e))?,
            ExternalFormat::Lz4Frame => qsafe_formats::lz4::extract_lz4(&input, &out_dir, pw)
                .map_err(|e| anyhow!("LZ4: {}", e))?,
            ExternalFormat::SevenZ => qsafe_formats::sevenz::extract_7z(&input, &out_dir, pw)
                .map_err(|e| anyhow!("7Z: {}", e))?,
            ExternalFormat::Tar => qsafe_formats::tar_fmt::extract_tar(&input, &out_dir, pw)
                .map_err(|e| anyhow!("TAR: {}", e))?,
            ExternalFormat::Zstd => qsafe_formats::zstd_fmt::extract_zstd(&input, &out_dir, pw)
                .map_err(|e| anyhow!("ZSTD: {}", e))?,
            ExternalFormat::Qsafe => bail!("'.qs' 파일은 `qsafe unpack`을 사용하세요"),
            _ => bail!("이 포맷 추출 미구현: {}", format.name()),
        };
    println!("✓ {} 파일 추출 → {}", count, out_dir.display());
    Ok(())
}

fn cmd_ls(input: PathBuf, password: Option<String>) -> Result<()> {
    use qsafe_formats::{detect_format, ExternalFormat};
    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let format = detect_format(&bytes);
    drop(bytes);

    println!("{}: {}", input.display(), format.name());

    match format {
        ExternalFormat::Rar => {
            let entries = qsafe_formats::rar::list_rar(&input, password.as_deref())
                .map_err(|e| anyhow!("RAR: {}", e))?;
            println!("총 {}개 엔트리:", entries.len());
            for e in &entries {
                let kind = if e.is_directory {
                    "DIR"
                } else if e.is_encrypted {
                    "ENC"
                } else {
                    "FILE"
                };
                println!("  {:>12}  {:>4}  {}", e.unpacked_size, kind, e.filename);
            }
        }
        _ => println!("(이 포맷의 ls는 아직 미구현)"),
    }
    Ok(())
}

fn cmd_mnemonic(cmd: MnemonicCmd) -> Result<()> {
    match cmd {
        MnemonicCmd::Generate {
            words,
            oneline,
            show_seed,
            show_entropy,
        } => mnemonic_generate(words, oneline, show_seed, show_entropy),
        MnemonicCmd::Verify { words } => mnemonic_verify(&words),
        MnemonicCmd::Info { words } => mnemonic_info(&words),
    }
}

fn mnemonic_generate(words: u8, oneline: bool, show_seed: bool, show_entropy: bool) -> Result<()> {
    let m = GeneratedMnemonic::random(words).map_err(|e| anyhow!("mnemonic 생성 실패: {}", e))?;
    let word_list = m.words();

    if oneline {
        println!("{}", word_list.join(" "));
    } else {
        eprintln!("⚠️  아래 단어들을 종이에 적어 안전한 곳에 보관하세요.");
        eprintln!("    화면 캡처/디지털 저장은 매우 위험합니다.");
        eprintln!();
        println!("{}", display_words(&word_list));
        eprintln!();
        eprintln!("총 {} 단어 (영어, BIP39 표준)", word_list.len());
    }

    if show_entropy || show_seed {
        // bip39 라이브러리로 다시 파싱하여 추가 정보 추출
        use bip39::{Language, Mnemonic};
        let joined = word_list.join(" ");
        let mnemonic = Mnemonic::parse_in(Language::English, &joined)
            .map_err(|e: bip39::Error| anyhow!("내부 mnemonic 파싱 실패: {}", e))?;

        if show_entropy {
            let entropy = mnemonic.to_entropy();
            eprintln!(
                "entropy ({} bytes): {}",
                entropy.len(),
                hex::encode(&entropy)
            );
        }
        if show_seed {
            let seed = mnemonic.to_seed("");
            eprintln!("seed (64 bytes, passphrase=\"\"): {}", hex::encode(seed));
        }
    }

    Ok(())
}

fn mnemonic_verify(words: &str) -> Result<()> {
    use bip39::{Language, Mnemonic};
    match Mnemonic::parse_in(Language::English, words.trim()) {
        Ok(m) => {
            println!("✓ 유효한 BIP39 mnemonic ({} 단어)", m.words().count());
            Ok(())
        }
        Err(e) => {
            bail!("✗ 유효하지 않은 mnemonic: {}", e);
        }
    }
}

fn mnemonic_info(words: &str) -> Result<()> {
    use bip39::{Language, Mnemonic};
    let mnemonic = Mnemonic::parse_in(Language::English, words.trim())
        .map_err(|e: bip39::Error| anyhow!("파싱 실패: {}", e))?;
    let count = mnemonic.words().count();
    let entropy = mnemonic.to_entropy();
    let entropy_bits = entropy.len() * 8;

    println!("BIP39 Mnemonic 정보:");
    println!("  단어 수      : {}", count);
    println!("  언어         : English");
    println!(
        "  entropy 크기 : {} bits ({} bytes)",
        entropy_bits,
        entropy.len()
    );
    println!("  체크섬       : 통과 ✓");

    // 짧은 fingerprint (entropy의 BLAKE3 해시 앞 8 bytes)
    let hash = qsafe_core::integrity::blake3_hash(&entropy);
    println!("  fingerprint  : {}", hex::encode(&hash[..8]));

    Ok(())
}

struct PackOptions {
    input: PathBuf,
    output: Option<PathBuf>,
    compress: CompressArg,
    profile: Profile,
    password: Option<String>,
    no_password: bool,
    fido2: Vec<String>,
    fido2_pin: Option<String>,
    pubkey: Vec<PathBuf>,
    sfx: bool,
    sfx_stub: Option<PathBuf>,
    label: Option<String>,
    force: bool,
    shred: bool,
}

fn cmd_pack(opts: PackOptions) -> Result<()> {
    let PackOptions {
        input,
        output,
        compress,
        profile,
        password,
        no_password,
        fido2,
        fido2_pin,
        pubkey,
        sfx,
        sfx_stub,
        label,
        force,
        shred,
    } = opts;

    if no_password && fido2.is_empty() && pubkey.is_empty() {
        bail!("적어도 하나의 수신자가 필요합니다 (패스워드, --fido2, 또는 --pubkey)");
    }

    // SFX는 patient stub 구조상 password recipient만 지원.
    if sfx && no_password {
        bail!("--sfx 는 패스워드 수신자가 필요합니다. --no-password와 함께 쓸 수 없음.");
    }

    let input_meta =
        fs::metadata(&input).with_context(|| format!("cannot stat {}", input.display()))?;
    if !input_meta.is_file() {
        bail!("입력이 일반 파일이 아닙니다: {}", input.display());
    }

    let output = output.unwrap_or_else(|| {
        let mut p = input.clone();
        let new_name = format!(
            "{}.qs",
            input
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("output")
        );
        p.set_file_name(new_name);
        p
    });

    paths_must_differ(&input, &output)?;
    refuse_overwrite_unless_force(&output, force)?;

    // 대용량 파일 → streaming 모드 (메모리 효율적)
    if input_meta.len() >= STREAM_THRESHOLD {
        eprintln!(
            "ℹ️  파일 크기 {} ≥ {} MB → 스트리밍 모드 (메모리 ~10MB만 사용)",
            format_size_pack(input_meta.len()),
            STREAM_THRESHOLD / (1024 * 1024)
        );
        return cmd_pack_streaming(
            input,
            output,
            input_meta.len(),
            profile,
            password,
            no_password,
            fido2,
            fido2_pin,
            label,
            shred,
        );
    }

    // 1. 입력 읽기 (TODO: 큰 파일은 스트리밍)
    tracing::info!(path = %input.display(), size = input_meta.len(), "reading input");
    let plaintext = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let original_size = plaintext.len() as u64;
    let original_hash = blake3_hash(&plaintext);

    // 2. 압축 (auto fallback) — 메모리 효율을 위해 두 단계 분기
    let chosen_algo = decide_compression(compress, plaintext.len());
    let (compressed, final_algo) = if chosen_algo == CompressionAlgo::None {
        (plaintext, CompressionAlgo::None)
    } else {
        let compressor = make_compressor(chosen_algo)?;
        tracing::debug!(algo = ?chosen_algo, "compressing");
        let result = compressor.compress(&plaintext)?;
        // Auto 모드에서 압축이 역효과면 원본 사용 (메모리 두 배 피함)
        if matches!(compress, CompressArg::Auto) && result.len() >= plaintext.len() {
            tracing::debug!("compression did not help; using None");
            (plaintext, CompressionAlgo::None)
        } else {
            drop(plaintext);
            (result, chosen_algo)
        }
    };

    // 3. FileKey 생성 + 페이로드 암호화
    let file_key = FileKey::random();
    let payload_nonce = random_payload_nonce();
    tracing::debug!("encrypting payload");
    let ciphertext = encrypt_payload(&file_key, &payload_nonce, &compressed)?;
    drop(compressed);

    // 4. 수신자들 — 다중 수신자 (OR 논리: 어느 하나만 충족돼도 풀림)
    let mut recipients: Vec<Recipient> = Vec::new();

    // 4a. 패스워드 수신자 (기본, --no-password 없으면 추가)
    if !no_password {
        let pw = read_password_for_pack(password)?;
        tracing::debug!(profile = ?profile, "wrapping file key with password");
        let wrapper = match profile {
            Profile::Standard => PasswordWrapper::new(&pw),
            Profile::Strong => PasswordWrapper::strong(&pw),
        };
        let r = wrapper
            .wrap(&file_key)
            .map_err(|e| anyhow!("password wrap: {}", e))?;
        recipients.push(r);
        drop(wrapper);
        let mut pw_z = pw;
        pw_z.zeroize();
    }

    // 4b. FIDO2 수신자들
    if !fido2.is_empty() {
        let recipients_added = wrap_fido2_recipients(&fido2, &fido2_pin, &file_key)?;
        recipients.extend(recipients_added);
    }

    // 4c. Pubkey 수신자들 (X25519+ML-KEM-768 하이브리드)
    for pk_path in &pubkey {
        let recipient = wrap_pubkey_recipient(pk_path, &file_key)?;
        recipients.push(recipient);
    }

    drop(file_key);

    if recipients.is_empty() {
        bail!("internal: no recipients constructed");
    }

    // 5. 헤더
    let mut header = FileHeader::new(CipherSuite::V1Xchacha20Blake3, final_algo);
    header.integrity = IntegrityAlgo::Blake3;
    header.recipients = recipients;
    header.payload_nonce = payload_nonce;
    header.original_size = original_size;
    header.created_at_unix = chrono::Utc::now().timestamp();
    header.label = label;

    // 6. SFX 모드면 stub binary + qs payload를 합쳐 단일 실행파일로 작성.
    //    아니면 일반 .qs 파일로 직접 atomic write.
    if sfx {
        let stub_path = resolve_sfx_stub(sfx_stub.as_deref())?;
        let stub_bytes = fs::read(&stub_path)
            .with_context(|| format!("SFX stub 읽기: {}", stub_path.display()))?;

        // qs 페이로드를 메모리 버퍼에 빌드
        let mut qs_buf: Vec<u8> = Vec::new();
        write_packed_file(&mut qs_buf, &header, &ciphertext, &original_hash)
            .map_err(anyhow::Error::from)?;

        let sfx_bytes = qsafe_stub::assemble_sfx(&stub_bytes, &qs_buf);

        // 출력 경로가 명시 안 됐으면 OS 별 기본 확장자
        let output = sfx_default_output(&input, &output);
        paths_must_differ(&input, &output)?;
        refuse_overwrite_unless_force(&output, force)?;

        write_atomic(&output, |w| {
            use std::io::Write;
            w.write_all(&sfx_bytes).map_err(anyhow::Error::from)
        })?;

        // 실행 비트 + 0755 (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&output)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&output, perms)?;
        }

        let out_size = fs::metadata(&output)?.len();
        println!("✓ packed (SFX) {} → {}", input.display(), output.display());
        println!(
            "  {} bytes → {} bytes (stub {} + qs {} + footer 16)",
            original_size,
            out_size,
            stub_bytes.len(),
            qs_buf.len()
        );
        eprintln!(
            "⚠️  SFX는 unsigned 실행파일입니다. macOS Gatekeeper / Windows SmartScreen 차단 가능."
        );

        if shred {
            eprintln!("⚠️  --shred: 베스트 에포트 (SSD/copy-on-write 파일시스템에서는 잔존 가능)");
            secure_delete(&input)?;
            println!("  shredded original: {}", input.display());
        }
        return Ok(());
    }

    // 6'. 일반 모드: atomic write
    write_atomic(&output, |w| {
        write_packed_file(w, &header, &ciphertext, &original_hash).map_err(anyhow::Error::from)
    })?;

    let out_size = fs::metadata(&output)?.len();
    let ratio = if original_size == 0 {
        0.0
    } else {
        100.0 * out_size as f64 / original_size as f64
    };
    println!("✓ packed {} → {}", input.display(), output.display());
    println!(
        "  {} bytes → {} bytes ({:.1}% of original)",
        original_size, out_size, ratio
    );

    if shred {
        eprintln!("⚠️  --shred: 베스트 에포트 (SSD/copy-on-write 파일시스템에서는 잔존 가능)");
        secure_delete(&input)?;
        println!("  shredded original: {}", input.display());
    }

    Ok(())
}

fn format_size_pack(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes / 1024)
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_unpack_streaming(
    input: PathBuf,
    output: Option<PathBuf>,
    password: Option<String>,
    _use_fido2: bool,
    _fido2_pin: Option<String>,
    force: bool,
) -> Result<()> {
    use std::io::BufReader;

    eprintln!("ℹ️  스트리밍 모드로 해제 (메모리 ~10MB만 사용)");

    let input_file = fs::File::open(&input)?;
    let mut reader = BufReader::with_capacity(64 * 1024, input_file);
    let header = read_stream_header(&mut reader)?;
    let chunks = header
        .chunks
        .clone()
        .ok_or_else(|| anyhow!("not streaming"))?;

    // payload_nonce에서 base_nonce 추출
    if header.payload_nonce.len() < STREAM_BASE_NONCE_LEN {
        bail!("payload_nonce too short for streaming");
    }
    let mut base_nonce = [0u8; STREAM_BASE_NONCE_LEN];
    base_nonce.copy_from_slice(&header.payload_nonce[..STREAM_BASE_NONCE_LEN]);

    // password 수신자 (MVP)
    let pw_recipient = header
        .recipients
        .iter()
        .find_map(|r| match r {
            Recipient::Password(p) => Some(p.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("password recipient 없음"))?;

    let pw = match password {
        Some(p) => p,
        None => {
            if let Ok(Some(stored)) = config::load_default_password() {
                stored
            } else {
                rpassword::prompt_password("Password: ")?
            }
        }
    };
    let file_key =
        unwrap_password(&pw, &pw_recipient).map_err(|e| anyhow!("패스워드 오류: {}", e))?;
    let mut pw_z = pw;
    pw_z.zeroize();

    let output = output.unwrap_or_else(|| safe_default_output(&input));
    paths_must_differ(&input, &output)?;
    refuse_overwrite_unless_force(&output, force)?;

    let mut hasher = qsafe_core::envelope::stream_integrity_hasher(&file_key);

    write_atomic(&output, |out| {
        stream_decrypt_with_hash(
            &mut reader,
            &mut *out,
            &file_key,
            &base_nonce,
            chunks.num_chunks,
            &mut hasher,
        )
        .map_err(anyhow::Error::from)
    })?;

    // 무결성 검증: trailing hash 32 bytes
    let mut trailing_hash = [0u8; 32];
    use std::io::Read;
    reader.read_exact(&mut trailing_hash)?;
    let computed = hasher.finalize();
    if computed.as_bytes() != &trailing_hash {
        bail!("BLAKE3 hash 검증 실패 (파일 변조 의심)");
    }

    println!(
        "✓ unpacked (streaming) {} → {} ({} 청크, {} bytes)",
        input.display(),
        output.display(),
        chunks.num_chunks,
        header.original_size
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_pack_streaming(
    input: PathBuf,
    output: PathBuf,
    input_size: u64,
    profile: Profile,
    password: Option<String>,
    no_password: bool,
    fido2: Vec<String>,
    fido2_pin: Option<String>,
    label: Option<String>,
    shred: bool,
) -> Result<()> {
    use std::io::BufReader;

    // FileKey 생성
    let file_key = FileKey::random();
    let base_nonce = random_stream_base_nonce();

    // payload_nonce는 base_nonce 20 bytes + zeros 4 bytes로 채움 (헤더 호환)
    let mut payload_nonce_24 = vec![0u8; 24];
    payload_nonce_24[..STREAM_BASE_NONCE_LEN].copy_from_slice(&base_nonce);

    // 수신자 봉투화
    let mut recipients: Vec<Recipient> = Vec::new();
    if !no_password {
        let pw = read_password_for_pack(password)?;
        let wrapper = match profile {
            Profile::Standard => PasswordWrapper::new(&pw),
            Profile::Strong => PasswordWrapper::strong(&pw),
        };
        let r = wrapper
            .wrap(&file_key)
            .map_err(|e| anyhow!("password wrap: {}", e))?;
        recipients.push(r);
        let mut pw_z = pw;
        pw_z.zeroize();
    }
    if !fido2.is_empty() {
        let r = wrap_fido2_recipients(&fido2, &fido2_pin, &file_key)?;
        recipients.extend(r);
    }

    // 청크 수 사전 계산
    let chunk_size = qsafe_core::envelope::STREAM_CHUNK_SIZE as u64;
    let num_chunks = input_size.div_ceil(chunk_size).max(1) as u32;
    let last_chunk_size = if input_size == 0 {
        0
    } else if input_size % chunk_size == 0 {
        chunk_size as u32
    } else {
        (input_size % chunk_size) as u32
    };

    let mut header = FileHeader::new(CipherSuite::V1Xchacha20Blake3, CompressionAlgo::None);
    header.integrity = IntegrityAlgo::Blake3;
    header.recipients = recipients;
    header.payload_nonce = payload_nonce_24;
    header.original_size = input_size;
    header.created_at_unix = chrono::Utc::now().timestamp();
    header.label = label;
    header.chunks = Some(ChunkInfo {
        chunk_size: chunk_size as u32,
        num_chunks,
        last_chunk_size,
    });

    write_atomic(&output, |out| {
        use std::io::Write;
        write_stream_header(&mut *out, &header).map_err(anyhow::Error::from)?;

        let input_file = fs::File::open(&input)?;
        let mut reader = BufReader::with_capacity(64 * 1024, input_file);

        let mut hasher = qsafe_core::envelope::stream_integrity_hasher(&file_key);
        let (n_chunks, last_size, total) =
            stream_encrypt_with_hash(&mut reader, &mut *out, &file_key, &base_nonce, &mut hasher)
                .map_err(|e| anyhow!("stream encrypt: {}", e))?;

        if n_chunks != num_chunks {
            bail!("청크 수 불일치 ({} vs 계산 {})", n_chunks, num_chunks);
        }
        if last_size != last_chunk_size {
            bail!("마지막 청크 크기 불일치");
        }
        if total != input_size {
            bail!("총 byte 불일치 ({} vs {})", total, input_size);
        }

        // BLAKE3 hash 32 bytes
        let hash = hasher.finalize();
        out.write_all(hash.as_bytes())?;
        Ok(())
    })?;

    drop(file_key);
    let out_size = fs::metadata(&output)?.len();
    println!(
        "✓ packed (streaming) {} → {}",
        input.display(),
        output.display()
    );
    println!(
        "  {} bytes → {} bytes ({} 청크)",
        input_size, out_size, num_chunks
    );

    if shred {
        eprintln!("⚠️  --shred: 베스트 에포트 (SSD 잔존 가능)");
        secure_delete(&input)?;
        println!("  shredded original");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_unpack(
    input: PathBuf,
    output: Option<PathBuf>,
    password: Option<String>,
    use_fido2: bool,
    fido2_pin: Option<String>,
    identity: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    let input_meta =
        fs::metadata(&input).with_context(|| format!("cannot stat {}", input.display()))?;
    if !input_meta.is_file() {
        bail!("입력이 일반 파일이 아닙니다: {}", input.display());
    }

    // 헤더만 읽어서 streaming 모드인지 확인
    {
        use std::io::BufReader;
        let probe_file = fs::File::open(&input)?;
        let mut probe_reader = BufReader::with_capacity(64 * 1024, probe_file);
        if let Ok(h) = read_stream_header(&mut probe_reader) {
            if h.chunks.is_some() {
                drop(probe_reader);
                return cmd_unpack_streaming(input, output, password, use_fido2, fido2_pin, force);
            }
        }
    }

    // 1. 파일 읽기 + 헤더 파싱 (batch 모드)
    tracing::debug!(path = %input.display(), "reading qsafe file (batch)");
    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let PackedFile {
        header,
        payload,
        original_hash,
    } = read_packed_file(bytes.as_slice())?;

    // 2. FileKey 복원 — identity 우선 → FIDO2 → 패스워드 (사용자가 명시한 우선순위 따름)
    let file_key = if let Some(id_path) = identity.as_ref() {
        try_unwrap_identity(&header.recipients, id_path)?
    } else if use_fido2 {
        try_unwrap_fido2(&header.recipients, fido2_pin.as_deref())?
    } else {
        try_unwrap_password(&header.recipients, password)?
    };

    // 4. 페이로드 복호화
    tracing::debug!("decrypting payload");
    let compressed = decrypt_payload(&file_key, &header.payload_nonce, &payload)?;
    drop(file_key);

    // 5. 압축 해제
    let compressor = make_compressor(header.compression)?;
    tracing::debug!(algo = ?header.compression, "decompressing");
    let plaintext = compressor.decompress(&compressed, Some(header.original_size as usize))?;
    drop(compressed);

    // 6. 크기 + 해시 검증
    if plaintext.len() as u64 != header.original_size {
        bail!(
            "복호화 크기 불일치 (header={}, actual={})",
            header.original_size,
            plaintext.len()
        );
    }
    verify_blake3(&plaintext, &original_hash)?;

    // 7. 출력 경로 — 헤더 라벨 신뢰 X, .qs 확장자 제거만
    let output = output.unwrap_or_else(|| safe_default_output(&input));

    paths_must_differ(&input, &output)?;
    refuse_overwrite_unless_force(&output, force)?;

    // 8. atomic write
    write_atomic(&output, |w| {
        std::io::Write::write_all(w, &plaintext).map_err(anyhow::Error::from)
    })?;

    println!(
        "✓ unpacked {} → {} ({} bytes)",
        input.display(),
        output.display(),
        header.original_size
    );

    // 9. 라벨 정보 출력만 (파일명 영향 X)
    if let Some(ref label) = header.label {
        // ANSI escape 등 제어 문자 제거 후 표시 (악의적 헤더 방어)
        println!("  original label: {}", sanitize_for_terminal(label));
    }

    Ok(())
}

fn cmd_info(input: PathBuf) -> Result<()> {
    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let pf = read_packed_file(bytes.as_slice())?;
    let h = &pf.header;

    println!("qsafe file: {}", input.display());
    println!("  format version : {}", h.version);
    println!("  cipher suite   : {:?}", h.suite);
    println!("  compression    : {:?}", h.compression);
    println!("  integrity      : {:?}", h.integrity);
    println!("  original size  : {} bytes", h.original_size);
    println!("  payload size   : {} bytes", pf.payload.len());
    if h.original_size > 0 {
        let ratio = 100.0 * pf.payload.len() as f64 / h.original_size as f64;
        println!("  payload ratio  : {:.1}% of original", ratio);
    }
    println!(
        "  created at     : {}",
        chrono::DateTime::<chrono::Utc>::from_timestamp(h.created_at_unix, 0)
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "unknown".into())
    );
    if let Some(ref label) = h.label {
        // 악의적 ANSI escape 차단
        println!("  label          : {}", sanitize_for_terminal(label));
    }
    println!("  recipients     : {}", h.recipients.len());
    for (i, r) in h.recipients.iter().enumerate() {
        let kind = match r {
            Recipient::Password(p) => format!(
                "password (Argon2id m={}KiB t={} p={}, XChaCha20-Poly1305)",
                p.argon2_m_kib, p.argon2_t, p.argon2_p
            ),
            Recipient::Fido2(f) => {
                let lbl = sanitize_for_terminal(f.label.as_deref().unwrap_or("unnamed"));
                let rp = sanitize_for_terminal(&f.rp_id);
                format!(
                    "fido2 (hmac-secret, rp={}, label={}, uv={})",
                    rp, lbl, f.user_verification_required
                )
            }
            Recipient::Bip39(b) => {
                let lbl = sanitize_for_terminal(b.label.as_deref().unwrap_or("unnamed"));
                let lang = sanitize_for_terminal(&b.language);
                format!(
                    "bip39 paper ({} words, {}, label={})",
                    b.word_count, lang, lbl
                )
            }
            Recipient::Timelock(_) => "timelock (drand)".into(),
            Recipient::Pubkey(_) => "pubkey (X25519+MLKEM)".into(),
            Recipient::ShamirCommitment(_) => "shamir (M-of-N)".into(),
        };
        println!("    [{}] {}", i, kind);
    }

    Ok(())
}

// ───────────── 수신자 wrap/unwrap 헬퍼 ─────────────────────────

/// 헤더에서 Password 수신자 찾아 FileKey 복원.
fn try_unwrap_password(recipients: &[Recipient], password: Option<String>) -> Result<FileKey> {
    let pw_recipient = recipients
        .iter()
        .find_map(|r| match r {
            Recipient::Password(p) => Some(p.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            anyhow!("이 파일은 password 수신자가 없습니다. --fido2 옵션을 사용해 보세요")
        })?;

    let pw = match password {
        Some(p) => p,
        None => {
            // OS 키링 자동 시도
            if let Ok(Some(stored)) = config::load_default_password() {
                if !stored.is_empty() {
                    eprintln!("ℹ️  OS 키링의 기본 패스워드 시도");
                    stored
                } else {
                    rpassword::prompt_password("Password: ").context("read password")?
                }
            } else {
                rpassword::prompt_password("Password: ").context("read password")?
            }
        }
    };
    if pw.is_empty() {
        bail!("empty password");
    }
    let result = unwrap_password(&pw, &pw_recipient)
        .map_err(|e| anyhow!("패스워드가 틀렸거나 파일이 손상되었습니다 (raw: {})", e));
    let mut pw_z = pw;
    pw_z.zeroize();
    result
}

/// 헤더의 모든 Fido2 수신자를 순회하며 첫 성공한 것으로 FileKey 복원.
fn try_unwrap_fido2(recipients: &[Recipient], pin: Option<&str>) -> Result<FileKey> {
    let fido2_recipients: Vec<&Fido2Recipient> = recipients
        .iter()
        .filter_map(|r| match r {
            Recipient::Fido2(f) => Some(f),
            _ => None,
        })
        .collect();

    if fido2_recipients.is_empty() {
        bail!("이 파일은 FIDO2 수신자가 없습니다");
    }

    #[cfg(feature = "fido2-hw")]
    {
        use qsafe_hardware::hw::Fido2HwBackend;
        for (idx, recipient) in fido2_recipients.iter().enumerate() {
            let label_disp = recipient.label.as_deref().unwrap_or("");
            eprintln!(
                "[{}/{}] FIDO2 키를 꽂고 Touch 하세요 ({})",
                idx + 1,
                fido2_recipients.len(),
                label_disp
            );
            let mut backend = Fido2HwBackend::new(&recipient.rp_id);
            if let Some(p) = pin {
                backend = backend.with_pin(p);
            }
            match unwrap_fido2_with(&backend, recipient) {
                Ok(fk) => {
                    tracing::info!("FIDO2 unwrap 성공 ({})", label_disp);
                    return Ok(fk);
                }
                Err(e) => {
                    tracing::warn!("FIDO2 시도 {} 실패: {}", idx + 1, e);
                    continue;
                }
            }
        }
        bail!("모든 FIDO2 수신자 복호화 실패")
    }
    #[cfg(not(feature = "fido2-hw"))]
    {
        let _ = (fido2_recipients, pin);
        bail!("이 빌드는 FIDO2 하드웨어 지원이 없습니다. `cargo build --features fido2-hw`로 재빌드 필요")
    }
}

/// pack 시 FIDO2 수신자들 생성.
fn wrap_fido2_recipients(
    names: &[String],
    pin: &Option<String>,
    file_key: &FileKey,
) -> Result<Vec<Recipient>> {
    #[cfg(feature = "fido2-hw")]
    {
        use qsafe_hardware::hw::Fido2HwBackend;
        let store = CredentialStore::load()?;
        let mut out = Vec::with_capacity(names.len());

        for name in names {
            let cred = store.find(name).ok_or_else(|| {
                anyhow!(
                    "등록되지 않은 FIDO2 credential: '{}'. `qsafe fido2 enroll {} <name>` 먼저",
                    name,
                    name
                )
            })?;
            let credential_id =
                hex::decode(&cred.credential_id_hex).context("credential_id hex decode")?;

            eprintln!("FIDO2 키 ({})에 Touch 하세요...", name);
            let mut backend = Fido2HwBackend::new(&cred.rp_id);
            if let Some(p) = pin {
                backend = backend.with_pin(p);
            }
            let wrapper = Fido2Wrapper::new(&backend, credential_id)
                .with_rp_id(cred.rp_id.clone())
                .with_label(cred.name.clone());
            let r = wrapper
                .wrap(file_key)
                .map_err(|e| anyhow!("FIDO2 wrap 실패 ({}): {}", name, e))?;
            out.push(r);
        }
        Ok(out)
    }
    #[cfg(not(feature = "fido2-hw"))]
    {
        let _ = (names, pin, file_key);
        bail!("이 빌드는 FIDO2 하드웨어 지원이 없습니다. `cargo build --features fido2-hw`로 재빌드 필요")
    }
}

/// 한 명의 수신자 public identity JSON 파일 → Pubkey Recipient.
/// X25519 + ML-KEM-768 하이브리드 봉투.
fn wrap_pubkey_recipient(pk_path: &Path, file_key: &FileKey) -> Result<Recipient> {
    let bytes =
        fs::read(pk_path).with_context(|| format!("read public identity {}", pk_path.display()))?;
    let public: qsafe_identity::IdentityPublic = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse public identity JSON: {}", pk_path.display()))?;
    let wrapper = qsafe_identity::PubkeyWrapper::new(public);
    wrapper
        .wrap(file_key)
        .map_err(|e| anyhow!("pubkey wrap ({}): {}", pk_path.display(), e))
}

/// secret identity 파일로 Pubkey Recipient 풀기.
/// 헤더의 Pubkey recipients 중 우리 mlkem_pk_hash와 일치하는 첫 항목으로 시도.
fn try_unwrap_identity(recipients: &[Recipient], identity_path: &Path) -> Result<FileKey> {
    let bytes = fs::read(identity_path)
        .with_context(|| format!("read identity {}", identity_path.display()))?;
    let secret: qsafe_identity::IdentitySecretBytes = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse secret identity: {}", identity_path.display()))?;
    let identity = secret
        .to_identity()
        .map_err(|e| anyhow!("identity 복원 실패: {}", e))?;

    let mut last_err: Option<anyhow::Error> = None;
    for r in recipients {
        if let Recipient::Pubkey(pr) = r {
            match qsafe_identity::unwrap_pubkey(&identity, pr) {
                Ok(fk) => return Ok(fk),
                Err(e) => last_err = Some(anyhow!("pubkey unwrap: {}", e)),
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("Pubkey recipient를 가진 파일이 아닙니다")))
}

fn cmd_fido2(cmd: Fido2Cmd) -> Result<()> {
    match cmd {
        Fido2Cmd::Devices => cmd_fido2_devices(),
        Fido2Cmd::Enroll { name, pin } => cmd_fido2_enroll(name, pin),
        Fido2Cmd::List => cmd_fido2_list(),
        Fido2Cmd::Forget { name } => cmd_fido2_forget(name),
    }
}

fn cmd_fido2_devices() -> Result<()> {
    #[cfg(feature = "fido2-hw")]
    {
        use qsafe_hardware::hw::Fido2HwBackend;
        let n = Fido2HwBackend::device_count();
        println!("연결된 FIDO2 키: {} 개", n);
        Ok(())
    }
    #[cfg(not(feature = "fido2-hw"))]
    {
        bail!("이 빌드는 FIDO2 지원이 없습니다. `cargo build --features fido2-hw`")
    }
}

fn cmd_fido2_enroll(name: String, pin: Option<String>) -> Result<()> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        bail!("유효하지 않은 이름: '{}'", name);
    }

    #[cfg(feature = "fido2-hw")]
    {
        use qsafe_hardware::hw::Fido2HwBackend;
        use qsafe_hardware::DEFAULT_RP_ID;

        let mut store = CredentialStore::load()?;
        if store.find(&name).is_some() {
            bail!(
                "이미 존재하는 이름: '{}'. 먼저 `fido2 forget {}` 해주세요.",
                name,
                name
            );
        }

        eprintln!("FIDO2 키를 꽂고 Touch 하세요 (등록)...");
        let mut backend = Fido2HwBackend::new(DEFAULT_RP_ID);
        if let Some(p) = pin {
            backend = backend.with_pin(p);
        }
        let enrolled = backend
            .enroll(Some(&name))
            .map_err(|e| anyhow!("enroll 실패: {}", e))?;

        let cred = StoredCredential {
            name: name.clone(),
            credential_id_hex: hex::encode(&enrolled.credential_id),
            rp_id: DEFAULT_RP_ID.into(),
            created_at_unix: chrono::Utc::now().timestamp(),
            label: enrolled.label,
        };
        store.add(cred)?;
        store.save()?;
        println!("✓ FIDO2 credential 등록 완료: {}", name);
        Ok(())
    }
    #[cfg(not(feature = "fido2-hw"))]
    {
        let _ = (name, pin);
        bail!("이 빌드는 FIDO2 지원이 없습니다. `cargo build --features fido2-hw`")
    }
}

fn cmd_fido2_list() -> Result<()> {
    let store = CredentialStore::load()?;
    if store.credentials.is_empty() {
        println!("등록된 FIDO2 credential 없음. `qsafe fido2 enroll <name>` 먼저.");
        return Ok(());
    }
    println!("등록된 FIDO2 credentials:");
    for c in &store.credentials {
        let created = chrono::DateTime::<chrono::Utc>::from_timestamp(c.created_at_unix, 0)
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "unknown".into());
        println!(
            "  {}  rp={}  created={}  id_short={}...",
            c.name,
            c.rp_id,
            created,
            &c.credential_id_hex[..c.credential_id_hex.len().min(16)]
        );
    }
    Ok(())
}

fn cmd_fido2_forget(name: String) -> Result<()> {
    let mut store = CredentialStore::load()?;
    store.remove(&name)?;
    store.save()?;
    println!("✓ qsafe에서 제거됨: {} (실 키에는 영향 X)", name);
    Ok(())
}

// ───────────── 헬퍼 ────────────────────────────────────────────

fn decide_compression(arg: CompressArg, input_len: usize) -> CompressionAlgo {
    match arg {
        CompressArg::None => CompressionAlgo::None,
        CompressArg::Zstd => CompressionAlgo::Zstd,
        CompressArg::Auto => {
            if input_len < COMPRESS_THRESHOLD_BYTES {
                CompressionAlgo::None
            } else {
                CompressionAlgo::Zstd
            }
        }
    }
}

fn read_password_for_pack(provided: Option<String>) -> Result<String> {
    if let Some(p) = provided {
        if p.is_empty() {
            bail!("empty password");
        }
        eprintln!("⚠️  --password 옵션 사용: 패스워드가 셸 히스토리에 남을 수 있습니다");
        return Ok(p);
    }
    // OS 키링의 기본 패스워드 자동 사용
    if let Ok(Some(pw)) = config::load_default_password() {
        if !pw.is_empty() {
            eprintln!("ℹ️  OS 키링의 기본 패스워드 사용 (override: --password)");
            return Ok(pw);
        }
    }
    let pw = rpassword::prompt_password("Password: ").context("read password")?;
    if pw.is_empty() {
        bail!("empty password");
    }
    let pw2 = rpassword::prompt_password("Confirm password: ").context("read confirm")?;
    if !constant_time_eq(pw.as_bytes(), pw2.as_bytes()) {
        bail!("passwords do not match");
    }
    drop(pw2);
    Ok(pw)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// .qs 확장자만 제거한 안전한 기본 출력 경로. **헤더 라벨 신뢰 X**.
/// SFX 모드의 기본 출력 경로 — 명시 안 했으면 OS 별 확장자 자동.
fn sfx_default_output(input: &Path, explicit: &Path) -> PathBuf {
    if explicit != input {
        // 호출자가 이미 default `.qs`를 자동 생성했더라도 SFX 모드에선 다시 결정.
        // explicit이 input과 다르면 사용자 지정으로 간주하고 그대로 사용.
        // (cmd_pack 진입 시 output unwrap이 항상 일어나므로 input과 비교가 안전.)
        if !explicit.to_string_lossy().ends_with(".qs") {
            return explicit.to_path_buf();
        }
    }
    let mut p = input.to_path_buf();
    let name = input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output");
    #[cfg(target_os = "windows")]
    let new_name = format!("{}.exe", name);
    #[cfg(not(target_os = "windows"))]
    let new_name = format!("{}.run", name);
    p.set_file_name(new_name);
    p
}

/// SFX stub binary 경로를 결정한다.
/// 우선순위: (1) --sfx-stub 명시 → (2) 환경변수 QSAFE_STUB_BIN → (3) CARGO_TARGET_DIR/release/qsafe-stub
/// → (4) ./target/release/qsafe-stub → (5) ~/.cargo/bin/qsafe-stub
fn resolve_sfx_stub(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        if !p.exists() {
            bail!("--sfx-stub 경로가 없습니다: {}", p.display());
        }
        return Ok(p.to_path_buf());
    }
    if let Ok(env_path) = std::env::var("QSAFE_STUB_BIN") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Ok(p);
        }
    }
    let candidates = [
        std::env::var("CARGO_TARGET_DIR")
            .ok()
            .map(|d| PathBuf::from(d).join("release").join(stub_bin_name())),
        Some(PathBuf::from("target/release").join(stub_bin_name())),
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".cargo/bin").join(stub_bin_name())),
    ];
    for c in candidates.into_iter().flatten() {
        if c.exists() {
            return Ok(c);
        }
    }
    bail!(
        "SFX stub binary를 찾을 수 없습니다. `cargo build --release -p qsafe-stub` 후 재시도하거나 \
         --sfx-stub <PATH> 또는 QSAFE_STUB_BIN 환경변수를 사용하세요."
    )
}

fn stub_bin_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "qsafe-stub.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "qsafe-stub"
    }
}

fn safe_default_output(input: &Path) -> PathBuf {
    let mut p = input.to_path_buf();
    let ext = p.extension().and_then(|e| e.to_str());
    if ext == Some("qs") || ext == Some("cl") {
        // .qs (현재) + .cl (v0.1 호환)
        p.set_extension("");
    } else {
        let stem = input.file_name().and_then(|n| n.to_str()).unwrap_or("out");
        p.set_file_name(format!("{}.unpacked", stem));
    }
    p
}

/// 두 경로가 같은 파일을 가리키면 거부 (심볼릭 링크/상대 경로 대응).
fn paths_must_differ(a: &Path, b: &Path) -> Result<()> {
    let ac = canonical_or_self(a);
    let bc = canonical_or_self(b);
    if ac == bc {
        bail!("입력과 출력이 같은 파일을 가리킵니다: {}", a.display());
    }
    Ok(())
}

fn canonical_or_self(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| {
        // 파일이 아직 없을 수 있음 — parent + filename으로 fallback
        if let (Some(parent), Some(name)) = (p.parent(), p.file_name()) {
            let parent_c = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
            parent_c.join(name)
        } else {
            p.to_path_buf()
        }
    })
}

/// 출력 파일이 이미 있고 --force 없으면 거부.
fn refuse_overwrite_unless_force(output: &Path, force: bool) -> Result<()> {
    if output.exists() && !force {
        bail!(
            "출력 파일이 이미 존재합니다: {}\n  --force로 덮어쓰거나 다른 경로를 지정하세요.",
            output.display()
        );
    }
    Ok(())
}

/// 실패/panic 시 임시 파일을 자동 정리하는 RAII 가드.
struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }
    /// rename 성공 시 호출 — guard가 파일을 지우지 않게 함.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            // 베스트 에포트, 실패 무시
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// 임시 파일 + fsync + rename. **TOCTOU 방어 + 0600 권한**.
///
/// 보안 강화:
/// - `create_new(true)`: 임시파일 이미 존재 시 거부 (TOCTOU 차단)
/// - `O_NOFOLLOW` (POSIX): 심볼릭 링크 따라가지 않음 (TOCTOU 차단)
/// - `0o600` mode (POSIX): 다른 사용자 읽기 금지
/// - panic / 에러 시 임시파일 자동 cleanup (RAII)
fn write_atomic<F>(path: &Path, write_fn: F) -> Result<()>
where
    F: FnOnce(&mut fs::File) -> Result<()>,
{
    let parent = path.parent().ok_or_else(|| anyhow!("path has no parent"))?;
    let parent_for_create = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    fs::create_dir_all(parent_for_create)
        .with_context(|| format!("ensure dir {}", parent_for_create.display()))?;

    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("path has no file name"))?
        .to_string_lossy()
        .to_string();
    let tmp = parent_for_create.join(format!(".{}.tmp.{}", file_name, std::process::id()));

    let guard = TempFileGuard::new(tmp.clone());

    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
            // O_NOFOLLOW: 심볼릭 링크 따라가지 않음
            // O_NOFOLLOW = 0x100 on Linux, 0x100 on macOS (다르지만 libc 사용 X)
            // std::os::unix::fs::OpenOptionsExt::custom_flags는 libc::O_NOFOLLOW를 받음
            #[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
            {
                // O_NOFOLLOW 직접 값 (POSIX, 호환 안전)
                #[cfg(target_os = "linux")]
                const O_NOFOLLOW: i32 = 0o400000;
                #[cfg(target_os = "macos")]
                const O_NOFOLLOW: i32 = 0x100;
                #[cfg(target_os = "freebsd")]
                const O_NOFOLLOW: i32 = 0x100;
                opts.custom_flags(O_NOFOLLOW);
            }
        }

        let mut f = opts
            .open(&tmp)
            .with_context(|| format!("create temp file {}", tmp.display()))?;
        write_fn(&mut f)?;
        f.sync_all()
            .with_context(|| format!("fsync {}", tmp.display()))?;
    }

    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    guard.disarm();
    Ok(())
}

/// 사용자에게 표시할 라벨에서 ANSI escape + 제어 문자 제거.
/// 악의적 .qs 파일이 터미널 hijack 시도하는 것 방지.
fn sanitize_for_terminal(s: &str) -> String {
    s.chars()
        // 인쇄 가능 문자 + 정상 공백/한글/이모지는 허용. 제어 문자(\x00-\x1F, \x7F)는 제거,
        // 단 \t/\n/공백은 정보용으로 보존. ANSI escape(\x1b)·bell(\x07)·null 등은 차단.
        .filter(|c| *c == '\n' || *c == '\t' || *c == ' ' || !c.is_control())
        .take(512) // 길이 제한 — 거대 라벨 방어
        .collect()
}

/// 베스트 에포트 secure delete. SSD에서는 보장 X.
fn secure_delete(path: &Path) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let len = fs::metadata(path)?.len();
    let mut f = fs::OpenOptions::new().write(true).open(path)?;
    f.seek(SeekFrom::Start(0))?;

    let zeros = vec![0u8; 64 * 1024];
    let mut remaining = len;
    while remaining > 0 {
        let n = remaining.min(zeros.len() as u64) as usize;
        f.write_all(&zeros[..n])?;
        remaining -= n as u64;
    }
    f.sync_all()?;
    drop(f);
    fs::remove_file(path)?;
    Ok(())
}

/// 256x256 lock 아이콘 — qsafe-gui/icons/icon.png 와 동일한 자산.
/// Linux 파일 매니저(Nautilus/Dolphin/Thunar)가 `qsafe thumbnail` hook을 부를 때 사용.
const THUMBNAIL_ICON_PNG: &[u8] = include_bytes!("../../qsafe-gui/icons/icon.png");

fn cmd_thumbnail(input: PathBuf, output: PathBuf, _size: u32) -> Result<()> {
    // 입력이 valid qsafe 헤더인지 magic bytes로 검증 (전체 디코드는 비싸고 불필요).
    // 헤더가 valid 면 lock 아이콘을, 아니면 에러로 종료해서 파일 매니저가 generic 아이콘으로 fallback 하게 함.
    let mut buf = [0u8; 8];
    let mut f = fs::File::open(&input).with_context(|| format!("open {}", input.display()))?;
    use std::io::Read as _;
    f.read_exact(&mut buf)
        .with_context(|| format!("read magic from {}", input.display()))?;
    if &buf != qsafe_core::format::MAGIC {
        anyhow::bail!(
            "유효한 qsafe 파일 아님 (magic 불일치) — 파일 매니저가 generic 아이콘으로 fallback"
        );
    }

    // 아이콘을 그대로 출력에 write (Linux .thumbnailer는 PNG의 정확한 size를 요구하지 않음.
    // Nautilus / Dolphin 등이 표시 시 자동 resize).
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("output path 에 디렉토리 없음"))?;
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all {}", parent.display()))?;
    }
    fs::write(&output, THUMBNAIL_ICON_PNG)
        .with_context(|| format!("write {}", output.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_rejects_non_qsafe() {
        let mut tmp_in = std::env::temp_dir();
        tmp_in.push(format!("qsafe-thumb-test-in-{}.bin", std::process::id()));
        std::fs::write(&tmp_in, b"not a qsafe file at all").unwrap();
        let mut tmp_out = std::env::temp_dir();
        tmp_out.push(format!("qsafe-thumb-test-out-{}.png", std::process::id()));
        let r = cmd_thumbnail(tmp_in.clone(), tmp_out.clone(), 256);
        assert!(r.is_err());
        assert!(!tmp_out.exists(), "fail 시 output 가 생성되어선 안 됨");
        let _ = std::fs::remove_file(&tmp_in);
    }

    #[test]
    fn thumbnail_writes_png_for_valid_qsafe() {
        // 최소 valid 헤더: MAGIC 8B + 더미 — cmd_thumbnail은 magic만 검사함
        let mut tmp_in = std::env::temp_dir();
        tmp_in.push(format!("qsafe-thumb-valid-{}.qs", std::process::id()));
        let mut content = qsafe_core::format::MAGIC.to_vec();
        content.extend_from_slice(&[0u8; 32]); // 패딩
        std::fs::write(&tmp_in, &content).unwrap();

        let mut tmp_out = std::env::temp_dir();
        tmp_out.push(format!("qsafe-thumb-valid-out-{}.png", std::process::id()));

        let r = cmd_thumbnail(tmp_in.clone(), tmp_out.clone(), 256);
        assert!(r.is_ok(), "{:?}", r);

        let bytes = std::fs::read(&tmp_out).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG 시그니처");
        assert!(bytes.len() > 100, "icon 데이터 가 너무 작음");

        let _ = std::fs::remove_file(&tmp_in);
        let _ = std::fs::remove_file(&tmp_out);
    }

    #[test]
    fn sanitize_strips_escape_codes() {
        // ANSI escape 시퀀스
        let dangerous = "normal\x1b[2J\x1b[Hhidden";
        let clean = sanitize_for_terminal(dangerous);
        assert!(!clean.contains('\x1b'));
        assert!(clean.contains("normal"));
        assert!(clean.contains("hidden"));
    }

    #[test]
    fn sanitize_preserves_korean() {
        let s = "내 중요 문서 📄";
        let clean = sanitize_for_terminal(s);
        assert_eq!(clean, s);
    }

    #[test]
    fn sanitize_removes_null_bytes() {
        let s = "hello\x00world\x07";
        let clean = sanitize_for_terminal(s);
        assert_eq!(clean, "helloworld");
    }

    #[test]
    fn sanitize_length_limit() {
        let huge = "a".repeat(10_000);
        let clean = sanitize_for_terminal(&huge);
        assert!(clean.len() <= 512);
    }

    #[test]
    fn sanitize_keeps_tabs_newlines() {
        let s = "line1\nline2\tcol";
        let clean = sanitize_for_terminal(s);
        assert!(clean.contains('\n'));
        assert!(clean.contains('\t'));
    }

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn decide_compression_auto() {
        assert_eq!(
            decide_compression(CompressArg::Auto, 10),
            CompressionAlgo::None
        );
        assert_eq!(
            decide_compression(CompressArg::Auto, 10_000),
            CompressionAlgo::Zstd
        );
        assert_eq!(
            decide_compression(CompressArg::None, 10_000),
            CompressionAlgo::None
        );
        assert_eq!(
            decide_compression(CompressArg::Zstd, 10),
            CompressionAlgo::Zstd
        );
    }

    #[test]
    fn safe_default_output_strips_clk() {
        assert_eq!(
            safe_default_output(Path::new("foo.txt.qs")),
            PathBuf::from("foo.txt")
        );
        // v0.1 호환: .cl도 인식
        assert_eq!(
            safe_default_output(Path::new("foo.txt.cl")),
            PathBuf::from("foo.txt")
        );
        assert_eq!(
            safe_default_output(Path::new("foo.bin")),
            PathBuf::from("foo.bin.unpacked")
        );
    }
}
