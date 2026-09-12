#!/usr/bin/env bash
# Exercise the real packaging script with a harmless downloaded tool.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
mkdir -p "$test_dir/installer/linux" "$test_dir/target/release" "$test_dir/bin"
cp "$repo/installer/linux/create-appimage.sh" "$test_dir/installer/linux/"
printf 'fake application\n' > "$test_dir/target/release/logsleuth"
export MARKER="$test_dir/tool-ran" FAKE_DOWNLOAD="$test_dir/download"
cat > "$FAKE_DOWNLOAD" <<'EOF'
#!/usr/bin/env bash
touch "$MARKER"
printf 'package\n' > "$2"
EOF
cat > "$test_dir/bin/wget" <<'EOF'
#!/usr/bin/env bash
cp "$FAKE_DOWNLOAD" "${@: -1}"
EOF
chmod +x "$test_dir/bin/wget"
export PATH="$test_dir/bin:$PATH"
if command -v appimagetool >/dev/null; then
    echo 'Run this download-path test without appimagetool installed.' >&2
    exit 1
fi
export APPIMAGETOOL_VERSION=test
export APPIMAGETOOL_SHA256=invalid
if bash "$test_dir/installer/linux/create-appimage.sh" 1.2.3 >"$test_dir/output" 2>&1; then
    echo 'Incorrect checksum was accepted' >&2; exit 1
fi
grep -q 'checksum mismatch' "$test_dir/output"
test ! -e "$MARKER"
APPIMAGETOOL_SHA256="$(sha256sum "$FAKE_DOWNLOAD" | cut -d' ' -f1)"
export APPIMAGETOOL_SHA256
bash "$test_dir/installer/linux/create-appimage.sh" 1.2.3
test -f "$MARKER"
test "$(cat "$test_dir/LogSleuth-1.2.3.AppImage")" = package
echo 'Verified: wrong hash never executes; correct hash packages successfully.'
