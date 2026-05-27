# qsafe — Quantum-Safe Compression & Encryption

> **양자 시대를 견디는 압축 + 암호화 도구.**
> 하드웨어 키, 종이 백업, 다중 수신자 봉투. 블록체인 의존 0%, 외부 서비스 의존 0%.

## ⚡ 한눈에 보기

```bash
# 압축 + 암호화 (자동)
qsafe pack myfile.pdf

# 풀기
qsafe unpack myfile.pdf.qs

# 반디집/7-Zip 호환 — 외부 압축 포맷도 풀기
qsafe extract foo.zip
qsafe extract foo.rar       # ← RAR도 가능!
qsafe extract foo.tar.xz
qsafe extract foo.7z

# BIP39 종이 백업
qsafe mnemonic generate

# X25519 + ML-KEM-768 하이브리드 공개키 (PQ-안전, v0.1.5+)
qsafe identity generate -o me.json                 # 나의 키쌍
qsafe identity export-pubkey me.json               # 공유용 공개키 추출
qsafe pack secret.pdf --pubkey friend.pub.json     # 친구만 풀 수 있음
qsafe unpack secret.pdf.qs --identity me.json      # 내 secret 키로 풀기

# 자기압축해제(SFX) 실행파일 (v0.1.5+, 패스워드 수신자 필요)
qsafe pack secret.pdf --sfx --password mypw        # → secret.pdf.run (1.3MB 자체 추출 실행파일)
./secret.pdf.run                                   # 받은 사람이 더블 클릭만으로 풀기
```

> ⚠️ SFX 출력은 unsigned 실행파일이라 macOS Gatekeeper / Windows SmartScreen이 차단할 수 있습니다. 배포 전 codesign / notarization 권장.

## ✨ 차별점

| 기능 | qsafe | 7-Zip | WinRAR | age | Bandizip |
|---|---|---|---|---|---|
| 강 KDF (Argon2id) | ✅ | ❌ | ❌ | ✅ scrypt | ❌ |
| AEAD (XChaCha20-Poly1305) | ✅ | ⚠️ | ✅ AES | ✅ | ⚠️ |
| FIDO2 하드웨어 키 | ✅ | ❌ | ❌ | ❌ | ❌ |
| BIP39 종이 백업 | ✅ | ❌ | ❌ | ❌ | ❌ |
| 다중 수신자 봉투 (OR) | ✅ | ❌ | ❌ | ✅ | ❌ |
| Post-Quantum (ML-KEM) | 🟡 진행 | ❌ | ❌ | ⚠️ -pq | ❌ |
| RAR 풀기 | ✅ | ✅ | ✅ | ❌ | ✅ |
| ZIP / 7Z / TAR.* | ✅ | ✅ | ✅ | ❌ | ✅ |
| 가격 | $0 | $0 | $29 | $0 | $0/$29 Pro |
| 오픈소스 + 감사 가능 | ✅ | ✅ | ❌ | ✅ | ❌ |
| 블록체인 의존 0 | ✅ | ✅ | ✅ | ✅ | ✅ |

## 🔐 보안 모델

### 압호화
- **Argon2id** KDF (RFC 9106)
- **XChaCha20-Poly1305** AEAD (192-bit nonce)
- **BLAKE3** 무결성
- **HKDF-SHA256** 도메인 분리

### 보호
- 압축 폭탄 방어 (ratio limit)
- TOCTOU 방어 (O_NOFOLLOW + create_new)
- 0600 파일 권한
- ANSI escape sanitize
- 헤더 라벨 신뢰 X
- Zeroize 보장

### 양자 안전성
- 현재: 패스워드 + 종이 BIP39 (양자컴 영향 X)
- 향후: X25519 + ML-KEM-768 하이브리드 공개키

## 📦 지원 포맷 (반디집 호환)

### 풀기 + 만들기 (양방향)
- **qsafe (.qs)** — 우리 표준
- **ZIP** (.zip)
- **TAR** (.tar)
- **GZIP** (.gz, .tgz)
- **LZ4** (.lz4)
- **Brotli** (.br)

### 풀기만 가능
- **RAR / RAR5** — unrar (라이센스 제약)
- **7Z** (.7z) — pure Rust!
- **XZ / LZMA** (.xz, .lzma)
- **BZIP2** (.bz2)
- **Zstd** (.zst)

## 🚀 빠른 시작

### 설치

```bash
# Cargo 로
cargo install --git https://github.com/heedong719/qsafe qsafe-cli

# 빌드 (소스에서)
git clone https://github.com/heedong719/qsafe
cd qsafe
cargo build --release
./target/release/qsafe --help
```

### 첫 파일 보호

```bash
$ qsafe pack important.pdf
Password: ********
Confirm password: ********
✓ packed important.pdf → important.pdf.qs
  1048576 bytes → 524288 bytes (50.0% of original)
```

### 정보 보기 (복호화 없이)

```bash
$ qsafe info important.pdf.qs
qsafe file: important.pdf.qs
  format version : 2
  cipher suite   : V1Xchacha20Blake3
  compression    : Zstd
  integrity      : Blake3
  recipients     : 1
    [0] password (Argon2id m=65536KiB t=3 p=4, XChaCha20-Poly1305)
```

### FIDO2 하드웨어 키

```bash
# 키 등록 (한 번만)
$ qsafe fido2 enroll yubikey-main

# 키로 보호
$ qsafe pack secret.txt --fido2 yubikey-main

# 키로 풀기
$ qsafe unpack secret.txt.qs --fido2
```

### 외부 포맷 풀기

```bash
$ qsafe extract some.rar -o output/
감지된 포맷: rar
✓ 5 파일 추출 → output/
```

## 🏗️ 아키텍처

```
qsafe-core         ← 봉투 + 압축 + 무결성
qsafe-crypto       ← Argon2id + ChaCha20 + Password 수신자
qsafe-hardware     ← FIDO2 PRF (옵션)
qsafe-paper        ← BIP39 24단어
qsafe-formats      ← 외부 압축 (반디집 호환)
qsafe-cli          ← CLI
```

## ⚠️ 면책 (DISCLAIMER)

```
qsafe는 "있는 그대로" 제공됩니다.
명시적/암시적 어떠한 보증도 없습니다.

데이터 손실, 패스워드 분실, 키 손상에 대한 책임은
사용자에게 있습니다.

중요 데이터는:
  1. 반드시 별도 백업하세요.
  2. 종이 백업을 종이에 적어 안전한 곳에 보관하세요.
  3. 운영체제 + 디스크 백업도 병행하세요.

이 도구를 사용함으로써 위 조건에 동의하는 것으로 간주됩니다.
```

자세한 면책은 `LICENSE-APACHE` 및 `LICENSE-MIT` 참조.

## 📜 라이센스

이 프로젝트는 **MIT OR Apache-2.0** 듀얼 라이센스입니다.
사용자가 선호하는 라이센스로 사용할 수 있습니다.

- `LICENSE-MIT` — MIT License
- `LICENSE-APACHE` — Apache License 2.0
- `NOTICE.md` — 제3자 라이브러리 라이센스
- `SECURITY.md` — 보안 정책 + 취약점 보고

## 🤝 기여

PR / Issue 환영합니다. 단:
- 보안 이슈는 `SECURITY.md` 절차에 따라 비공개 보고
- 새 알고리즘은 표준 (RFC, NIST, IETF) 우선
- 자체 암호 알고리즘 절대 NO (Kerckhoffs's principle)

## 🌐 언어

영어 / 한국어 (en/ko)
일본어 / 중국어 / 스페인어 등 향후 추가.

## 🔗 링크

- 저장소: https://github.com/heedong719/qsafe
- 이슈: https://github.com/heedong719/qsafe/issues
- 보안: `SECURITY.md`
