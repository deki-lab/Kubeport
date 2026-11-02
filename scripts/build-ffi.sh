#!/usr/bin/env bash
# Compile kubeport-ffi, génère les bindings Swift et assemble app/KubeportFFI.xcframework.
# Sorties (ignorées par git) :
#   app/KubeportFFI.xcframework           bibliothèque statique + en-têtes + modulemap
#   app/Sources/Kubeport/Generated/       code Swift généré par UniFFI
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GEN="$ROOT/target/uniffi"
XCF="$ROOT/app/KubeportFFI.xcframework"
SWIFT_OUT="$ROOT/app/Sources/Kubeport/Generated"

cd "$ROOT"
cargo build --release -p kubeport-ffi

rm -rf "$GEN" "$XCF" "$SWIFT_OUT"
mkdir -p "$GEN/include" "$SWIFT_OUT"
cargo run --release -q -p kubeport-ffi --bin uniffi-bindgen -- \
  target/release/libkubeport_ffi.dylib "$GEN" \
  --swift-sources --headers --modulemap \
  --module-name kubeport_ffiFFI --modulemap-filename module.modulemap

mv "$GEN"/*.h "$GEN/module.modulemap" "$GEN/include/"
mv "$GEN"/*.swift "$SWIFT_OUT/"

xcodebuild -create-xcframework \
  -library target/release/libkubeport_ffi.a \
  -headers "$GEN/include" \
  -output "$XCF" >/dev/null
echo "$XCF"
