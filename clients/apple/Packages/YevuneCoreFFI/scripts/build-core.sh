#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../../../../.." && pwd)"
core="$root/core"
package="$root/clients/apple/Packages/YevuneCoreFFI"
generated="$package/Sources/YevuneCoreFFI"
framework="$package/YevuneCoreFFI.xcframework"
target="$core/target/macos14"

rm -rf "$generated" "$framework"
mkdir -p "$generated"

MACOSX_DEPLOYMENT_TARGET=14.0 CARGO_TARGET_DIR="$target" cargo build --manifest-path "$core/Cargo.toml" --release
(cd "$core" && MACOSX_DEPLOYMENT_TARGET=14.0 CARGO_TARGET_DIR="$target" cargo run --bin uniffi-bindgen -- \
  generate --library "$target/release/libyevune_core.dylib" \
  --language swift --out-dir "$generated" --config "$core/uniffi.toml")
cp "$generated/YevuneCoreFFI.modulemap" "$generated/module.modulemap"
sed -i '' 's/canImport(YevuneCoreFFI)/canImport(YevuneCoreFFIBinary)/; s/import YevuneCoreFFI/import YevuneCoreFFIBinary/' \
  "$generated/YevuneCoreFFI.swift"
sed -i '' 's/module YevuneCoreFFI/module YevuneCoreFFIBinary/' \
  "$generated/YevuneCoreFFI.modulemap" "$generated/module.modulemap"

xcodebuild -create-xcframework \
  -library "$target/release/libyevune_core.a" \
  -headers "$generated" \
  -output "$framework"
