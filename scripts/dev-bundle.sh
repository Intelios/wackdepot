#!/bin/sh
# Debug build wrapped in "WackDepot Dev.app" (bundle id com.wackdepot.app.dev)
# so it gets a real app identity for screenshots and UI checks, then launches
# it. Extra arguments are passed as KEY=VALUE environment for the app, e.g.
#   sh scripts/dev-bundle.sh WACKDEPOT_OPEN=~/code/some-repo WACKDEPOT_SLOWMO=8
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/dev/WackDepot Dev.app"

cd "$ROOT"
cargo build -p wackdepot

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/debug/wackdepot "$APP/Contents/MacOS/wackdepot"
[ -f "$ROOT/assets/icon.icns" ] && cp "$ROOT/assets/icon.icns" "$APP/Contents/Resources/icon.icns"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>WackDepot Dev</string>
    <key>CFBundleIdentifier</key><string>com.wackdepot.app.dev</string>
    <key>CFBundleExecutable</key><string>wackdepot</string>
    <key>CFBundleIconFile</key><string>icon.icns</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign - "$APP" 2>/dev/null || true

pkill -f "WackDepot Dev.app/Contents/MacOS/wackdepot" 2>/dev/null || true
# `open --env KEY=VALUE` hands environment to the launched app.
ENVS=""
for kv in "$@"; do ENVS="$ENVS --env $kv"; done
# shellcheck disable=SC2086
open -n "$APP" $ENVS
