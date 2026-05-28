# conversations/ — development transcripts

이 디렉토리는 qsafe v0.1.0 → v0.1.6 개발 과정의 **Claude Code 세션
transcript** 스냅샷입니다. 사용자 명시적 요청에 의해 보존됩니다.

## 파일

| 파일 | 시점 | 메시지 수 | 크기 | 비고 |
|---|---|---|---|---|
| `09445ea6-...jsonl` | 2026-05-25 ~ 2026-05-26 | 212 | 437 KB | 초기 작업 |
| `e5aa3e5f-...jsonl` | 2026-05-27 | 3391 | 7.4 MB | v0.1.0 → v0.1.5 본격 개발 |
| `e95793d4-...jsonl` | 2026-05-27 ~ 2026-05-28 | 3177 | 6.1 MB | v0.1.5 → v0.1.6 review 사이클 |

## ⚠️ 보안 / Privacy 경고

- **테스트 패스워드 평문 다수 포함** (`testpw`, `sfxpw`, `newpw456`,
  `hunter2`, `r9pw`, `mypw` 등). 모두 임시 테스트용이며 실제 키와는
  무관하지만, **패스워드 패턴이 영구 공개**된다는 점에 유의.
- 임시 디렉토리 경로 (`/tmp/qs-*`) 다수 포함. cleanup 후 sensitive
  아니지만 파일 시스템 layout 추론 가능.
- 개발 의사결정 / 시행착오 / 실패한 시도 그대로 보존. **production
  코드의 단순 history와 다름**.

## 향후 정책

- 새 세션 transcript는 **자동으로 commit되지 않습니다**. `.gitignore`
  의 `conversations/*.jsonl` 패턴으로 자동 추가 차단.
- 이 디렉토리의 transcript는 일회성 스냅샷으로 v0.1.6 시점 영구
  보존. 이후 세션은 별도 외부 백업으로.

## 포맷

각 `.jsonl`은 한 줄에 한 메시지 (JSONL). Claude Code CLI의 내부
포맷이며, 외부 도구 호환은 보장되지 않습니다.
