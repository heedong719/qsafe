# CHANGELOG

모든 주요 변경사항을 기록합니다. 형식은 [Keep a Changelog](https://keepachangelog.com/) 준수.

버전 표기는 [SemVer](https://semver.org/) 기반.

## [Unreleased]

### 추가
- GUI 다이얼로그 (계획)
- OS 통합: Windows 탐색기 / macOS Finder / Linux 파일 매니저 (계획)
- 외부 감사 (Trail of Bits / NCC Group) — v1.0 전
- X25519 + ML-KEM-768 하이브리드 공개키 수신자
- ctap-hid-fido2 v3 API 마이그레이션 (qsafe-hardware/src/hw.rs Builder 패턴) — ring 0.17+로 RUSTSEC-2025-0009 해결

### 검토 중
- EGG / ALZ 한국 사유 포맷 풀기 (라이브러리 미성숙)
- 모바일 앱 (iOS / Android)
- WebAssembly 빌드

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
