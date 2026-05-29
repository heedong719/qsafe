# CHANGELOG

모든 주요 변경사항을 기록합니다. 형식은 [Keep a Changelog](https://keepachangelog.com/) 준수.

버전 표기는 [SemVer](https://semver.org/) 기반.

## [Unreleased]

(다음 사이클 — 아직 없음)

---

## [0.1.7] — 2026-05-29 (Commercial Polish Cycle, R1~R14)

### Commercial Polish Cycle (R1~R5, post-v0.1.6)

**R1 (503ea14) — modal-info '압축 풀기' 버튼**
- 압축 파일 더블 클릭 → 정보 모달이 열리면 "📂 압축 풀기" 버튼 활성. `.qs` → modal-unpack prefill, 외부 아카이브 → 같은 디렉토리에 즉시 extract.

**R2 (38f1075) — 상용 UX 3종 일괄**
- Drag & Drop: Tauri `tauri://drag-drop` listener — `.qs`/외부 아카이브 → modal-info, `.iso`/`.img`/`.dmg` → modal-iso, 일반 파일 → modal-pack prefill.
- 키보드 단축키: F5/Cmd-R/Ctrl-R 새로고침, Backspace 상위 폴더, Cmd-Comma 언어, Cmd-N 압축. ESC는 기존.
- 컬럼 정렬: 이름/크기/종류/수정 클릭 → ASC/DESC 토글, 폴더 상단 고정.

**R3 (35188e5) — About 재디자인**
- 큰 🔐 글리프 + tagline + 버전 강조 + license/build-with/GitHub 링크 + 저작권. `modal.about.*` 10개 i18n 키 추가.

**R4 (a9e2b71) — OS 자동 등록 (Windows / macOS / Linux)**
- `tauri.conf.json` `bundle.fileAssociations` — `.qs` (MIME `application/x-qsafe`, Editor) + `.iso`/`.img`/`.dmg` (Viewer). Tauri bundler가 macOS Info.plist UTI + Windows MSI/NSIS 자동 생성.
- `crates/qsafe-gui/install/qsafe.desktop` — 8-언어 GenericName/Comment, MimeType 8개 (.qs + 7개 외부 아카이브 + ISO), Desktop Actions ("Compress with qsafe" / "Unpack with qsafe").
- `crates/qsafe-gui/install/qsafe-mime.xml` — shared-mime-info 패키지, `*.qs` glob + magic-byte 룰 (`QSAFE001` at offset 0, priority 80).
- `install-linux.sh` — `--user` (sudo 불필요) / 전역 / `--prefix=PATH` / `--uninstall`. 바이너리 + .desktop + MIME + 아이콘 설치 + 3개 cache update 자동.
- `install-macos.sh` — `cargo tauri build` 산출물 `qsafe.app` 자동 탐색 → `~/Applications/` → `lsregister -f` → quarantine 제거.
- `install-windows.ps1` — Registry ProgID `qsafe.qsfile`, DefaultIcon, shell\open\command, `*\shell\qsafe-compress` + `Directory\shell\qsafe-compress`, `qsafe.qsfile\shell\unpack`. `-User` HKCU (admin 불필요) / 기본 HKLM / `-Uninstall`.

**R5 (45f51cc) — startup argv 라우팅 (R4와 GUI 연결)**
- `StartupArgs` 구조체 + `parse()` — `--action=<verb>` (pack/unpack/info) + 첫 positional path (canonicalize).
- `main.rs` 부팅 시 `std::env::args().skip(1)` 파싱 → Tauri `.manage()`. 새 `startup_args()` command JS 노출.
- `ui/index.html` init() 끝: 파일 매니저로 디렉토리 탐색 OK 후 → 액션별 자동 모달 오픈 (pack/unpack/info/iso, 기본 modal-pack prefill).
- 단위 테스트 6건 추가 (empty / path만 / action만 / `=` 형식 / first positional / unknown flag). 워크스페이스 테스트 133 → **139**.

**문서 동기화 (이번 사이클)**
- `README.md` 빠른 시작 직전에 "OS 자동 등록" 섹션 (3-OS 설치 명령 + 상세 가이드 링크).
- `crates/qsafe-gui/install/README.md` — OS별 설치/검증 명령/통합 매트릭스/다음 단계 큐.

### 후속 마일스톤 (대기)
- 외부 감사 (Trail of Bits / NCC Group) — v1.0 전
- SFX codesign / notarization (macOS Gatekeeper / Windows SmartScreen)
- 정식 Tauri release workflow (icon.ico/.icns + MSI/NSIS/DMG bundle)
- macOS Quick Look plugin (`.qlgenerator`)
- Windows Shell ThumbnailProvider
- Linux `.thumbnailer` (`/usr/share/thumbnailers/qsafe.thumbnailer`)
- 자동 업데이트 (Sparkle / WinSparkle)

### qsafe-gui 후속 마일스톤 (사용자 합의 — R1 완료)
- ✅ ~~압축 파일 더블 클릭 시 팝업에 **압축 풀기 버튼** 추가~~ (R1)
- 팝업 안의 파일 더블 클릭 → 임시 디렉토리에 **단일 파일 추출 + 연결 프로그램 실행**
- 모달 닫기 시 **임시 추출 파일 자동 cleanup**
- 압축/풀기 **진행 % 팝업** (qsafe-cli `--progress` 플래그 + GUI stderr 파싱)
- 8 locale (ja/zh/es/fr/de/it)의 **native speaker 번역 보강** (현재 핵심만 + 영어 fallback)

### qsafe-gui 다국어 지원 — Phase 1+2 (post-a093a06 main update)
- 새 `ui/locales/` 디렉토리 + 8개 locale JSON: `ko, en, ja, zh, es, fr, de, it`.
  - `ko` / `en`: 모든 키 완전 번역 (~90 키, app/toolbar/modal/result/error 카테고리).
  - 나머지 6개: `_meta` (native 이름) + 핵심 (toolbar/modal title/result) 번역, 미번역 키는 `en` fallback.
- 새 `ui/i18n.js` loader (의존성 0, vanilla JS, ~120 줄):
  - `localStorage` → `navigator.language` 자동 감지 (`navigator.language.split('-')[0]`).
  - `data-i18n="key"` / `data-i18n-title="key"` / `data-i18n-placeholder="key"` 속성을 DOM 적용.
  - `{placeholder}` 보간 (`progress.percent`, `status.items` 등).
  - `setLocale(code)` → `localStorage` 저장 + `applyDom()` + `qsafe-locale-changed` 이벤트 dispatch.
- `index.html` 통합:
  - `<html lang>` / `<title>` 자동 갱신.
  - titlebar, toolbar, filelist 헤더, delete 모달, 새 language 모달에 `data-i18n` 마킹.
  - 도구바에 **🌐 언어** 버튼 추가 → 모달에서 8개 native 이름으로 선택.
- ⚠️ 점진적 마이그레이션: modal-pack / modal-unpack / modal-info 내부 텍스트와 JS 동적 텍스트의 일부는 다음 사이클에 처리. 영어 fallback이 작동하므로 미번역 키도 깨지지 않음.

### v0.1.6 직후 post-release 정리 (untagged main updates)
- `tests/e2e.rs`: 새 통합 테스트 2건 (총 10 → 12) — `identity_generate_show_export_roundtrip` (0600 권한 + fingerprint 일치 검증), `pack_unpack_with_pubkey_recipient` (X25519+ML-KEM-768 라운드트립).
- `install.sh`: Windows에서 "install.ps1 권장" 안내가 misleading (스크립트 없음) → "Releases에서 .zip 직접 다운로드 + PATH 추가"로 수정. PowerShell 스크립트가 추가될 때 다시 갱신.

### qsafe-gui 전면 재구축 (`626cd0a`, untagged main update)

**Windows 탐색기 스타일 GUI로 완전 재작성**:
- 좌측 드라이브/홈 트리 + 우측 파일 리스트 (이름/크기/종류/수정 날짜 컬럼).
- 주소바, 뒤로/위로/홈/새로고침 네비, 폴더 더블 클릭 진입, 시작 폴더 = current working directory.
- 모달 다이얼로그 (압축/풀기/삭제/정보).

**압축/풀기 통합** (qsafe CLI shell-out):
- 4가지 모드: `qsafe-open` (`PUBLIC_PASSWORD=qsafe-public-v1` 자동) / qsafe + password / qsafe + 친구 공개키 / qsafe + 둘 다 / 표준 ZIP.
- 폴더 입력 시 자동 `tar` → 풀 때 tar magic 감지 → 자동 untar로 원본 폴더 복원.
- 압축 알고리즘 선택 (auto/zstd/none), 보안 강도 (standard 64MiB / strong 256MiB), `--sfx` 옵션.
- **MD5 사이드카**: 압축 시 원본 MD5 → `<out>.qs.md5` 자동 생성, 풀 때 자동 비교.
- 결과 모달: 경과 시간, 압축률 + 절약 용량, MD5/SFX 경로 표시.

**탐색기 부가 기능**:
- 삭제 (안전 가드: 드라이브 루트 / 시스템 폴더 `C:\Windows`, `/usr`, `/etc` 등 거부).
- 일반 파일 더블 클릭 → 연결된 프로그램 실행 (`cmd start` / `open` / `xdg-open`).
- `.qs` / 외부 압축파일 더블 클릭 → 헤더 / 내부 목록 모달 (ZIP/7Z/RAR/TAR/GZ/XZ/BZ2/LZ4/ZSTD/BR).

**Tauri 2 commands 22개** + **qsafe-gui 단위 테스트 36개** (총 workspace tests 97 → **133**).

**의존성 추가**: `tauri-plugin-dialog 2.7`, `md-5 0.10`, `tar`, `zip` (workspace).

**자산 추가**: `icons/icon.ico` (Windows tauri-build 요구사항 해결), 실제 `icon.png` (1223 bytes), `capabilities/default.json`.

**SECURITY.md 보강** (v0.1.6+ qsafe-gui):
- `qsafe-gui process argv 노출`: GUI가 `qsafe-cli`에 패스워드를 `--password` argv로 전달 → 다른 사용자가 `ps`로 볼 가능성. 후속에서 stdin pipe로 마이그레이션 예정.
- `qsafe-gui "open mode" / PUBLIC_PASSWORD`: "암호 없는" 모드는 고정 상수 `qsafe-public-v1` 사용 → plaintext와 동등. 진짜 비밀은 사용자 패스워드 또는 `--pubkey` / `--fido2` 사용.

### 추가 분석 사이클 (commit 미반영 main updates)
- qsafe-gui clippy 위반 6건 fix (`vec_init_then_push`, `needless_borrow`, `too_many_arguments` allow, `redundant_closure` 2건, `manual_char_comparison`).
- `cargo fmt` 적용.

### v0.1.6 → v0.1.7 추가 (R6~R14)

### 추가 — R6 (d0dada2) 문서 동기화 + 8-locale i18n about
- `README.md` 빠른 시작 직전에 OS 자동 등록 섹션. 3-OS 설치 명령 + `crates/qsafe-gui/install/README.md` 링크.
- `modal.about.*` 10 키를 6개 locale (ja/zh/es/fr/de/it) 에 native 추가. 이전엔 en fallback.

### 추가 — R7 (1471e7c) Q2 + Q3 단일 추출 + 자동 cleanup
- `qsafe-formats::rar::extract_rar_entry(rar, entry_name, base, password) -> PathBuf` 신규 — 전체 archive walk 후 일치하는 entry만 extract_to.
- `qsafe-gui::commands::extract_archive_entry_to_temp` Tauri command — `$TMP/qsafe-info-<pid>-<nanos>/` (Unix 0700) 에 단일 entry 추출.
- `qsafe-gui::commands::cleanup_temp_dir` Tauri command — canonicalize starts_with temp_dir + file_name starts_with `qsafe-info-` 이중 guard.
- UI: m-info-list rows에 dblclick 핸들러 — clickable 행은 임시 추출 + open_with_associated 실행. closeModal('modal-info') 가 infoTempDirs 큐 drain.
- 단위 테스트 3건 추가 (cleanup_temp_dir reject /tmp / reject / / accept qsafe-info-*). 워크스페이스 tests 139 → 142.

### 추가 — R8 (27a9f4e) Linux 썸네일 통합
- `qsafe thumbnail INPUT OUTPUT --size N` 새 CLI 서브커맨드 — magic byte 검사 후 임베디드 256x256 lock PNG 출력 (qsafe-gui/icons/icon.png include_bytes!).
- `install/qsafe.thumbnailer` XDG hook (TryExec=qsafe / Exec=qsafe thumbnail %i %o --size %s / MimeType=application/x-qsafe).
- `install-linux.sh` 가 `$DATA_DIR/thumbnailers/` 에 자동 설치 + uninstall.
- 단위 테스트 2건 (thumbnail_rejects_non_qsafe, thumbnail_writes_png_for_valid_qsafe). 워크스페이스 tests 142 → 144.

### 추가 — R9 (3ff942a) + R10 (fc841b7) Q4 풀스택 진행률
- **R9**: `qsafe-core::stream` 에 `stream_encrypt_with_hash_progress(..., FnMut(u64))` + `stream_decrypt_with_hash_progress(..., FnMut(u32, u64))`. 기존 함수는 wrapper 로 유지. `qsafe-cli pack/unpack` 에 `--progress` flag — chunk 후 callback 이 stderr에 `PROGRESS\tcur\ttot\tpct` 출력 (% 변동 시에만 dedupe).
- **R10**: `qsafe-gui::commands::spawn_with_progress(cmd, app: Option<&AppHandle>, event_name)` 헬퍼. child stderr → BufReader::lines() → PROGRESS 라인 파싱 → `PackUnpackProgress { current, total, percent }` Tauri event emit. pack_path_ext / unpack_qsafe / unpack_qsafe_ext 를 `*_impl(Option<&AppHandle>, ...)` 패턴으로 분리 (기존 테스트가 AppHandle 없이 호출 가능). UI: 18px `.progress.determinate .bar` width = pct, `.pct` 라벨 = "{pct}%", `applyProgress` / `resetProgress` 헬퍼.

### 추가 — R11 (1977f4a) modal-pack / modal-unpack 완전 i18n
- HTML 의 ~50개 정적 label/button/radio/select/checkbox 에 `data-i18n` 마킹.
- 49개 새 키 (`modal.pack.heading` / `.browse` / `.password_hint` / `.pubkey_file` / `.advanced` / `.sfx_warn` / `.go` / `.mode.{open,password,pubkey,both,zip}` / `.compression.{auto,zstd,none}` / `.profile.{standard,strong}` / `.opt.{sfx,md5,label}` / `.unpack.heading` / `.browse` / `.mode.{open,password,identity}` / `.secret_file` / `.go`) 8개 locale 모두 native.

### 추가 — R12 (8d68073) modal-key + modal-mnemonic 완전 i18n
- 14개 새 키 8개 locale 모두 native (modal.key.* + modal.mnemonic.*).

### 추가 — R13 (43ffb41) 동적 JS 문자열 i18n + tErr 헬퍼
- `i18n.js` 에 `qsafeI18n.tErr(key, e, vars)` 헬퍼 추가 — 다국어 prefix + e.message 결합.
- 26개 `showErr` / `setResult` / `setStatus` 호출을 `tErr` / `t` 로 변환.
- 22개 새 키 (error.{loading,open_folder,open_file,up_failed,pick_failed,extract_single,extract_external,drop_failed,mnemonic,lang_load,write_in_progress,write_start,write_failed,init_invoke,init_about,init_sidebar,init_currdir,init_navigate,init_home_fallback} + status.{ready,fallback_home} + result.external_archive with {format}/{count} 보간) 8개 locale 모두 native.

### 빌드 / 릴리스 — R14 (7b37b42) Tauri bundler 활성화
- `tauri.conf.json` `bundle.active: false → true`, icons 배열에 .png + .ico + **.icns** (iconutil 로 11개 size 임베드), category=Utility, shortDescription/longDescription 추가.
- `.github/workflows/release.yml` 에 새 `build-gui` matrix job 추가 (macOS arm64/x86_64 + Windows + Linux). 각 runner 에서 `cargo install tauri-cli` + `cargo tauri build --target <triple>` → `.dmg / .msi / .nsis / .deb / .AppImage` 자동 생성.
- Linux runner 에 libwebkit2gtk-4.1-dev + libayatana-appindicator3-dev + librsvg2-dev + libudev-dev 사전 설치.
- `release` job 의 needs 가 `[build, build-gui]` 로 확장 — 두 matrix 모두 통과해야 release 발사. `files:` glob 에 `.dmg/.msi/.exe/.deb/.AppImage` 추가.

### 후속 마일스톤 (대기)
- 외부 감사 (Trail of Bits / NCC Group) — v1.0 전
- SFX codesign / notarization (Apple Developer ID / Windows EV cert)
- macOS .qlgenerator Quick Look plugin
- Windows Shell ThumbnailProvider (COM DLL)
- 자동 업데이트 (Sparkle / WinSparkle)

---

## [0.1.6] — 2026-05-28 (v0.1.5 후속 분석 — secret file 권한 강화, CI 안정화)

### 🔒 보안

- **secret 파일 0600 권한 강제** (R6, 큰 발견):
  - `qsafe identity generate`: 이미 `write_atomic` 사용 (0600+O_NOFOLLOW+O_EXCL)
  - `qsafe identity export-pubkey`: `fs::write` → `write_atomic` 으로 일관 적용
  - SFX stub의 풀린 파일: `write_secret_file` 헬퍼 추가 (0600, `create_new`)
  - qsafe-gui `identity_generate`: `write_secret_json` 헬퍼 추가 (0600, `create_new`)
  - Windows에서는 NTFS ACL 기본을 따름 (`create_new`만 적용).

- **SECURITY.md에 SFX/Pubkey 신뢰 모델 명시** (R4):
  - SFX: payload는 AEAD 보호되지만 stub은 변조될 수 있다는 한계, codesign/notarization 권장
  - Pubkey: transcript MitM 방어, X25519+ML-KEM-768 하이브리드 안전성, PFS, identity 파일 보호

### 🔧 CI / 인프라

- **Windows CI fail 해소** (R1):
  - v0.1.5의 ubuntu CI는 통과했지만 windows에서 `tauri-build`가 `icons/icon.ico` 부재로 fail.
  - 해결: CI test/MSRV/clippy/build/test 모두 `--workspace --exclude qsafe-gui`로 분리. Tauri 빌드는 별도 release workflow 책임.
  - Linux Tauri WebKit deps (`webkit2gtk-4.1` 등)도 CI matrix에서 제거 (gui 미빌드).

- **release.yml에 qsafe-stub 4-target 빌드 추가** (R2):
  - `cargo build --release --target ${target} -p qsafe-stub` + Linux strip 추가.
  - dist 아카이브에 `qsafe-stub${ext}` 포함 → `qsafe pack --sfx`가 같은 디렉토리의 stub 자동 발견.
  - CHANGELOG.md도 dist에 포함.

- **install.sh에 qsafe-stub 자동 설치** (R3):
  - `qsafe` + `qsafe-stub` 동시 설치 (sudo 한 번).
  - 새 사용법 가이드 출력: `pack --sfx`, `pack --pubkey`, `identity generate`.

### 🧹 코드 품질 / UX

- **qsafe-stub UX 개선** (R10):
  - 사용자가 standalone `qsafe-stub`을 직접 실행한 경우 `NotSfx` 에러를 친절한 안내로 보완.
  - `--version` / `--help` flag 추가 (이전: SFX 풀기를 무조건 시도하던 confusing 동작).

- clippy `needless_return` 위반 1건 해소 (cfg(unix) 블록의 explicit `return`).
- `cargo fmt` 자동 적용.

### 검증 (14 라운드, R12/R13/R14 3번 연속 0건)

- cargo build --workspace --release --all-features: clean
- cargo fmt --check: clean
- cargo clippy --workspace --all-features --no-deps -- -D warnings: clean
- cargo test --workspace --release --all-features: 95 / 0
- cargo deny check: advisories ok, bans ok, licenses ok, sources ok
- cargo doc --workspace --no-deps: no warnings
- **e2e 매트릭스 검증**: (1) password pack/unpack, (2) X25519+ML-KEM-768 PQ 하이브리드 pack/unpack, (3) SFX 자기압축해제 1.3MB `.run` → 풀기 — 모두 SHA256 일치.

---

## [0.1.5] — 2026-05-28 (CLI Pubkey + SFX 자기압축해제 + GUI MVP scaffold)

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
