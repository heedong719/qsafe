# .bkit-snapshot/ — bkit Vibecoding Kit state snapshot

qsafe v0.1.6 시점의 **bkit Vibecoding Kit 플러그인 상태** 스냅샷.
사용자 명시적 요청에 의해 보존됩니다.

## 파일

| 경로 | 내용 |
|---|---|
| `audit/2026-05-27.jsonl` | 5/27 모든 file modification + command 실행 audit log (604 KB) |
| `audit/2026-05-28.jsonl` | 5/28 audit log (5 KB) |
| `state/pdca-status.json` | PDCA workflow 상태 |
| `state/memory.json` | bkit 에이전트 메모리 |
| `state/session-history.json` | 세션 이력 |
| `runtime/token-ledger.ndjson` | 토큰 사용 ledger (14 KB) |
| `runtime/control-state.json` | Trust level / automation state |
| `runtime/hook-reachability.json` | hook 도달 가능성 모니터링 |

## 출처

원래는 `/Users/heedong719/Project/40. Heedong719/utils/.bkit/` (이
qsafe repo의 **부모** 디렉토리). qsafe만의 작업이 아닌 utils/ 전체에
대한 bkit 상태이지만, 5/27~5/28에 해당하는 audit log는 거의 모두
qsafe 작업이라 함께 보존.

## ⚠️ 주의

- audit log는 모든 file write / Bash 명령을 기록 → **시행착오 및
  실행한 모든 명령이 그대로 공개**됨.
- token-ledger는 토큰 사용량 + 작업 패턴 정보 노출.
- 새 bkit 상태는 **자동으로 commit되지 않음** (`.bkit/` 자체는
  utils/에 있고 qsafe repo 밖이라 git이 추적 안 함).
