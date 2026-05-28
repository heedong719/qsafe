#!/bin/sh
# macOS Finder 통합 — .qs 더블 클릭 시 qsafe-gui 자동 실행
#
# 권장: tauri-bundler로 정식 .app bundle을 만들고 macOS가 자동 처리하게 함.
# 이 스크립트는 .app이 없는 dev/portable 환경에서 LSRegisterURL + duti로 매핑.
#
# 사용:
#   sh install-macos.sh                # qsafe.app을 ~/Applications/에 두고 .qs를 매핑
#   sh install-macos.sh --uninstall    # association 제거
#
# 전제 조건:
#   - tauri-bundler로 만든 qsafe.app 이 있어야 함 (Tauri의 bundle.fileAssociations로 UTI 자동 등록).
#   - 또는 portable 바이너리만 있으면 file association은 동작하지 않고, qsafe-gui 직접 실행만 가능.

set -e

UNINSTALL=0
for arg in "$@"; do
    case "$arg" in
        --uninstall) UNINSTALL=1 ;;
        --help|-h)
            sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
    esac
done

APP_NAME="qsafe.app"
APP_PATH_USER="$HOME/Applications/$APP_NAME"
APP_PATH_SYSTEM="/Applications/$APP_NAME"

if [ "$UNINSTALL" = "1" ]; then
    echo "qsafe macOS 통합 제거 중…"
    [ -d "$APP_PATH_USER" ] && rm -rf "$APP_PATH_USER" && echo "  ✓ $APP_PATH_USER"
    [ -d "$APP_PATH_SYSTEM" ] && rm -rf "$APP_PATH_SYSTEM" && echo "  ✓ $APP_PATH_SYSTEM"
    # Launch Services 캐시 재구성
    /System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister \
        -kill -seed 2>/dev/null || true
    echo "제거 완료."
    exit 0
fi

echo "qsafe macOS Finder 통합…"

# 1) .app 찾기 (tauri-bundler 결과물 또는 사용자 제공)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CANDIDATES="$SCRIPT_DIR/../../../target/release/bundle/macos/$APP_NAME \
            $SCRIPT_DIR/../../../target/aarch64-apple-darwin/release/bundle/macos/$APP_NAME \
            $SCRIPT_DIR/../../../target/x86_64-apple-darwin/release/bundle/macos/$APP_NAME"
APP_SRC=""
for c in $CANDIDATES; do
    if [ -d "$c" ]; then
        APP_SRC="$c"
        break
    fi
done

if [ -z "$APP_SRC" ]; then
    echo "  ⚠ $APP_NAME 미발견."
    echo "    먼저 'cargo tauri build' 또는 'cargo bundle --release -p qsafe-gui'로 .app 생성 필요."
    echo "    또는 portable 모드로 사용하려면 'cargo run --release -p qsafe-gui' 직접 실행."
    exit 1
fi

# 2) ~/Applications/ 로 복사 (sudo 불필요)
mkdir -p "$HOME/Applications"
rm -rf "$APP_PATH_USER"
cp -R "$APP_SRC" "$APP_PATH_USER"
echo "  ✓ $APP_PATH_USER"

# 3) Launch Services에 등록 — Tauri 가 Info.plist의 UTI/CFBundleDocumentTypes를 자동 채워줌
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister \
    -f "$APP_PATH_USER" 2>/dev/null && echo "  ✓ Launch Services 등록" || true

# 4) Gatekeeper quarantine attribute 제거 (codesign 없는 dev build용)
xattr -dr com.apple.quarantine "$APP_PATH_USER" 2>/dev/null || true

echo
echo "설치 완료. Finder에서 .qs 더블 클릭 → qsafe-gui 자동 실행."
echo "처음 실행 시 Gatekeeper가 차단할 수 있음 — 우클릭 → 열기 → 확인."
