#!/bin/sh
# WackDepot .app bundle — ad-hoc signed dev build (no notarization; that's
# deliberately out of scope until distribution is decided).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/WackDepot.app"

cd "$ROOT"
cargo build --release -p wackdepot

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/wackdepot "$APP/Contents/MacOS/wackdepot"

if [ ! -f "$ROOT/assets/icon.icns" ]; then
    mkdir -p "$ROOT/assets/icon.iconset"
    python3 "$ROOT/scripts/make_icon.py"
    for s in 16 32 128 256 512; do
        sips -z "$s" "$s" /tmp/wackdepot-icon.png --out "$ROOT/assets/icon.iconset/icon_${s}x${s}.png" >/dev/null
        sips -z "$((s * 2))" "$((s * 2))" /tmp/wackdepot-icon.png --out "$ROOT/assets/icon.iconset/icon_${s}x${s}@2x.png" >/dev/null
    done
    iconutil -c icns "$ROOT/assets/icon.iconset" -o "$ROOT/assets/icon.icns"
    rm -rf "$ROOT/assets/icon.iconset"
fi
cp "$ROOT/assets/icon.icns" "$APP/Contents/Resources/icon.icns"

cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>WackDepot</string>
    <key>CFBundleDisplayName</key><string>WackDepot</string>
    <key>CFBundleIdentifier</key><string>com.wackdepot.app</string>
    <key>CFBundleVersion</key><string>0.1.0</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleExecutable</key><string>wackdepot</string>
    <key>CFBundleIconFile</key><string>icon.icns</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
EOF

codesign --force --deep --sign - "$APP" 2>/dev/null || true
echo "Built $APP"
