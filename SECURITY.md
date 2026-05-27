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

#### 알려진 이슈 (Known Issues)

| ID | 영향 범위 | 실제 영향 | 후속 조치 |
|---|---|---|---|
| RUSTSEC-2025-0009 (ring 0.16.20) | `fido2-hw` feature (default OFF)의 transitive 의존 | **없음** — QUIC `HeaderProtectionKey::new_mask()` panic이며 qsafe는 QUIC 미사용 | ctap-hid-fido2 v3 API 마이그레이션 후 ring 0.17+로 자동 해결 |

### 향후
- v1.0: 외부 감사 (Trail of Bits / NCC Group)
- Bug Bounty 프로그램 가동
