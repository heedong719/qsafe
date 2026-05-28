#!/bin/sh
# Linux 데스크톱 통합 설치 스크립트 (XDG 표준)
#
# 무엇을 하는가:
#   1. qsafe / qsafe-stub / qsafe-gui 바이너리를 $PREFIX/bin (기본 /usr/local/bin)에 설치
#   2. qsafe.desktop 을 $XDG_DATA_DIRS/applications (기본 /usr/share/applications) 에 등록
#   3. qsafe-mime.xml 을 $XDG_DATA_DIRS/mime/packages 에 등록 → update-mime-database
#   4. 아이콘을 $XDG_DATA_DIRS/icons/hicolor/256x256/apps/qsafe.png 에 등록 → gtk-update-icon-cache
#
# 사용:
#   sudo sh install-linux.sh                       # 전역 (/usr/local + /usr/share)
#   sh install-linux.sh --user                     # 사용자만 (~/.local)
#   sh install-linux.sh --prefix=/opt              # 커스텀 prefix
#   sh install-linux.sh --uninstall                # 제거

set -e

# 기본 설정
USER_MODE=0
UNINSTALL=0
BIN_DIR_GLOBAL=/usr/local/bin
DATA_DIR_GLOBAL=/usr/share
BIN_DIR_USER="$HOME/.local/bin"
DATA_DIR_USER="$HOME/.local/share"

# 옵션 파싱
for arg in "$@"; do
    case "$arg" in
        --user) USER_MODE=1 ;;
        --uninstall) UNINSTALL=1 ;;
        --prefix=*) BIN_DIR_GLOBAL="${arg#--prefix=}/bin"; DATA_DIR_GLOBAL="${arg#--prefix=}/share" ;;
        --help|-h)
            sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
    esac
done

# 모드별 경로 결정
if [ "$USER_MODE" = "1" ]; then
    BIN_DIR="$BIN_DIR_USER"
    DATA_DIR="$DATA_DIR_USER"
else
    BIN_DIR="$BIN_DIR_GLOBAL"
    DATA_DIR="$DATA_DIR_GLOBAL"
    if [ "$(id -u)" != "0" ] && [ "$UNINSTALL" != "1" ]; then
        echo "전역 설치는 root 권한 필요 (또는 --user 사용)" >&2
        exit 1
    fi
fi

APPS_DIR="$DATA_DIR/applications"
MIME_DIR="$DATA_DIR/mime/packages"
ICON_DIR="$DATA_DIR/icons/hicolor/256x256/apps"

DESKTOP_FILE="$APPS_DIR/qsafe.desktop"
MIME_FILE="$MIME_DIR/qsafe.xml"
ICON_FILE="$ICON_DIR/qsafe.png"

if [ "$UNINSTALL" = "1" ]; then
    echo "qsafe 데스크톱 통합 제거 중…"
    rm -f "$DESKTOP_FILE" "$MIME_FILE" "$ICON_FILE"
    rm -f "$BIN_DIR/qsafe" "$BIN_DIR/qsafe-stub" "$BIN_DIR/qsafe-gui"
    update-mime-database "$DATA_DIR/mime" 2>/dev/null || true
    update-desktop-database "$APPS_DIR" 2>/dev/null || true
    gtk-update-icon-cache "$DATA_DIR/icons/hicolor" 2>/dev/null || true
    echo "제거 완료."
    exit 0
fi

echo "qsafe Linux 설치 시작…"
echo "  bin:  $BIN_DIR"
echo "  data: $DATA_DIR"

# 1) 바이너리 설치 — 빌드 결과물이 ./target/release/ 또는 인접 경로에 있어야 함
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SEARCH_DIRS="$SCRIPT_DIR/../../../target/release $SCRIPT_DIR ./target/release ."
mkdir -p "$BIN_DIR"
for bin in qsafe qsafe-stub qsafe-gui; do
    found=""
    for d in $SEARCH_DIRS; do
        if [ -f "$d/$bin" ]; then
            found="$d/$bin"
            break
        fi
    done
    if [ -n "$found" ]; then
        cp "$found" "$BIN_DIR/$bin"
        chmod +x "$BIN_DIR/$bin"
        echo "  ✓ $BIN_DIR/$bin"
    else
        echo "  ⚠ $bin 빌드 결과물을 찾을 수 없습니다 — 건너뜀"
    fi
done

# 2) .desktop entry
mkdir -p "$APPS_DIR"
cp "$SCRIPT_DIR/qsafe.desktop" "$DESKTOP_FILE"
echo "  ✓ $DESKTOP_FILE"

# 3) MIME 정의
mkdir -p "$MIME_DIR"
cp "$SCRIPT_DIR/qsafe-mime.xml" "$MIME_FILE"
echo "  ✓ $MIME_FILE"

# 4) 아이콘 — qsafe-gui/icons/icon.png을 표준 위치로 복사
mkdir -p "$ICON_DIR"
if [ -f "$SCRIPT_DIR/../icons/icon.png" ]; then
    cp "$SCRIPT_DIR/../icons/icon.png" "$ICON_FILE"
    echo "  ✓ $ICON_FILE"
else
    echo "  ⚠ icon.png 미발견 — 기본 아이콘 사용"
fi

# 5) 캐시 갱신
update-mime-database "$DATA_DIR/mime" 2>/dev/null && echo "  ✓ update-mime-database" || true
update-desktop-database "$APPS_DIR" 2>/dev/null && echo "  ✓ update-desktop-database" || true
gtk-update-icon-cache "$DATA_DIR/icons/hicolor" 2>/dev/null && echo "  ✓ gtk-update-icon-cache" || true

echo
echo "설치 완료. 파일 매니저(Nautilus/Dolphin/Thunar)에서 .qs 더블 클릭 → qsafe-gui 실행."
