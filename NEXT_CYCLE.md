# 다음 사이클 — 무한 반복 분석 재개 지점

다른 컴퓨터 / 다음 세션에서 이 파일을 읽고 바로 R47 부터 이어서 진행.

## 즉시 재개 명령

```bash
git clone https://github.com/heedong719/qsafe.git
cd qsafe

# (필요 시) Rust + Linux deps
curl https://sh.rustup.rs -sSf | sh
# Ubuntu/Debian: sudo apt-get install pkg-config libssl-dev libudev-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev

# 빌드 검증 — 145 tests 통과해야 정상
cargo test --workspace --release --all-features
cargo run --release -p qsafe-gui

# Claude Code 에서:
# "무한 반복 분석 계속 진행해"
```

## 현재 상태 스냅샷 (R46 종료 시점 — 2회차 3 연속 0 findings 달성)

| 항목 | 값 |
|---|---|
| 최신 commit | R46 audit + docs sync |
| 워크스페이스 버전 | **0.1.9** (Cargo.toml + tauri.conf.json) |
| Workspace tests | **145 / 0** |
| 8 locale i18n 키 / locale | **236+ 균일** (R28 audit 통과) |
| 사용자 가시 한국어 미국제 텍스트 | **0건** |
| Tauri bundler | active=true, CI 자동 4-OS native installer |
| OS 통합 | Linux/macOS/Windows 모두 인스톨러 자동 등록 |
| Linux thumbnailer | ✓ |
| macOS .qlgenerator | ✗ (대기) |
| Windows ThumbnailProvider | ✗ (대기) |
| codesign / notarization | ✗ (외부 자원 필요) |

## 사용자 요청 큐 — 모두 완료

| Q | 항목 | 완료 |
|---|---|---|
| Q1 | 압축 파일 더블 클릭 + 풀기 버튼 | R1 |
| Q2 | 모달 내 더블 클릭 + 단일 추출 + 실행 | R7 |
| Q3 | 모달 닫기 + 임시 cleanup | R7 |
| Q4 | 압축/풀기 진행 % 팝업 | R9 + R10 |
| Q5 | 8-locale native 번역 | R6 + R11 + R12 + R13 + R20 + R25 |
| 다국어 "절대 필수" | 풀스택 i18n | R6~R13 + R20 + R25 |
| ISO 마운트 + USB 굽기 | "Rufus 보다 좋게" | v0.1.6 + R4 + R26 가드 |
| OS 자동 등록 | "탐색기 / Finder / Linux" | R4 + R5 + R8 + R14 |

## R28+ 큐 (우선순위)

### 🟢 단기 (한 사이클씩 처리 가능)
1. **v0.1.9 태그 push** — 사용자 트리거 필요: `git tag v0.1.9 && git push origin v0.1.9` → CI 자동 release
2. **Tab / Arrow key navigation** — 파일 리스트 키보드 탐색 (Up/Down/Tab focus, Enter activate)
3. **Multi-select** — Shift/Cmd-Click 다중 선택 + 일괄 작업 (압축/삭제)
4. **modal-iso / modal-usb 동적 메시지** 잔여 i18n 점검
5. **파일 리스트 가상 스크롤** — 10000+ 항목 폴더에서 성능
6. **압축 / 풀기 cancel 버튼** — pack/unpack 중 child kill

### 🟡 중기 (별도 crate 또는 큰 구조 변경)
7. **macOS .qlgenerator Quick Look plugin** — Objective-C bundle 또는 Rust objc bridge. .qs 헤더 미리보기.
8. **Windows Shell ThumbnailProvider** — COM DLL (IThumbnailProvider). .qs 썸네일.
9. **자동 업데이트 인스톨 트리거** — R16+R17 알림만 → 클릭 시 자동 다운로드 + 설치 (Sparkle / WinSparkle)
10. **백업/복원 자동화** — identity / config 자동 백업 모달

### 🔴 장기 (외부 자원 또는 큰 작업)
11. **codesign / notarization** — Apple Developer ID + Windows EV cert (유료)
12. **외부 보안 감사** — Trail of Bits / NCC Group (v1.0 전)
13. **자체 빌드 시스템 점검** — 매 release에서 `cargo deny` + supply chain audit

## 최근 누적 사이클 (R1~R46)

```
R44~R46 audit loop (2회차) — 모두 0 finding → stop
858f98e R43  🔴🔴 5건 silent ID typo (OS 통합 흐름 복구)
95b77ce R42  🔴 sort silent bug fix (renderEntries → renderFileList)
153c8b9 R41  가상 모드 키보드 nav scroll
a377403 R40  가상 스크롤
350d05a R39  키보드 multi-select (Shift+Arrow + Cmd-A)
a24ff53 R38  audit stop (1회차, silent bug 못 잡음 입증됨)
2752962 R35  i18n (unpack 결과 + 3 dialog + sort + overwrite)
93f75a0 R34  i18n (pack 결과 + 2 dialog + entry tooltip)
038c1fe R33  docs sync (R29-R32 entries)
6c565cd R32  다중 pack (pack_multiple_to_qsafe)
494b6bd R31  Multi-select + 다중 삭제
ec9fa36 R30  cancel 버튼 (pack/unpack)
da5ccd8 R29  키보드 navigation
2058756      R28 docs sync
4414a37 R28  i18n locale 일관성 audit (6 locale × 79 키 native)
cc03b5f      transcript snapshot
f56d668 R27  v0.1.9 release cut + CHANGELOG R22-R26
c523b06 R26  TOCTOU 가드 (run_iso_write)
847a8d3 R25  R24 follow-up
6066b07 R24  파일 검색/필터
a5172e4 R23  RAR archive bomb 가드
13617d9 R22  메모리 unbounded growth 가드
b4a56d3 R21  v0.1.8 release cut
b52bf8c R20+ 잔여 i18n 정리
d6482e2 R20  modal-info 헤더 i18n
58856ab R19  우클릭 컨텍스트 메뉴
0dff060 R18  F1 단축키 cheatsheet
9ce23a0 R17  silent startup update check
3b7c886 R16  About update 버튼
7a4bad6 R15  v0.1.7 release cut
7b37b42 R14  Tauri bundler + native CI
43ffb41 R13  동적 JS i18n
8d68073 R12  modal-key/mnemonic i18n
1977f4a R11  modal-pack/unpack i18n
fc841b7 R10  Q4 GUI progress bar
3ff942a R9   Q4 CLI --progress
27a9f4e R8   Linux .thumbnailer
01a4fb7      transcript snapshot
1471e7c R7   Q2+Q3 dblclick + cleanup
d0dada2 R6   docs sync + i18n about
45f51cc R5   startup argv routing
a9e2b71 R4   OS desktop integration
35188e5 R3   About redesign
38f1075 R2   drag&drop + 단축키 + 정렬
503ea14 R1   modal-info 풀기 버튼
```

## 검증 명령 (다음 세션에서 즉시 실행)

```bash
# 회귀 없음 확인
cargo build --release
cargo fmt --check
cargo clippy --workspace --all-features --no-deps -- -D warnings
cargo test --workspace --release --all-features
cargo deny check advisories

# GUI 실행 (모든 R1~R26 사용자 가시 기능 확인)
cargo run --release -p qsafe-gui
# ✓ 파일 더블 클릭 → modal-info + 풀기 버튼 (R1)
# ✓ 파일 드래그-드롭 → 모달 자동 라우팅 (R2)
# ✓ Cmd-N / Cmd-R / F5 / Backspace / Esc / Cmd-, / Cmd-F / F1 (R2 + R18 + R24)
# ✓ 컬럼 헤더 클릭 정렬 (R2)
# ✓ About 모달 (R3) + 업데이트 확인 (R16) + statusbar 배지 (R17)
# ✓ OS 자동 등록 (인스톨러 사용 시) (R4 + R14)
# ✓ 우클릭 컨텍스트 메뉴 (R19)
# ✓ 파일 검색/필터 (R24)
# ✓ 진행률 바 (큰 파일 pack/unpack 시) (R10)
# ✓ 8 locale 전환 (Cmd-,)
```

## 메모

- 모든 시크릿 파일 (identity 등) 은 0600 권한 + atomic write 보장 (v0.1.6).
- `qsafe pack ... --progress` flag 가 stderr에 `PROGRESS\tcur\ttot\tpct` 라인 출력 (R9).
- 자식 stderr / ureq 응답에 256 KB 캡, RAR entry 추출에 2 GiB 캡 (R22 + R23).
- ISO 디스크 쓰기에 3-단계 confirm + TOCTOU 재검증 (R26).
- conversations/ 의 jsonl 3개는 R1~R27 전체 대화 — 다음 세션에서 컨텍스트 복원에 사용.

Claude는 완벽하지 않습니다. 중요한 결정은 항상 확인하세요.
