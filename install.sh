#!/bin/sh
# qsafe 설치 스크립트 — 모든 주요 OS 자동 감지
# 사용:
#   curl -fsSL https://qsafe.dev/install.sh | sh
#   (또는 wget -qO- https://qsafe.dev/install.sh | sh)

set -e

REPO="heedong719/qsafe"
VERSION="${QSAFE_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# 색상
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo "${BLUE}ℹ${NC} $*"; }
ok()   { echo "${GREEN}✓${NC} $*"; }
warn() { echo "${YELLOW}⚠${NC} $*" >&2; }
err()  { echo "${RED}✗${NC} $*" >&2; exit 1; }

# OS + arch 감지
detect_target() {
    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
                aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
                *) err "지원하지 않는 Linux 아키텍처: $ARCH" ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                arm64) TARGET="aarch64-apple-darwin" ;;
                x86_64) TARGET="x86_64-apple-darwin" ;;
                *) err "지원하지 않는 macOS 아키텍처: $ARCH" ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            TARGET="x86_64-pc-windows-msvc"
            warn "Windows 감지 — install.ps1 사용 권장"
            ;;
        *) err "지원하지 않는 OS: $OS" ;;
    esac
    echo "$TARGET"
}

# 다운로드 도구 감지
detect_downloader() {
    if command -v curl >/dev/null 2>&1; then
        echo "curl -fsSL"
    elif command -v wget >/dev/null 2>&1; then
        echo "wget -qO-"
    else
        err "curl 또는 wget 필요"
    fi
}

main() {
    info "qsafe 설치 시작..."

    TARGET=$(detect_target)
    info "감지된 타겟: $TARGET"

    # 최신 버전 가져오기
    if [ "$VERSION" = "latest" ]; then
        DL=$(detect_downloader)
        TAG=$($DL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
            | grep '"tag_name"' \
            | sed 's/.*"\([^"]*\)".*/\1/' \
            | head -1)
        if [ -z "$TAG" ]; then
            warn "최신 버전 조회 실패 (GitHub API 한도?) — 'main' 빌드 시도"
            install_from_source
            return
        fi
        VERSION="$TAG"
    fi

    info "버전: $VERSION"

    # 다운로드
    URL="https://github.com/$REPO/releases/download/$VERSION/qsafe-$VERSION-$TARGET.tar.gz"
    info "다운로드: $URL"

    TMP=$(mktemp -d)
    trap "rm -rf $TMP" EXIT

    DL=$(detect_downloader)
    $DL "$URL" > "$TMP/qsafe.tar.gz" 2>/dev/null \
        || err "다운로드 실패. 'qsafe build from source' 옵션 사용 가능"

    # 압축 해제
    tar -xzf "$TMP/qsafe.tar.gz" -C "$TMP"

    # 설치
    if [ -w "$INSTALL_DIR" ]; then
        cp "$TMP/qsafe" "$INSTALL_DIR/"
    else
        info "권한 필요: sudo 사용"
        sudo cp "$TMP/qsafe" "$INSTALL_DIR/"
    fi

    chmod +x "$INSTALL_DIR/qsafe" 2>/dev/null || \
        sudo chmod +x "$INSTALL_DIR/qsafe"

    ok "설치 완료: $INSTALL_DIR/qsafe"
    echo
    "$INSTALL_DIR/qsafe" --version 2>/dev/null || \
        warn "PATH에 $INSTALL_DIR 추가 필요"

    echo
    echo "사용법:"
    echo "  qsafe pack file.pdf          # 압축+암호화"
    echo "  qsafe unpack file.pdf.qs     # 풀기"
    echo "  qsafe extract foo.rar        # RAR/ZIP/7Z 등 풀기"
    echo "  qsafe mnemonic generate      # BIP39 단어 생성"
    echo "  qsafe shamir split secret -m 3 -n 5"
    echo "  qsafe bench                  # 성능 측정"
    echo "  qsafe --help"
}

install_from_source() {
    info "소스에서 빌드 — Rust 필요"
    if ! command -v cargo >/dev/null 2>&1; then
        err "Rust/Cargo 미설치. https://rustup.rs 에서 설치 후 재시도"
    fi

    info "git clone + cargo install..."
    cargo install --git "https://github.com/$REPO" --bin qsafe \
        || err "빌드 실패"

    ok "설치 완료: ~/.cargo/bin/qsafe"
}

main "$@"
