#!/usr/bin/env bash
# create-appimage.sh -- Build a portable Linux AppImage for LogSleuth.
#
# Usage:
#   installer/linux/create-appimage.sh [VERSION]
#
# VERSION defaults to the value read from Cargo.toml if not supplied.
#
# appimagetool is used from PATH when installed (preferred). Otherwise it is
# downloaded from a PINNED release and its SHA-256 is verified before it is
# executed; override APPIMAGETOOL_VERSION and APPIMAGETOOL_SHA256 together to
# use a different release. The download is never run unverified.
# Requires: wget or curl and sha256sum (only when downloading appimagetool).
#
# Output: LogSleuth-<VERSION>.AppImage in the workspace root.

set -euo pipefail

# ---------------------------------------------------------------------------
# Resolve paths
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$WORKSPACE_ROOT"

# ---------------------------------------------------------------------------
# Version
# ---------------------------------------------------------------------------

if [[ "${1:-}" != "" ]]; then
    VERSION="$1"
else
    VERSION="$(grep '^version' Cargo.toml | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')"
fi

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "ERROR: version must be X.Y.Z. Pass it as the first argument or ensure Cargo.toml is present." >&2
    exit 1
fi

echo "[create-appimage] Building LogSleuth ${VERSION} AppImage..."

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

APP_NAME="LogSleuth"
BINARY="target/release/logsleuth"
APPIMAGE_NAME="${APP_NAME}-${VERSION}.AppImage"
WORK_DIR="$(mktemp -d)"
APPDIR="$WORK_DIR/${APP_NAME}.AppDir"

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Validate binary
# ---------------------------------------------------------------------------

if [[ ! -f "$BINARY" ]]; then
    echo "ERROR: release binary not found at $BINARY. Run 'cargo build --release' first." >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Build AppDir structure
# ---------------------------------------------------------------------------

mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Copy binary
cp "$BINARY" "$APPDIR/usr/bin/logsleuth"
chmod +x "$APPDIR/usr/bin/logsleuth"

# Copy icon (256x256 at AppDir root and standard hicolor location)
if [[ -f "assets/icon_256.png" ]]; then
    cp "assets/icon_256.png" "$APPDIR/logsleuth.png"
    cp "assets/icon_256.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/logsleuth.png"
else
    echo "WARNING: assets/icon_256.png not found; AppImage will have no icon." >&2
fi

# .desktop file (AppDir root + standard share location)
cat > "$APPDIR/logsleuth.desktop" << 'EOF'
[Desktop Entry]
Name=LogSleuth
GenericName=Log Viewer
Comment=Cross-platform log file viewer and analyser
Exec=logsleuth %u
Icon=logsleuth
Type=Application
Categories=Utility;System;
Keywords=log;viewer;analyser;parser;
StartupNotify=true
EOF

cp "$APPDIR/logsleuth.desktop" "$APPDIR/usr/share/applications/logsleuth.desktop"

# AppRun script (launched by the AppImage runtime)
cat > "$APPDIR/AppRun" << 'EOF'
#!/usr/bin/env bash
SELF="$(readlink -f "$0")"
HERE="${SELF%/*}"
export PATH="${HERE}/usr/bin:${PATH}"
exec "${HERE}/usr/bin/logsleuth" "$@"
EOF
chmod +x "$APPDIR/AppRun"

# ---------------------------------------------------------------------------
# Resolve appimagetool
# ---------------------------------------------------------------------------

APPIMAGETOOL_BIN="$WORK_DIR/appimagetool"

if command -v appimagetool &>/dev/null; then
    APPIMAGETOOL_BIN="$(command -v appimagetool)"
else
    # The downloaded file is executed with the caller's full privileges, so it
    # is pinned to a specific release (never the mutable 'continuous' tag,
    # which upstream overwrites) and its SHA-256 is verified before it runs.
    #
    # The default hash below is the sha256 GitHub reports for the
    # appimagetool-x86_64.AppImage asset of AppImage/appimagetool tag 1.9.1,
    # confirmed by downloading the asset and hashing it locally. Re-verify it
    # yourself before trusting it. Override both variables together to use a
    # different release.
    APPIMAGETOOL_VERSION="${APPIMAGETOOL_VERSION:-1.9.1}"
    if [[ "$APPIMAGETOOL_VERSION" == "1.9.1" ]]; then
        APPIMAGETOOL_SHA256="${APPIMAGETOOL_SHA256:-ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0}"
    else
        APPIMAGETOOL_SHA256="${APPIMAGETOOL_SHA256:-}"
    fi
    APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-x86_64.AppImage"

    if [[ -z "$APPIMAGETOOL_SHA256" ]]; then
        echo "ERROR: no expected checksum is configured for appimagetool ${APPIMAGETOOL_VERSION}." >&2
        echo "       Refusing to download and execute an unverified binary." >&2
        echo "       Install appimagetool from your distribution, or verify the release" >&2
        echo "       yourself and re-run with:" >&2
        echo "         APPIMAGETOOL_VERSION=${APPIMAGETOOL_VERSION} APPIMAGETOOL_SHA256=<sha256> $0" >&2
        echo "       Release: ${APPIMAGETOOL_URL}" >&2
        exit 1
    fi

    if ! command -v sha256sum &>/dev/null; then
        echo "ERROR: sha256sum is required to verify the appimagetool download." >&2
        exit 1
    fi

    echo "[create-appimage] appimagetool not found in PATH; downloading ${APPIMAGETOOL_VERSION}..."
    if command -v wget &>/dev/null; then
        wget -q "$APPIMAGETOOL_URL" -O "$APPIMAGETOOL_BIN"
    elif command -v curl &>/dev/null; then
        curl -sSL "$APPIMAGETOOL_URL" -o "$APPIMAGETOOL_BIN"
    else
        echo "ERROR: neither wget nor curl is available to download appimagetool." >&2
        exit 1
    fi

    ACTUAL_SHA256="$(sha256sum "$APPIMAGETOOL_BIN" | cut -d' ' -f1)"
    if [[ "$ACTUAL_SHA256" != "$APPIMAGETOOL_SHA256" ]]; then
        rm -f "$APPIMAGETOOL_BIN"
        echo "ERROR: appimagetool checksum mismatch -- refusing to execute the download." >&2
        echo "       expected: $APPIMAGETOOL_SHA256" >&2
        echo "       actual:   $ACTUAL_SHA256" >&2
        exit 1
    fi

    chmod +x "$APPIMAGETOOL_BIN"
fi

# ---------------------------------------------------------------------------
# Build AppImage
# ---------------------------------------------------------------------------

# Remove any stale AppImage from a previous run
rm -f "$APPIMAGE_NAME"

echo "[create-appimage] Running appimagetool..."
ARCH=x86_64 "$APPIMAGETOOL_BIN" "$APPDIR" "$APPIMAGE_NAME"

chmod +x "$APPIMAGE_NAME"

echo "[create-appimage] Created: $APPIMAGE_NAME"
