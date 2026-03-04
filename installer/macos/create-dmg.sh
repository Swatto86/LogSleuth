#!/usr/bin/env bash
# create-dmg.sh -- Build a macOS DMG containing a LogSleuth.app bundle.
#
# Usage:
#   installer/macos/create-dmg.sh [VERSION]
#
# VERSION defaults to the value read from Cargo.toml if not supplied.
#
# Prerequisites (installed by the CI workflow or locally):
#   brew install create-dmg
#
# Output: LogSleuth-<VERSION>.dmg in the workspace root.

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

if [[ -z "$VERSION" ]]; then
    echo "ERROR: could not determine version. Pass it as the first argument or ensure Cargo.toml is present." >&2
    exit 1
fi

echo "[create-dmg] Building LogSleuth ${VERSION} DMG..."

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

APP_NAME="LogSleuth"
BUNDLE_ID="com.swatto.logsleuth"
BINARY="target/release/logsleuth"
APP_BUNDLE="${APP_NAME}.app"
DMG_NAME="${APP_NAME}-${VERSION}.dmg"
STAGING_DIR="$(mktemp -d)"

cleanup() {
    rm -rf "$STAGING_DIR"
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
# Build .app bundle
# ---------------------------------------------------------------------------

BUNDLE_PATH="$STAGING_DIR/$APP_BUNDLE"
mkdir -p "$BUNDLE_PATH/Contents/MacOS"
mkdir -p "$BUNDLE_PATH/Contents/Resources"

cp "$BINARY" "$BUNDLE_PATH/Contents/MacOS/logsleuth"
chmod +x "$BUNDLE_PATH/Contents/MacOS/logsleuth"

# Info.plist
cat > "$BUNDLE_PATH/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>               <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>         <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>            <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key> <string>${VERSION}</string>
    <key>CFBundleExecutable</key>         <string>logsleuth</string>
    <key>CFBundlePackageType</key>        <string>APPL</string>
    <key>LSMinimumSystemVersion</key>     <string>12.0</string>
    <key>NSHighResolutionCapable</key>    <true/>
    <key>NSHumanReadableCopyright</key>   <string>Copyright 2024 Swatto. MIT License.</string>
</dict>
</plist>
EOF

# ---------------------------------------------------------------------------
# Generate .icns from PNG assets (requires sips + iconutil, present on macOS)
# ---------------------------------------------------------------------------

ICONSET_DIR="$(mktemp -d)/logsleuth.iconset"
mkdir -p "$ICONSET_DIR"
ICNS_BUILT=false

if command -v sips &>/dev/null && command -v iconutil &>/dev/null; then
    echo "[create-dmg] Building .icns from PNG assets..."
    sips -z 16   16   assets/icon.png     --out "$ICONSET_DIR/icon_16x16.png"      &>/dev/null || true
    sips -z 32   32   assets/icon_32.png  --out "$ICONSET_DIR/icon_16x16@2x.png"   &>/dev/null || true
    sips -z 32   32   assets/icon_32.png  --out "$ICONSET_DIR/icon_32x32.png"      &>/dev/null || true
    sips -z 64   64   assets/icon.png     --out "$ICONSET_DIR/icon_32x32@2x.png"   &>/dev/null || true
    sips -z 128  128  assets/icon.png     --out "$ICONSET_DIR/icon_128x128.png"    &>/dev/null || true
    sips -z 256  256  assets/icon_256.png --out "$ICONSET_DIR/icon_128x128@2x.png" &>/dev/null || true
    sips -z 256  256  assets/icon_256.png --out "$ICONSET_DIR/icon_256x256.png"    &>/dev/null || true
    sips -z 512  512  assets/icon_512.png --out "$ICONSET_DIR/icon_256x256@2x.png" &>/dev/null || true
    sips -z 512  512  assets/icon_512.png --out "$ICONSET_DIR/icon_512x512.png"    &>/dev/null || true

    ICNS_OUT="$BUNDLE_PATH/Contents/Resources/logsleuth.icns"
    if iconutil -c icns "$ICONSET_DIR" -o "$ICNS_OUT" 2>/dev/null; then
        # Inject CFBundleIconFile into Info.plist
        /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string logsleuth" \
            "$BUNDLE_PATH/Contents/Info.plist" 2>/dev/null || true
        ICNS_BUILT=true
        echo "[create-dmg] .icns created."
    fi
fi

if [[ "$ICNS_BUILT" == "false" ]]; then
    echo "[create-dmg] WARNING: sips/iconutil not available; bundle will have no icon." >&2
fi

# ---------------------------------------------------------------------------
# Build DMG
# ---------------------------------------------------------------------------

# Remove any stale DMG from a previous run
rm -f "$DMG_NAME"

if command -v create-dmg &>/dev/null; then
    echo "[create-dmg] Building DMG with create-dmg..."
    create-dmg \
        --volname        "$APP_NAME" \
        --volicon        "assets/icon.icns" 2>/dev/null \
        --window-pos     200 120 \
        --window-size    600 400 \
        --icon-size      128 \
        --icon           "$APP_BUNDLE" 150 195 \
        --hide-extension "$APP_BUNDLE" \
        --app-drop-link  450 195 \
        "$DMG_NAME" \
        "$STAGING_DIR" || \
    create-dmg \
        --volname       "$APP_NAME" \
        --window-pos    200 120 \
        --window-size   600 400 \
        --icon-size     128 \
        --icon          "$APP_BUNDLE" 150 195 \
        --hide-extension "$APP_BUNDLE" \
        --app-drop-link 450 195 \
        "$DMG_NAME" \
        "$STAGING_DIR"
else
    echo "[create-dmg] create-dmg not found; falling back to plain hdiutil DMG..."
    hdiutil create \
        -srcfolder "$STAGING_DIR" \
        -volname   "$APP_NAME" \
        -format    UDZO \
        -imagekey  zlib-level=9 \
        "$DMG_NAME"
fi

echo "[create-dmg] Created: $DMG_NAME"
