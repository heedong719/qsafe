# CHANGELOG

모든 주요 변경사항을 기록합니다. 형식은 [Keep a Changelog](https://keepachangelog.com/) 준수.

버전 표기는 [SemVer](https://semver.org/) 기반.

## [Unreleased]

### 추가
- OS 통합: Windows 탐색기 / macOS Finder / Linux 파일 매니저 (계획)
- 외부 감사 (Trail of Bits / NCC Group) — v1.0 전
- GUI M2~M8: pack/unpack 명령, 진행 바, drag&drop, 다중 수신자 UI (qsafe-gui scaffold는 v0.1.5에 활성화됨)
- SFX cross-compile 4-target 자동 빌드 (CI release workflow 확장)
- SFX codesign / notarization (macOS Gatekeeper / Windows SmartScreen 통과용)

---

## [0.1.5] — 2026-05-28 (CLI Pubkey + SFX 자기압축해제 + GUI MVP scaffold)

### 추가 — 새 기능 3종

#### 1. CLI Pubkey recipient 통합 (X25519 + ML-KEM-768 하이브리드)
- 새 명령: `qsafe identity generate / show / export-pubkey`.
- `qsafe pack --pubkey <recipient.pub.json>` (복수 지정 가능, OR 논리).
- `qsafe unpack --identity <my-secret.json>` (Pubkey recipient 풀기).
- 키 직렬화: JSON (`IdentitySecretBytes` / `IdentityPublic`).
- 단위 + e2e 검증: 86 → 90 → **95 tests passing**. SHA256 라운드트립 일치.

#### 2. SFX (Self-Extracting eXecutable) 자기압축해제
- 새 crate `qsafe-stub` (lib + binary): SFX 포맷 정의 + 최소 extractor binary.
- SFX 파일 구조: `[stub binary][.qs payload][payload_len u64 LE 8B][SFX_MAGIC "QSAFESFX" 8B]`.
- `qsafe pack --sfx [--sfx-stub <PATH>]`: 결과로 `<input>.run` (Unix) 또는 `<input>.exe` (Windows) 단일 실행파일 생성. 0755 권한 자동.
- Stub binary는 `current_exe()` → footer 읽음 → payload 추출 → password prompt (TTY) 또는 stdin (pipe) → AEAD decrypt + BLAKE3 검증 → 원본 복원.
- ⚠️ SFX는 unsigned 실행파일이라 macOS Gatekeeper / Windows SmartScreen 차단 가능. codesign / notarization은 후속 작업.
- e2e: 66 bytes → 1.3 MB SFX 실행파일 → 풀기 후 SHA256 일치 확인.

#### 3. qsafe-gui (Tauri 2.x) MVP scaffold
- 새 crate `qsafe-gui`: Tauri 기반 GUI 프론트엔드 (HTML/CSS/JS + Rust commands).
- MVP commands: `about`, `file_info`, `identity_generate`, `identity_show`.
- frontend: 단일 `ui/index.html` (dark theme, identity 관리 UI).
- 윈도우: 960×640 기본, min 720×480, resizable.
- 한계: pack/unpack, drag&drop, 진행 바는 다음 마일스톤 (M2~M8). OS 통합 (Finder/Explorer)은 v0.2.x 로드맵.

### 정리

- `deny.toml`의 stale RUSTSEC-2025-0009 ignore 항목 제거 (v0.1.3 마이그레이션으로 더 이상 적용 안 됨).
- 모든 새 crate에 workspace 메타 (license/repository/keywords/categories) 일관 상속.

### 보안 모델 변동

- **SFX는 사용자 신뢰 모델에 큰 영향**: 사용자가 임의 `.exe` 더블 클릭 = 광범위한 공격 표면. payload는 여전히 AEAD + BLAKE3 검증되므로 payload 변조는 잡히지만, stub 자체가 변조될 수 있다는 한계가 SFX 본질. 권장: codesign / notarization 적용 후 배포.
- **Pubkey recipient의 transcript MitM 방어**: HKDF salt에 ephemeral_pk + recipient_pk + ML-KEM ct + recipient_mlkem_pk를 모두 포함하므로 중간자가 일부 값만 바꿔도 wrap_key 도출 실패.

---

## [0.1.4] — 2026-05-28 (qsafe-identity 활성화: X25519 + ML-KEM-768)

---

## [0.1.4] — 2026-05-28 (qsafe-identity 활성화: X25519 + ML-KEM-768)

### 추가

- **qsafe-identity crate를 workspace에 활성화** (이전까지 보류 상태였음).
  - X25519 (Diffie-Hellman) + ML-KEM-768 (FIPS 203 PQ KEM) 하이브리드 봉투 wrap/unwrap.
  - IKM = X25519_shared || ML-KEM_shared, transcript salt = BLAKE3/SHA256(eph_pk || recipient_pk || ct || mlkem_pk)으로 MitM 방어.
  - 도메인 분리: HKDF info `qsafe-v1-pq-hybrid-pubkey-wrap-key`.
  - 라이브러리만 활성화. CLI 통합 (`qsafe pack --pubkey`)은 다음 릴리스에 진행.

### 의존성 / 마이그레이션

- `ml-kem 0.2.3` API 호환 마이그레이션 (전부 `qsafe-identity` 내부):
  - `DecapsulationKey<MlKem768>` → `<MlKem768 as KemCore>::DecapsulationKey` (associated type 사용; `MlKem768`은 `Kem<MlKem768Params>` 래퍼라 `ParameterSet`을 직접 구현하지 않음)
  - `as_bytes()` / `from_bytes()` 호출 위해 `ml_kem::EncodedSizeUser` trait import 명시
  - `Encapsulate<EK, SS>`가 2개 type parameter generic이라 추론 실패 → `(Ciphertext<MlKem768>, SharedKey<MlKem768>)` explicit annotation
- `x25519_dalek::StaticSecret::random_from_rng(rng)` (clippy::needless_borrows_for_generic_args 준수)
- ML-KEM 0.3.x는 MSRV 1.85를 요구해 우리 1.80 MSRV와 호환되지 않으므로 0.2.3 유지.

### 테스트

- workspace 테스트 85 → **90** (qsafe-identity 단위 테스트 +5: identity 라운드트립, IdentityPublic fingerprint, IdentitySecretBytes 직렬화, PubkeyWrapper 라운드트립, transcript MitM 거부).
- clippy `--workspace --all-features --no-deps -- -D warnings`: clean
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok

---

## [0.1.3] — 2026-05-27 (FIDO2 의존 업그레이드, ring 0.17)

---

## [0.1.3] — 2026-05-27 (FIDO2 의존 업그레이드, ring 0.17)

### 🔒 보안

- **RUSTSEC-2025-0009 해소**: `ctap-hid-fido2` v2.2 → v3.5 마이그레이션 → `ring 0.16.20` → `ring 0.17.14` 자동 갱신.
  - 이전 알려진 이슈였던 QUIC `HeaderProtectionKey::new_mask()` panic 제거.
  - 영향 범위였던 `fido2-hw` feature 사용자도 이제 패치된 ring을 사용.
- `deny.toml`의 `ignore = [RUSTSEC-2025-0009]` 항목 제거 가능 (실제 advisory가 더 이상 발생하지 않음).

### 🔧 의존성 / 빌드

- `ctap-hid-fido2 = "3"` (workspace dependency).
- `qsafe-hardware/src/hw.rs` 마이그레이션:
  - `ctap_hid_fido2::get_assertion_params::Extension` → `ctap_hid_fido2::fidokey::AssertionExtension`
  - `ctap_hid_fido2::make_credential_params::Extension` → `ctap_hid_fido2::fidokey::CredentialExtension`
  - top-level `make_credential_with_args(&cfg, &args)` / `get_assertion_with_args(&cfg, &args)` → `FidoKeyHidFactory::create(&cfg)?` 후 `device.make_credential_with_args(&args)` / `device.get_assertion_with_args(&args)` 메서드 호출
- Transitive 업데이트: `hidapi v1.5 → v2.6`. CI Ubuntu runner는 `libudev-dev`가 필요하며 ci.yml에 추가됨.

### 🔧 CI 수정 (a55a104)

- Ubuntu CI clippy step이 `hidapi` 빌드 단계에서 `libudev` pkg-config 누락으로 실패하던 문제 해소: `apt-get install libudev-dev pkg-config` step 추가 (Linux 전용 + MSRV job).
- Linux-only `run_in_memory`의 `*const i8` → `*const libc::c_char` (aarch64 호환).

---

## [0.1.2] — 2026-05-27 (보안 + CI + 코드 품질)

### 검토 중
- EGG / ALZ 한국 사유 포맷 풀기 (라이브러리 미성숙)
- 모바일 앱 (iOS / Android)
- WebAssembly 빌드

---

## [0.1.2] — 2026-05-27 (보안 + CI + 코드 품질)

### 🔒 보안 (Breaking change for stream files)

- **스트리밍 모드 BLAKE3 keyed-mode 적용** (R1-1 — Known-plaintext truncation forgery 차단)
  - 이전: `blake3::Hasher::new()` (unkeyed) — 공격자가 평문 일부를 알면 청크 truncation + 헤더 ChunkInfo 변조 + trailing hash 위조 가능
  - 수정: `blake3::Hasher::new_keyed(derive_key("qsafe-v1-stream-integrity", file_key))`. 공격자가 file_key를 모르면 hash 위조 불가.
  - 호환성: v0.1.1 stream `.qs` 파일은 v0.1.2로 풀 수 없음. v0.1.1 사용자 거의 없으므로 영향 최소. batch (`<100 MB`) 파일은 영향 없음.
  - 추가 회귀 테스트: `stream_integrity_hasher_is_keyed_and_key_dependent` (key 의존성 + plain BLAKE3와 분리 검증)

### 🔧 CI / 빌드 신뢰성

- `RUSTFLAGS: -D warnings` 환경 변수 제거 — 외부 crate 워닝까지 에러로 만드는 위험 제거. clippy 명령에 `--no-deps -- -D warnings`만 유지.
- `rustsec/audit-check` 제거 — `cargo-deny`와 중복이고 `deny.toml`의 ignore 리스트를 무시. cargo-deny가 advisories/licenses/bans/sources를 통합 검사.
- `EmbarkStudios/cargo-deny-action` v1 → v2 (cargo-deny 0.19+ 스키마 호환).
- MSRV 1.80 빌드 실패 해소: `qsafe-cli/main.rs`의 unused imports (`OsStrExt`, `AsRawFd`) 제거.

### 🧹 코드 품질 (clippy clean)

- `qsafe-shamir`: `EncodedShare::to_string` → `impl Display` (clippy::inherent_to_string).
- `qsafe-shamir`: `total > MAX_SHARES` 비교 제거 (clippy::absurd_extreme_comparisons — `u8 > 255`는 항상 false).
- `qsafe-formats/brotli_fmt.rs`: `BrotliEncoderParams` 빌드 패턴 (clippy::field_reassign_with_default).
- `qsafe-cli/main.rs`:
  - 중복 `#[allow(clippy::too_many_arguments)]` 제거
  - `ConfigCmd` enum에 `#[allow(clippy::enum_variant_names)]` (의도된 *Password 접미사)
  - `(a + b - 1) / b` → `a.div_ceil(b)` 2곳
  - `sanitize_for_terminal` `filter_map` → `filter` (None/Some(c) 단순화)
- `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` → `.unwrap_or(Duration::ZERO).as_nanos()` (macOS/Windows `run_in_memory`) — 시계 이상 환경 panic 회피.

### 🛡️ 알려진 이슈 (변동 없음)

- RUSTSEC-2025-0009 (ring 0.16.20 transitive via ctap-hid-fido2 v2, `fido2-hw` feature) — QUIC 미사용으로 실제 영향 없음. v3 마이그레이션 시 자동 해결.

---

## [0.1.1] — 2026-05-27 (보안 핫픽스)

### 🔒 보안
- **Shamir M-of-N 다항식 계수 bias 수정** (RUSTSEC-2024-0398)
  - `sharks 0.5` → `blahaj 0.6` 교체 (Cure53가 감사한 fork)
  - 이전 영향: 같은 secret을 500–1500회 공유 시 일부 byte 값 brute force 가능
  - blahaj는 sharks와 100% API 호환 (drop-in replacement) — 기존 share 데이터는 영향 받지 않음
- **cargo-deny 0.19+ 스키마 마이그레이션**: deny.toml 재작성, advisories/licenses 자동 deny 활용
- **알려진 이슈** (영향 없음):
  - RUSTSEC-2025-0009 (ring 0.16.20 QUIC `HeaderProtectionKey::new_mask()` panic) — `fido2-hw` feature(default OFF) 의존성만 transitive. qsafe는 QUIC 미사용이라 실제 영향 없음 (ctap-hid-fido2 v3 마이그레이션으로 해결 예정).

### 정리
- 빌드 경고 3개 제거 (lz4.rs의 미사용 `Write`, main.rs의 미사용 BIP39 import, credentials의 dead code 주석)
- 워크스페이스 메타데이터 inheritance 정리 (`repository`, `homepage`, `readme`, `keywords`, `categories` 일관 적용)
- 내부 crate path 의존성에 version 명시 (publish 가능 상태)

---

## [0.1.0] — 2026-05 (개발 중, pre-release — 보안 이슈로 철회)

> ⚠️ v0.1.0은 릴리스 직후 RUSTSEC-2024-0398 발견으로 즉시 철회되었습니다. v0.1.1을 사용하세요.

### 추가
- **다중 수신자 봉투** — Password / FIDO2 / BIP39 / Shamir / Pubkey (OR 논리)
- **Password 수신자**: Argon2id (RFC 9106) + HKDF-SHA256 + XChaCha20-Poly1305
- **FIDO2 PRF 수신자**: hmac-secret (CTAP 표준) + 가상 PrfBackend (테스트용)
- **BIP39 종이 백업**: 24단어 영어 mnemonic + HKDF 도메인 분리
- **Shamir M-of-N**: GF(256), 종이 백업 친화 인코딩 (qs1-M-N-XX-HEX)
- **스트리밍 I/O**: 4MB 청크별 AEAD (재정렬 공격 차단) + BLAKE3 streaming hash
  - 1 GB 파일을 78 MB 메모리로 처리
  - 자동 분기: 100 MB 이상 streaming 모드
- **다중 압축 알고리즘**: zstd (zstdmt) / gzip / lz4 / brotli / xz / bzip2 / 7z / tar
- **외부 포맷 풀기**: RAR (unrar) / ZIP / 7z / TAR.* / GZIP / XZ / BZ2 / LZ4 / Zstd / Brotli
- **분할 압축**: split/merge 명령
- **인메모리 실행**: Linux memfd_create, macOS posix_spawn + immediate unlink
- **Mnemonic 유틸**: BIP39 generate / verify / info
- **OS 키링 통합**: macOS Keychain / Windows Credential Manager / Linux Secret Service
- **법적 문서**: MIT + Apache-2.0 듀얼, NOTICE.md (unrar 포함), SECURITY.md

### 보안
- 6개 Critical 갭 봉합:
  1. 압축 폭탄 방어 (ratio limit + size check)
  2. TOCTOU 방어 (O_NOFOLLOW + create_new)
  3. Argon2 매개변수 강제 (MIN_M_KIB=8MiB, MIN_T=1, MIN_P=1, MAX_M_KIB=16GiB)
  4. ANSI escape sanitize (헤더 라벨)
  5. 파일 권한 0600 (POSIX)
  6. wrap_key zeroize 보장 (`#[inline(never)]` on derive_key)
- 84개 회귀 테스트

### 성능
- zstd 멀티스레드 (zstdmt feature) — 8 코어 활용
- 측정값 (M-series Mac):
  - 50 MB 압축+암호화: 1.03초 (Argon2 1초 포함)
  - 순수 압축 처리량: ~1.6 GB/s
  - 압축 해제: ~170 MB/s
  - **1 GB 파일 peak memory: 78 MB**

### 호환성
- 이전 chainlock (.cl, .clk) 파일 읽기 가능 (호환성 모드)
- 새 파일은 .qs + magic `QSAFE001`

### 알려진 한계
- GUI 없음 (CLI만)
- OS 우클릭 통합 없음
- EGG / ALZ 미지원 (사유 포맷)
- ML-KEM 통합 보류 (ml-kem v0.2 API 변동 후 진행)
- 외부 감사 미수행

---

## [pre-0.1] (legacy chainlock)

블록체인 기반 컨셉으로 시작했으나, 실사용 분석 후 폐기:
- Lit Protocol Naga sunset (2026.04)
- 블록체인 의존성 = 단일 장애점
- 53% 암호화폐 폐기 사례 학습
→ 100% 오프라인 + 표준 알고리즘으로 재설계

## 마이그레이션 가이드

### chainlock 0.x → qsafe 0.1
- 확장자 `.cl`/`.clk` → `.qs`
- 매직 `CHNLOCK1`/`CHNLOCK2` → `QSAFE001`
- 이전 파일은 qsafe 0.1로 풀기 가능 (자동 인식)
- 새 파일은 qsafe 포맷으로만 생성
