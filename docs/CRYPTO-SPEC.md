# qsafe 암호 사양 (Cryptographic Specification)

**버전**: 0.1
**상태**: 사전 출시 (외부 감사 미수행)
**날짜**: 2026-05

이 문서는 qsafe의 모든 암호 동작을 정밀하게 규정합니다.
외부 감사 또는 호환 구현체 작성 시 참조.

---

## 1. 파일 포맷 v2

### 1.1 매직 바이트
```
0..8:  "QSAFE001" (8 bytes ASCII)
```

### 1.2 헤더
```
8..12:  header_len (u32 LE)
12..12+header_len: header_cbor (CBOR-encoded FileHeader)
```

CBOR 구조 — `FileHeader`:
```cbor
{
    "version": u16,                      // VERSION = 2
    "suite": "v1-xchacha20-blake3",
    "compression": "none" | "zstd" | "gzip" | "lz4" | "brotli",
    "integrity": "blake3",
    "recipients": [Recipient, ...],
    "payload_nonce": bytes(24),          // batch: 24 random | stream: base_20 + zero_4
    "original_size": u64,
    "created_at_unix": i64,
    "label": Option<String>,
    "split": Option<SplitInfo>,
    "chunks": Option<ChunkInfo>          // Some → streaming mode
}
```

### 1.3 Batch 페이로드 (chunks = None)
```
header_end..header_end+8: payload_len (u64 LE)
header_end+8..header_end+8+payload_len: payload (AEAD ciphertext)
end-32..end: original_hash (BLAKE3, 32 bytes)
```

### 1.4 Streaming 페이로드 (chunks = Some)
```
헤더 끝부터 청크 시퀀스:
  chunk_0_ct_len (u32 LE)
  chunk_0_ct (chunk_0_ct_len bytes)
  ...
  chunk_N-1_ct_len (u32 LE)
  chunk_N-1_ct (chunk_N-1_ct_len bytes)
  original_hash (BLAKE3, 32 bytes)
```

청크 크기:
- `STREAM_CHUNK_SIZE` = 4 MiB (4 × 1024 × 1024)
- 마지막 청크는 `chunks.last_chunk_size` (≤ 4 MiB)

---

## 2. 수신자 (Recipient) 사양

### 2.1 Password Recipient

```cbor
{
    "type": "password",
    "salt": bytes(>=8),
    "argon2_m_kib": u32,        // 8192 ≤ m ≤ 16,777,216
    "argon2_t": u32,            // >= 1
    "argon2_p": u32,            // >= 1
    "nonce": bytes(24),
    "encrypted_file_key": bytes(48)   // file_key(32) + AEAD tag(16)
}
```

### 2.1.1 Argon2id → wrap_key 도출

```
argon2_out = Argon2id(
    password = user_password.as_bytes(),
    salt = salt,
    parallelism = argon2_p,
    memory = argon2_m_kib (in KiB),
    iterations = argon2_t,
    output_size = 32 bytes,
    version = 0x13
)

# HKDF 도메인 분리
wrap_key = HKDF-SHA256-Expand(
    PRK = HKDF-SHA256-Extract(salt, argon2_out),
    info = "qsafe-v1-password-wrap-key",
    L = 32
)
```

### 2.1.2 File Key 봉투화/해체

```
# Pack
encrypted_file_key = XChaCha20-Poly1305-Encrypt(
    key = wrap_key,
    nonce = recipient.nonce (24 bytes),
    aad = empty,
    plaintext = file_key (32 bytes)
)

# Unpack
file_key = XChaCha20-Poly1305-Decrypt(
    key = wrap_key,
    nonce = recipient.nonce,
    aad = empty,
    ciphertext = encrypted_file_key
)
```

### 2.2 Fido2 Recipient (CTAP hmac-secret)

```cbor
{
    "type": "fido2",
    "credential_id": bytes,
    "rp_id": "qsafe.local",
    "hmac_salt": bytes(32),
    "user_verification_required": bool,
    "nonce": bytes(24),
    "encrypted_file_key": bytes(48),
    "label": Option<String>
}
```

#### PRF 평가
```
prf_output = CTAP2.GetAssertion(
    rp_id = recipient.rp_id,
    credential_id = recipient.credential_id,
    extensions = { hmac-secret: { salt: hmac_salt } }
).hmac_secret  // 32 bytes (device 내부 HMAC-SHA256)
```

#### wrap_key 도출
```
wrap_key = HKDF-SHA256(
    salt = hmac_salt,
    ikm = prf_output,
    info = "qsafe-v1-fido2-prf-wrap-key",
    L = 32
)
```

### 2.3 Bip39 Recipient

```cbor
{
    "type": "bip39",
    "salt": bytes(16),
    "nonce": bytes(24),
    "encrypted_file_key": bytes(48),
    "word_count": u8,   // 12 | 15 | 18 | 21 | 24
    "language": "english",
    "label": Option<String>
}
```

#### Seed 도출 (BIP39 표준)
```
seed = PBKDF2-HMAC-SHA512(
    password = mnemonic_words.join(" ").as_bytes(),
    salt = "mnemonic" || passphrase,    // passphrase = "" (qsafe)
    iterations = 2048,
    output_size = 64
)
```

#### wrap_key 도출
```
wrap_key = HKDF-SHA256(
    salt = recipient.salt,
    ikm = seed,
    info = "qsafe-v1-bip39-paper-wrap-key",
    L = 32
)
```

### 2.4 Shamir Commitment (별도 share 파일)

```cbor
{
    "type": "shamir-commitment",
    "threshold": u8,
    "total_shares": u8,
    "set_id": String,
    "share_id_hashes": [String, ...]
}
```

실제 share 파일은 외부 (디스크 별도). 헤더는 메타데이터만.

---

## 3. Payload 암호화

### 3.1 Batch 모드 (작은 파일)
```
file_key = CSPRNG(32 bytes)
payload_nonce = CSPRNG(24 bytes)

compressed = Compress(plaintext)  // 알고리즘에 따라
ciphertext = XChaCha20-Poly1305-Encrypt(
    key = file_key,
    nonce = payload_nonce,
    aad = empty,
    plaintext = compressed
)
```

### 3.2 Streaming 모드 (100 MB 이상)
```
file_key = CSPRNG(32 bytes)
base_nonce = CSPRNG(20 bytes)
payload_nonce = base_nonce || zeros(4)

for chunk_idx in 0..num_chunks:
    chunk_plain = read chunks.chunk_size bytes (마지막은 last_chunk_size)
    chunk_nonce = base_nonce(20) || chunk_idx_be(4)
    chunk_ct = XChaCha20-Poly1305-Encrypt(
        key = file_key,
        nonce = chunk_nonce,
        aad = empty,
        plaintext = chunk_plain
    )
    output: chunk_ct.len() u32 LE || chunk_ct
```

청크별 다른 nonce → 재정렬 공격 차단.

---

## 4. 무결성 (BLAKE3)

### 4.1 Batch 모드
```
original_hash = BLAKE3(plaintext_raw)  // 압축 전 원본
verify: BLAKE3(decompressed) == original_hash
```

### 4.2 Streaming 모드
```
hasher = BLAKE3.new()
for chunk in chunks:
    hasher.update(chunk_plain)  // 평문에 대해
original_hash = hasher.finalize()  // 32 bytes
```

파일 끝 32 bytes에 저장 후 unpack 시 검증.

---

## 5. 보안 매개변수 강제

### 5.1 Argon2id (PasswordRecipient 검증)
```
MIN_M_KIB = 8192   (8 MiB)
MAX_M_KIB = 16,777,216   (16 GiB DoS guard)
MIN_T = 1
MIN_P = 1
```

경계 밖 매개변수 → `CryptoError::InvalidParams` 거부.

### 5.2 Default / Strong 프로필
- **Default**: m=64 MiB, t=3, p=4 (RFC 9106 second recommended)
- **Strong**: m=256 MiB, t=4, p=4

### 5.3 압축 폭탄 방어
```
MAX_DECOMPRESSION_RATIO = 1000
ABSOLUTE_MAX_OUTPUT = 10 GiB

limit = if expected_size.is_some():
    expected_size + 64
else:
    min(input_len * MAX_RATIO, ABSOLUTE_MAX_OUTPUT)
```

decompressor에 `Read::take(limit)` 적용.

### 5.4 헤더 DoS 방어
```
MAX_HEADER_LEN = 16 MiB
MAX_PAYLOAD_LEN = 1 PiB (sanity)
```

---

## 6. TOCTOU + 권한

### 6.1 출력 파일 생성
```
# POSIX
OpenOptions {
    write: true,
    create_new: true,    // 이미 존재 시 거부
    mode: 0o600,         // owner-only rw
    custom_flags: O_NOFOLLOW,  // 심볼릭 링크 거부
}
```

### 6.2 임시 파일 + rename
```
tmp = "<parent>/.<filename>.tmp.<pid>"
write_atomic(tmp) {
    write content
    fsync(tmp)
}
rename(tmp, output)  // POSIX atomic
```

panic / 에러 시 `TempFileGuard`로 자동 cleanup.

---

## 7. Constant-Time 비교

```rust
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}
```

패스워드 확인, hash 비교 등에 사용.

---

## 8. Zeroize

다음은 사용 후 즉시 zeroize:
- `wrap_key` (`#[inline(never)]` 보장)
- `argon_out` (HKDF 통과 후)
- `file_key` (Drop 시 자동)
- 패스워드 buffer (`pw_z.zeroize()`)
- Plaintext buffer (drop)

---

## 9. 헤더 라벨 Sanitize

CLI에서 사용자 표시 전:
```
사용 가능: '\n', '\t', ' ', 인쇄 가능 문자, 한글, 이모지
제거: 모든 다른 제어 문자 (\x00-\x1F 제외 위, \x7F)
길이 제한: 512 chars
```

ANSI escape (`\x1b`), null, bell 등 차단.

---

## 10. CSPRNG 소스

모든 무작위성 = `rand::rngs::OsRng`:
- macOS: `getentropy(3)` → `/dev/random`
- Linux: `getrandom(2)`
- Windows: `BCryptGenRandom`

응용:
- file_key, base_nonce, payload_nonce
- Argon2 salt, BIP39 entropy
- Shamir polynomial coefficients

---

## 11. 알고리즘 버전 관리

| Suite ID | 알고리즘 | 도입 |
|---|---|---|
| `v1-xchacha20-blake3` | XChaCha20-Poly1305 + BLAKE3 + Argon2id | v0.1 |

향후 (대비):
- `v2-aes256-gcm-blake3`: AES-256-GCM-SIV (하드웨어 AES 가속)
- `v3-pq-mlkem`: ML-KEM-768 통합

마이그레이션은 `qsafe migrate` 명령으로.

---

## 12. 알려진 한계

### 12.1 외부 감사 미수행
- v1.0 전 Trail of Bits / NCC Group / Cure53 감사 계획
- Bug Bounty 프로그램은 v1.0 동시

### 12.2 메타데이터 보호 없음
- `original_size`, `created_at_unix`, `label`은 헤더에 평문 저장
- 향후 metadata encryption mode 검토

### 12.3 Forward Secrecy 부분
- file_key는 매 파일 새로 생성 (forward secrecy ✓)
- 그러나 사용자 패스워드 유출 시 과거 모든 파일 풀림
- HSM 키 + 정기 rotation으로 완화 가능

---

## 13. 호환성

### 13.1 이전 chainlock 포맷 (0.x)
qsafe v0.1은 다음 magic을 모두 인식 (읽기 전용):
- `CHNLOCK1` (chainlock v0.1)
- `CHNLOCK2` (chainlock v0.2)
- `QSAFE001` (현재)

쓰기는 `QSAFE001`만.

### 13.2 향후 버전
모든 변경은 SemVer:
- Major: 매직 변경 (호환 불가)
- Minor: 새 알고리즘 / 새 수신자 타입 (구버전 읽기 가능)
- Patch: 버그 수정

---

## 14. 참고 문헌

- RFC 9106: Argon2 Memory-Hard Function
- RFC 8439: ChaCha20 and Poly1305 (XChaCha 확장)
- RFC 5869: HKDF
- BLAKE3 Specification (Jean-Philippe Aumasson et al.)
- BIP39: Mnemonic code for generating deterministic keys
- CTAP 2.1: hmac-secret Extension
- Shamir's Secret Sharing (Adi Shamir, 1979)
- FIPS 203: ML-KEM (NIST, 2024)
- Kerckhoffs's Principle (1883)
