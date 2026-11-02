#!/usr/bin/env bash
# Builds dist/Kubeport.app, then dist/Kubeport-<version>-arm64.dmg.
# Ad hoc signing only (no Apple Developer account for now).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/version.txt")"
DIST="$ROOT/dist"
APP="$DIST/Kubeport.app"
DMG="$DIST/Kubeport-$VERSION-arm64.dmg"

rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

python3 -I "$ROOT/scripts/check-l10n.py"
"$ROOT/scripts/build-ffi.sh" >/dev/null
swift build -c release --arch arm64 --package-path "$ROOT/app"
BIN_DIR="$(swift build -c release --arch arm64 --package-path "$ROOT/app" --show-bin-path)"

cp "$BIN_DIR/Kubeport" "$APP/Contents/MacOS/Kubeport"
cp "$ROOT/app/Info.plist" "$APP/Contents/Info.plist"
cp -R "$ROOT/app/Resources/"*.lproj "$APP/Contents/Resources/"
cp "$ROOT/app/Resources/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"

codesign --force --deep --sign - "$APP"

hdiutil create -quiet -volname "Kubeport" -srcfolder "$APP" -ov -format UDZO "$DMG"
echo "$DMG"
