# 보안 정책 — Security Policy

## 취약점 보고 — Reporting a Vulnerability

qsafe 보안 이슈를 발견하셨다면 다음 절차를 따라주세요:

### 🚨 즉시 공개하지 마세요 (Do NOT open a public issue)

다음 채널 중 하나로 비공개 보고:

1. **이메일**: `heedong719@gmail.com` (Subject: `[qsafe-SECURITY]`)
2. **GitHub Security Advisory** (저장소 공개 후): private advisory

### 📋 보고 시 포함해 주실 정보

- 취약점 유형 (e.g., buffer overflow, key leak, AEAD bypass)
- 영향받는 버전
- 재현 단계 또는 PoC
- 잠재 영향 (CIA — Confidentiality / Integrity / Availability)
- 발견자 정보 (선택, 크레딧 표시 원하시면)

### ⏱️ 응답 시간 (Best Effort)

| 단계 | 목표 시간 |
|---|---|
| 접수 확인 | 72시간 이내 |
| 초기 평가 | 1주 이내 |
| 수정 패치 | 심각도별 (Critical 30일, High 90일) |
| 공개 (Coordinated Disclosure) | 패치 배포 후 30-90일 |

## 🛡️ 위협 모델 — Threat Model

qsafe가 방어하는 위협:

### ✅ 방어 대상
- 약한 패스워드 brute force (Argon2id로 차단)
- 오프라인 사전 공격
- 파일 위변조 (XChaCha20-Poly1305 AEAD)
- 알고리즘 매개변수 조작 (최소값 강제)
- 압축 폭탄 (ratio limit)
- ANSI escape injection in 라벨
- 심볼릭 링크 공격 (O_NOFOLLOW)
- TOCTOU race conditions
- 양자컴 미래 위협 (ML-KEM 통합 예정)
- 키 메모리 잔존 (Zeroize)

### ❌ 방어 불가 (사용자 책임)
- 키로거가 설치된 환경
- 권한 있는 멀웨어
- 물리적 디스크 압수 + 패스워드 강요
- 사용자가 패스워드 약하게 설정
- 사용자가 백업 안 함
- 운영체제 root 권한자
- HSM/TPM 펌웨어 백도어
- 사이드 채널 (전력 분석, 음향)

### ⚠️ SFX 자기압축해제 신뢰 모델 (v0.1.5+)

`qsafe pack --sfx`는 사용자 친화성과 보안을 교환하는 패턴이다:

- **payload는 여전히 안전**: SFX 안의 `.qs` payload는 AEAD + keyed BLAKE3 검증을 그대로 거친다. payload만 변조하면 stub이 실패한다.
- **stub 자체는 변조 가능**: 공격자가 SFX 파일의 stub 부분을 자기 코드로 교체할 수 있다. 사용자는 자기가 받은 `.run`/`.exe`가 진짜 qsafe stub인지 확신할 방법이 없다.
- **배포 권장 사항**:
  - 가능하면 **codesign / notarization**된 SFX만 배포 (macOS Apple Developer ID, Windows EV cert)
  - 그렇지 않다면 일반 `.qs` 파일 + qsafe CLI 사용 권장
  - SFX 받은 사용자도 가능하면 `qsafe unpack`으로 검증
- **alarm**: macOS Gatekeeper / Windows SmartScreen이 unsigned SFX를 차단하는 것은 사용자 보호의 일부이므로 우회 가이드를 제공하지 않는다.

### ⚠️ qsafe-gui process argv 노출 (v0.1.6+ qsafe-gui)

`qsafe-gui`는 사용자 입력 패스워드를 `qsafe-cli` 호출 시 `--password <PW>` argv로
전달합니다 (`crates/qsafe-gui/src/commands.rs`의 `cmd.arg("--password").arg(pw)`).

- **위험**: 같은 머신의 다른 사용자가 `ps auxww` 또는 `/proc/<pid>/cmdline`에서 패스워드 평문을 볼 수 있다.
- **단일 사용자 데스크톱**에서는 영향 미미하지만 multi-user 환경, container, 공용 시스템에서는 critical.
- **후속 마이그레이션** (계획됨): GUI가 child process의 stdin에 패스워드를 pipe해서 `--password` argv 사용을 제거. 기존 `qsafe-cli`의 `rpassword::prompt_password`가 비대화형 stdin에서 한 줄 읽어주는 동작을 활용.
- **임시 회피**: 단일 사용자 환경에서만 GUI 사용, 또는 SFX/pubkey 모드 (패스워드 불필요) 사용.

### ⚠️ qsafe-gui "open mode" / `PUBLIC_PASSWORD` (v0.1.6+ qsafe-gui)

`qsafe-gui`의 "기본 — 패스워드 없음" 압축 모드는 내부 고정값 `qsafe-public-v1`을 사용합니다.

- **이건 암호화가 아닙니다**: 모든 사용자가 같은 패스워드 → plaintext와 동등.
- 의도: "qsafe 포맷의 압축만 원하고 암호화는 불필요" 사용 사례에 편의 제공 (예: 단순 파일 묶음 + tar 자동화 + MD5 검증).
- 진짜 비밀이 있는 파일은 반드시 사용자 패스워드 또는 `--pubkey` / `--fido2` 옵션 사용.

### ⚠️ Pubkey recipient 신뢰 모델 (v0.1.5+)

- **transcript 바인딩**: HKDF salt에 `ephemeral_pk || recipient_x25519_pk || mlkem_ct || recipient_mlkem_pk`를 모두 포함하므로, 중간자가 일부 값만 바꿔도 wrap_key 도출이 실패한다.
- **하이브리드 안전성**: X25519가 깨져도 ML-KEM이 막고, ML-KEM이 깨져도 X25519가 막는다. 두 알고리즘이 모두 깨지려면 양자컴 + 새 수학적 돌파가 둘 다 필요하다.
- **PFS (Forward Secrecy)**: 매 봉투마다 새 ephemeral X25519 키쌍이 생성되므로 장기 키가 유출돼도 과거 봉투는 안전.
- **identity 파일 보호**: secret identity JSON 파일은 **외부 공유 금지**. 현재는 평문 JSON으로 저장되며, 향후 OS 키링 / 패스워드 보호 옵션이 추가될 예정.

## 🔐 암호 사양 — Cryptographic Specification

상세 사양은 `docs/CRYPTO-SPEC.md` 참조 (향후 작성).

핵심 알고리즘:
- **KDF**: Argon2id (RFC 9106), m≥8 MiB, t≥1, p≥1
- **AEAD**: XChaCha20-Poly1305 (RFC 8439 확장)
- **Hash**: BLAKE3
- **Key Wrapping**: HKDF-SHA256 도메인 분리
- **PQ-Ready**: ML-KEM-768 (FIPS 203) 통합 예정
- **Hardware**: FIDO2 PRF (hmac-secret) 옵션

## 📜 면책 — Disclaimer

**THIS SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.**

qsafe는 베스트 에포트로 작성되었으나:
- 외부 감사를 거치지 않은 베타 상태입니다.
- 군사/정부/생명 안전 시스템에 사용하지 마세요.
- 중요 데이터는 항상 별도 백업하세요.
- 패스워드/키 분실 시 데이터는 영구 손실됩니다.

## 🏆 명예의 전당 — Hall of Fame

보안 취약점을 책임감 있게 보고해 주신 분들:

(아직 없습니다 — 첫 보고자가 되어 주세요!)

## 📅 변경 이력 — Security Changelog

### v0.1.0 (2026-05, 철회됨)
- 릴리스 직후 RUSTSEC-2024-0398 (sharks 다항식 bias) 발견으로 즉시 철회
- v0.1.1로 대체 — 사용 금지

### v0.1.1 (2026-05-27, 보안 핫픽스)
- **RUSTSEC-2024-0398 해소**: Shamir 구현체 `sharks 0.5` → `blahaj 0.6` 교체 (Cure53 감사 fork)
- 초기 보안 모델 구현 (v0.1.0에서 계승)
- 6개 Critical 갭 사전 봉합 (압축 폭탄, TOCTOU, Argon2 검증, escape sanitize, 0600 권한, zeroize)
- 외부 감사 없음

### v0.1.2 (2026-05-27, 보안 + CI 개선)
- **스트리밍 모드 BLAKE3 keyed-mode 적용** (R1-1, 내부 발견):
  - 이전: plain BLAKE3 → known-plaintext + 청크 truncation으로 trailing hash 위조 이론상 가능
  - 수정: `derive_key("qsafe-v1-stream-integrity", file_key)` 기반 keyed BLAKE3로 위조 불가
  - Breaking: v0.1.1 stream `.qs` 파일은 v0.1.2로 풀 수 없음 (batch 파일은 영향 없음)
- `SystemTime UNIX_EPOCH` unwrap → panic-free (시계 이상 환경 방어, run_in_memory)

#### 알려진 이슈 (Known Issues)

v0.1.3에서 ctap-hid-fido2 v3 마이그레이션이 완료되어 transitive `ring 0.17.14`가 사용됩니다 — **RUSTSEC-2025-0009은 더 이상 적용되지 않습니다**. 현재 추적 중인 알려진 보안 이슈는 없습니다.

### 향후
- v1.0: 외부 감사 (Trail of Bits / NCC Group)
- Bug Bounty 프로그램 가동
