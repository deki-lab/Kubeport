#!/usr/bin/env bash
# Construit dist/Kubeport.app puis dist/Kubeport-<version>-arm64.dmg.
# Signature ad hoc uniquement (pas de compte Apple Developer pour l'instant).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/version.txt")"
DIST="$ROOT/dist"
APP="$DIST/Kubeport.app"
DMG="$DIST/Kubeport-$VERSION-arm64.dmg"

rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

"$ROOT/scripts/build-ffi.sh" >/dev/null
swift build -c release --arch arm64 --package-path "$ROOT/app"
BIN_DIR="$(swift build -c release --arch arm64 --package-path "$ROOT/app" --show-bin-path)"

cp "$BIN_DIR/Kubeport" "$APP/Contents/MacOS/Kubeport"
cp "$ROOT/app/Info.plist" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"

codesign --force --deep --sign - "$APP"

hdiutil create -quiet -volname "Kubeport" -srcfolder "$APP" -ov -format UDZO "$DMG"
echo "$DMG"
