# qsafe — Third-Party Notices

qsafe는 다음 제3자 라이브러리를 포함하거나 정적 링크합니다.
각 라이브러리의 원본 라이센스를 준수합니다.

## 핵심 암호 라이브러리 (모두 Apache-2.0 또는 MIT)

| 라이브러리 | 용도 | 라이센스 |
|---|---|---|
| `argon2` | Argon2id KDF | MIT OR Apache-2.0 |
| `chacha20poly1305` | XChaCha20-Poly1305 AEAD | MIT OR Apache-2.0 |
| `blake3` | BLAKE3 hashing | Apache-2.0 OR MIT OR CC0 |
| `hkdf` | HKDF-SHA256 | MIT OR Apache-2.0 |
| `sha2` | SHA-256/512 | MIT OR Apache-2.0 |
| `zeroize` | Secret zeroization | Apache-2.0 OR MIT |
| `x25519-dalek` | X25519 ECDH | BSD-3-Clause |
| `ml-kem` | ML-KEM-768 post-quantum KEM | Apache-2.0 OR MIT |
| `bip39` | BIP39 mnemonic | CC0-1.0 |

## 압축 라이브러리

| 라이브러리 | 용도 | 라이센스 |
|---|---|---|
| `zstd` (Facebook) | Zstandard compression | BSD-3-Clause |
| `flate2` + `miniz_oxide` | DEFLATE/gzip | MIT OR Apache-2.0 |
| `lz4_flex` | LZ4 frame format | MIT |
| `brotli` (Google) | Brotli | BSD-3-Clause OR MIT |
| `bzip2-rs` | bzip2 (pure Rust) | MIT |
| `lzma-rs` | XZ/LZMA | MIT |
| `ruzstd` | zstd decode (pure Rust) | MIT |
| `tar` | TAR container | MIT OR Apache-2.0 |
| `sevenz-rust2` | 7-Zip (pure Rust) | Apache-2.0 |
| `zip` | ZIP format | MIT |

## RAR 풀기 (특별 라이센스)

### `unrar` 크레이트 — Eugene Roshal의 UnRAR 라이브러리 사용

qsafe는 7-Zip과 동일한 방식으로 RAR 풀기 기능을 제공합니다.
이는 Eugene Roshal의 UnRAR 소스코드를 사용하며, 다음 조건을 준수합니다:

```
The unRAR sources may be used in any software to handle RAR archives
without limitations free of charge, but cannot be used to re-create the
RAR compression algorithm, which is proprietary. Distribution of modified
UnRAR sources in separate form or as a part of other software is permitted,
provided that it is clearly stated in the documentation and source
comments that the code may not be used to develop a RAR (WinRAR)
compatible archiver.
```

**qsafe는 RAR 압축 (생성) 기능을 제공하지 않습니다.** 오직 풀기만 가능합니다.
RAR 생성은 [WinRAR](https://www.win-rar.com/) 정품을 사용하세요.

상세 라이센스 전문은 `unrar` 크레이트의 라이센스 파일을 참조하세요.

## FIDO2 / 하드웨어 (옵션 기능 `--features fido2-hw`)

| 라이브러리 | 용도 | 라이센스 |
|---|---|---|
| `ctap-hid-fido2` | CTAP HID FIDO2 통신 | MIT OR Apache-2.0 |

## CLI / 유틸리티

| 라이브러리 | 라이센스 |
|---|---|
| `clap` | MIT OR Apache-2.0 |
| `serde` | MIT OR Apache-2.0 |
| `tokio` | MIT |
| `tracing` | MIT |
| `anyhow` / `thiserror` | MIT OR Apache-2.0 |
| `chrono` | MIT OR Apache-2.0 |
| `ciborium` | Apache-2.0 |
| `rpassword` | Apache-2.0 |

## 전체 의존성 트리

전체 라이센스 정보는 빌드 시 자동 생성됩니다:
```bash
cargo install cargo-about
cargo about generate about.hbs > LICENSES-FULL.html
```

## 라이센스 호환성

qsafe 자체 라이센스 (MIT OR Apache-2.0)는 위 모든 라이센스와 호환됩니다.
LGPL/GPL 라이브러리는 사용하지 않습니다 (정적 링크 시 라이센스 전염 회피).

---

이 NOTICE 파일은 Apache 2.0 라이센스 조항 4.(d)에 따라 제공됩니다.
