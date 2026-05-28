# OS 자동 등록 — qsafe Desktop Integration

설치 스크립트로 qsafe-gui를 OS 탐색기에 통합. `.qs` / `.iso` 파일 더블 클릭 시 자동 실행, 우클릭 메뉴, 아이콘.

## Linux (XDG 표준)

```sh
# 사용자만 (~/.local) — sudo 불필요
sh install-linux.sh --user

# 시스템 전역 (/usr/local + /usr/share)
sudo sh install-linux.sh

# 제거
sh install-linux.sh --uninstall
```

**무엇이 등록되나**:
- `~/.local/share/applications/qsafe.desktop` — 데스크톱 엔트리, 8개 언어 GenericName/Comment
- `~/.local/share/mime/packages/qsafe.xml` — `application/x-qsafe` MIME (magic bytes `QSAFE001` 자동 감지)
- `~/.local/share/icons/hicolor/256x256/apps/qsafe.png` — 아이콘 (hicolor 표준)
- `update-mime-database` / `update-desktop-database` / `gtk-update-icon-cache` 자동 호출

**검증**:
- Nautilus/Dolphin/Thunar에서 .qs 우클릭 → 기본 앱이 qsafe
- `xdg-mime query default application/x-qsafe` → `qsafe.desktop`

## macOS (Launch Services)

```sh
# .app bundle 위치 자동 감지 → ~/Applications/qsafe.app
sh install-macos.sh

# 제거
sh install-macos.sh --uninstall
```

**전제 조건**:
- `cargo tauri build` 또는 `cargo bundle` 로 `qsafe.app` 먼저 생성. `tauri.conf.json`의 `bundle.fileAssociations`가 자동으로 Info.plist에 UTI/CFBundleDocumentTypes 추가.

**무엇이 등록되나**:
- `qsafe.app` 의 Info.plist 에 `UTExportedTypeDeclarations` + `CFBundleDocumentTypes` (Tauri가 생성)
- `lsregister`로 Launch Services에 강제 등록
- Gatekeeper quarantine 제거 (dev build 한정)

**검증**:
- Finder에서 .qs 정보 보기 → "다음으로 열기" 기본값이 qsafe
- `mdls -name kMDItemContentType file.qs` → `app.qsafe.gui.qs`

⚠️ codesign / notarization 없으면 첫 실행 시 우클릭 → 열기 → 확인 필요.

## Windows (Registry)

```powershell
# 현재 사용자만 (HKCU) — admin 불필요
.\install-windows.ps1 -User

# 시스템 전역 (HKLM) — admin 필요
.\install-windows.ps1

# 제거
.\install-windows.ps1 -Uninstall
```

**무엇이 등록되나**:
- `Software\Classes\.qs` → `qsafe.qsfile` ProgID
- `qsafe.qsfile\DefaultIcon` → qsafe-gui.exe 첫 아이콘
- `qsafe.qsfile\shell\open\command` → 더블 클릭 매핑
- `Software\Classes\*\shell\qsafe-compress` → 모든 파일 우클릭에 "Compress with qsafe"
- `Software\Classes\Directory\shell\qsafe-compress` → 폴더 우클릭에 "Compress with qsafe"
- `qsafe.qsfile\shell\unpack` → .qs 우클릭에 "Unpack with qsafe"

**검증**:
- 탐색기에서 .qs 더블 클릭 → qsafe-gui 자동 실행
- 우클릭 메뉴에 "Compress with qsafe" / "Unpack with qsafe"

⚠️ Windows 11에서는 우클릭 메뉴가 "더 많은 옵션 표시" 안에 숨어 있을 수 있음 (registry 키 `InprocServer32` 추가 시 표시 가능, 추후 작업).

## 통합 매트릭스

| 기능 | Linux | macOS | Windows |
|---|---|---|---|
| 더블 클릭 → qsafe-gui | ✓ | ✓ (.app 필요) | ✓ |
| 우클릭 "압축" | (Action) | ✗ (Services 별도) | ✓ |
| 우클릭 "풀기" | (Action) | ✗ | ✓ |
| Quick Look 미리보기 | ✗ | (별도 .qlgenerator) | ✗ |
| 썸네일 | (썸네일러 등록 가능) | (별도 thumbnailer) | (별도 ThumbnailProvider) |

위 표의 ✗ / 별도 항목은 후속 사이클에서 OS별 추가 작업.

## 다음 단계 (큐)

- macOS Quick Look plugin (`.qlgenerator`) — `.qs` 헤더 미리보기
- Windows Shell IThumbnailProvider — `.qs` 썸네일
- Linux thumbnailer (`/usr/share/thumbnailers/qsafe.thumbnailer`) — Nautilus 썸네일
- 자동 업데이트 (Sparkle / WinSparkle)
- 코드 사이닝 (Apple Developer ID, Windows EV cert)
