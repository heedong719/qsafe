# qsafe 위협 모델 (Threat Model)

**버전**: 0.1
**STRIDE + 사용자 시나리오 기반**

---

## 1. 보호 대상

| 자산 | 가치 | CIA 우선순위 |
|---|---|---|
| 사용자 평문 파일 | 매우 높음 | C > I > A |
| 사용자 패스워드 | 매우 높음 | C 절대 |
| FIDO2 PRF secret | 매우 높음 | C 절대 (키 외부 노출 X) |
| BIP39 mnemonic (종이) | 매우 높음 | C 절대 |
| Shamir share (개별) | 중간 (단독 정보 0) | C |
| .qs 파일 메타데이터 (라벨 등) | 낮음 | I |
| 패스워드 OS 키링 | 높음 | C |

---

## 2. 공격자 모델

### 2.1 Threat Actor 분류

| 공격자 | 능력 | qsafe 방어 가능? |
|---|---|---|
| **T1: Script Kiddie** | 공개 도구 사용, 약한 패스워드 시도 | ✅ Argon2id 강제 |
| **T2: 가족/동료** | 같은 컴퓨터 접근, 어깨너머 | ✅ AEAD, 0600, OS 잠금 |
| **T3: 도난자** | 노트북 물리적 탈취 | ✅ Argon2id + 강 패스워드 |
| **T4: 표적 해커** | 사용자 표적, 사회공학 | ✅ + ⚠️ 사용자 위생 의존 |
| **T5: 조직범죄** | GPU 클러스터, 사이버 무기 | ✅ 양자 안전 통합 후 |
| **T6: 국가급 (수동)** | Bulk 감청, 메타데이터 분석 | ✅ + ⚠️ 메타데이터 leak 일부 |
| **T7: 국가급 (능동)** | 표적 침투, 백도어 요구 | ⚠️ 부분 (오픈소스 + 재현 빌드 후 강화) |
| **T8: 강압자** | 5달러 렌치 공격 | ⚠️ Deniability 미구현 (Phase 6) |

---

## 3. STRIDE 분석

### S — Spoofing (위장)
| 위협 | 방어 |
|---|---|
| 가짜 .qs 파일이 사용자 도구 흉내 | ✅ MAGIC 검증, 헤더 CBOR 검증 |
| qsafe 바이너리 자체 위장 | ⚠️ 코드사인 필요 (v1.0 전) |
| FIDO2 RP ID 위장 | ✅ rp_id = "qsafe.local" 헤더 명시 |

### T — Tampering (변조)
| 위협 | 방어 |
|---|---|
| 페이로드 1 byte 변조 | ✅ AEAD MAC 검증 |
| 헤더 변조 | ✅ CBOR 파싱 + 라벨 sanitize |
| 청크 재정렬 (streaming) | ✅ chunk_idx를 nonce에 포함 |
| 청크 누락 | ✅ num_chunks 사전 검증 |
| trailing bytes 삽입 | ✅ TrailingBytes 에러 |
| Argon2 매개변수 약화 | ✅ MIN_M_KIB 강제 |

### R — Repudiation (부인)
| 위협 | 방어 |
|---|---|
| 누가 풀었는지 증명 | ⚠️ Audit log 미구현 (Phase 6) |
| 파일 생성 시각 위변조 | ⚠️ 헤더 평문 (OpenTimestamps 검토) |

### I — Information Disclosure (정보 누출)
| 위협 | 방어 |
|---|---|
| 파일 내용 노출 | ✅ AEAD + 강 KDF |
| 패스워드 노출 (메모리) | ✅ Zeroize + #[inline(never)] |
| 패스워드 노출 (셸 히스토리) | ✅ --password 사용 시 경고 + 키링 기본 |
| 패스워드 노출 (스왑) | ⚠️ mlock 미구현 |
| 라벨/메타데이터 노출 | ⚠️ 평문 (의도) |
| 사이드 채널 (timing) | ✅ constant_time_eq |
| 사이드 채널 (전력/EM) | ⚠️ 일반 HW 한계 |

### D — Denial of Service
| 위협 | 방어 |
|---|---|
| 압축 폭탄 (1KB → 100GB) | ✅ MAX_RATIO + Read::take |
| 헤더 폭탄 (1 PiB payload_len 선언) | ✅ MAX_PAYLOAD_LEN sanity |
| Argon2 m=∞ DoS | ✅ MAX_M_KIB = 16 GiB |
| 디스크 풀 (ENOSPC) | ✅ 임시파일 cleanup (RAII) |
| OOM (큰 파일) | ✅ Streaming 모드 자동 |

### E — Elevation of Privilege
| 위협 | 방어 |
|---|---|
| 경로 이탈 (../) | ✅ sanitize_archive_path |
| 심볼릭 링크 공격 | ✅ O_NOFOLLOW |
| TOCTOU race | ✅ create_new |
| 출력 위치 임의 쓰기 | ✅ paths_must_differ + canonical |

---

## 4. 신뢰 경계 (Trust Boundaries)

```
┌──────────────────────────────────────────┐
│  ① 사용자 머릿속 (패스워드)                │ ← 절대 신뢰
└──────────────────────────────────────────┘
              │
              ▼
┌──────────────────────────────────────────┐
│  ② qsafe 프로세스 (메모리)                │ ← 신뢰
│     - 평문 처리                            │
│     - 키 derivation                       │
│     - AEAD                                │
└──────────────────────────────────────────┘
              │
              ▼
┌──────────────────────────────────────────┐
│  ③ OS 커널 (FS, 메모리)                   │ ← 부분 신뢰
└──────────────────────────────────────────┘
              │
              ▼
┌──────────────────────────────────────────┐
│  ④ 디스크 (.qs 파일)                      │ ← 적대적 가능
└──────────────────────────────────────────┘
              │
              ▼
┌──────────────────────────────────────────┐
│  ⑤ 네트워크 / 다른 사용자                  │ ← 적대적
└──────────────────────────────────────────┘
```

각 경계에서:
- ① → ②: 패스워드 입력 (rpassword, no echo)
- ② → ③: 시스템 콜 (fs::write, OpenOptions)
- ③ → ④: 디스크 fsync
- ④ → ⑤: 파일 전송 (별도 채널)

---

## 5. 미해결 위협 (Open Issues)

### 5.1 Plausible Deniability 부재 (T8: 강압자)
- 강요당해 패스워드 공개 시 모든 내용 노출
- 향후 hidden volume 모드 검토 (VeraCrypt 영감)

### 5.2 메모리 보호 미흡
- mlock() 미구현 → 패스워드가 swap 갈 수 있음
- secure_alloc 영역 미사용
- 향후 secrecy crate 검토

### 5.3 외부 감사 미수행
- 코드 변경/회귀 가능
- v1.0 전 Trail of Bits / NCC Group 의뢰

### 5.4 공급망 공격
- Cargo.lock 의존성 변조 → 빌드 결과 신뢰 어려움
- 향후 Reproducible Build + Sigstore 통합

### 5.5 키 회복 메커니즘 부족
- 패스워드 분실 시 영구 손실
- 종이 백업 강제 옵션 부재
- 향후 키 escrow + dead-man switch 검토

---

## 6. 위협 모델 갱신 정책

이 문서는 매 minor release마다 검토:
- 새 기능 추가 시 STRIDE 재분석
- 새 의존성 추가 시 공급망 위협 갱신
- CVE 발생 시 즉시 갱신

---

## 7. 보고

새 위협 발견 시 `SECURITY.md` 절차 준수.
