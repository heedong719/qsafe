# qsafe 기여 가이드

기여를 환영합니다! 다음 절차를 따라주세요.

## 🚀 빠른 시작

```bash
# 저장소 클론
git clone https://github.com/heedong719/qsafe
cd qsafe

# 빌드
cargo build --release

# 테스트
cargo test --release

# 모든 feature 빌드
cargo build --release --all-features
```

## 📋 기여 유형

### 🐛 버그 리포트
- GitHub Issues 사용
- 재현 단계, 환경 (OS, Rust 버전), 예상 vs 실제 동작 포함
- **보안 이슈는 `SECURITY.md` 절차를 따라 비공개 보고**

### ✨ 기능 제안
- GitHub Discussions에서 먼저 논의
- 어떤 사용 사례? 다른 도구와 차이점?
- 자체 암호 알고리즘 제안 ❌ — 표준만 받습니다.

### 🔧 코드 기여 (PR)
1. Issue 먼저 등록 (큰 변경 시)
2. Fork → branch (`feature/xxx` or `fix/xxx`)
3. 테스트 추가 (필수)
4. PR 제출

## 🧪 테스트 정책

### 새 코드는 반드시 테스트 포함
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() { ... }

    #[test]
    fn edge_case_empty_input() { ... }

    #[test]
    fn rejects_malformed_input() { ... }
}
```

### 보안 관련 코드는 회귀 테스트 필수
- 약한 매개변수 거부 → 모든 케이스
- AEAD 변조 → 거부 확인
- 경로 이탈 → 거부 확인

## 🔒 보안 정책

### 자체 암호 알고리즘 절대 금지

**Kerckhoffs's Principle 준수**: 표준 알고리즘만 사용.

| 영역 | 사용 가능 | 사용 금지 |
|---|---|---|
| AEAD | XChaCha20-Poly1305, AES-GCM-SIV | 자체 변형 |
| KDF | Argon2id, scrypt | 자체 KDF |
| Hash | BLAKE3, SHA-2, SHA-3 | 자체 hash |
| 비대칭 | X25519, Ed25519, ML-KEM | 자체 곡선 |

새 알고리즘 도입 시 RFC/NIST/IETF 표준 필수.

### 의존성 추가 정책

새 crate 추가 전 검토:
- 라이센스 호환 (MIT/Apache/BSD)
- 메인테이너 활성
- 보안 audit 이력
- GPL/LGPL ❌ (정적 링크 시 라이센스 전염)

## 🎨 코드 스타일

### Rust 표준
- `rustfmt` (CI에서 강제)
- `clippy` (warning = error)
- `#[deny(unsafe_code)]` 권장 (FFI 제외)

### 명명
- 함수: `snake_case`
- 타입: `CamelCase`
- 상수: `SCREAMING_CASE`
- 한국어 주석 OK (사용자 친화)

### Zeroize
민감 데이터에 무조건:
```rust
use zeroize::Zeroize;

let mut key = derive_key(...)?;
// ... use key ...
key.zeroize();  // 사용 후 즉시
```

### Constant-time 비교
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

## 📦 PR 체크리스트

PR 제출 전 확인:

- [ ] `cargo fmt` 통과
- [ ] `cargo clippy --all-features` 통과 (no warnings)
- [ ] `cargo test --release` 모두 통과
- [ ] 새 코드에 테스트 추가
- [ ] 보안 관련이면 `docs/THREAT-MODEL.md` 업데이트
- [ ] 알고리즘 변경 시 `docs/CRYPTO-SPEC.md` 업데이트
- [ ] CHANGELOG.md에 [Unreleased] 섹션 추가
- [ ] 새 의존성은 NOTICE.md에 추가

## 🌍 번역 기여

i18n 인프라 구축 후 (예정) 다음 언어 번역 환영:
- 한국어 (기본)
- 영어
- 일본어
- 중국어 (간/번)
- 스페인어
- 프랑스어
- 독일어
- 포르투갈어

## 💬 커뮤니티

- GitHub Issues: 버그/기능
- GitHub Discussions: 일반 토론
- 향후 Matrix/Discord 서버

## 📜 라이센스

기여하신 코드는 **MIT OR Apache-2.0** 듀얼 라이센스로 배포됩니다.
PR 제출은 이 라이센스에 동의함을 의미합니다.

별도 CLA(Contributor License Agreement)는 요구하지 않습니다.

## 🙏 감사합니다

qsafe를 더 안전하게 만드는 데 기여해 주셔서 감사합니다.
