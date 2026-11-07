#!/usr/bin/env bash
set -euo pipefail

unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_PASSWORD APPLE_ID

if [ "$(uname -s)" != "Darwin" ]; then
  echo "build-widget: macOS only, skipping"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src-tauri/widget"
OUT="$SRC/build"
APPEX="$OUT/TMGRWidget.appex"
IDENTITY="${WIDGET_SIGN_IDENTITY:--}"
MIN=13.3
SDK="$(xcrun --sdk macosx --show-sdk-path)"

rm -rf "$OUT"
mkdir -p "$OUT/obj" "$APPEX/Contents/MacOS"

SLICES=()
for arch in arm64 x86_64; do
  xcrun swiftc -O -parse-as-library -application-extension \
    -module-name TMGRWidget -sdk "$SDK" -target "$arch-apple-macos$MIN" \
    -framework WidgetKit -framework SwiftUI \
    -Xlinker -e -Xlinker _NSExtensionMain \
    -Xlinker -rpath -Xlinker @executable_path/../Frameworks \
    -Xlinker -rpath -Xlinker @executable_path/../../../../Frameworks \
    "$SRC"/Sources/*.swift -o "$OUT/obj/TMGRWidget-$arch"
  SLICES+=("$OUT/obj/TMGRWidget-$arch")
done
lipo -create "${SLICES[@]}" -output "$APPEX/Contents/MacOS/TMGRWidget"
cp "$SRC/Info.plist" "$APPEX/Contents/Info.plist"
VERSION="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT/src-tauri/tauri.conf.json" | head -n 1)"
[ -n "$VERSION" ] || { echo "build-widget: no version in tauri.conf.json" >&2; exit 1; }
for key in CFBundleShortVersionString CFBundleVersion; do
  /usr/libexec/PlistBuddy -c "Set :$key $VERSION" "$APPEX/Contents/Info.plist"
done
rm -rf "$OUT/obj"

SIGN_ARGS=(--force --sign "$IDENTITY" --options runtime --entitlements "$SRC/TMGRWidget.entitlements")
if [ -n "${WIDGET_KEYCHAIN:-}" ]; then
  SIGN_ARGS+=(--keychain "$WIDGET_KEYCHAIN")
fi
if [ "$IDENTITY" = "-" ]; then
  codesign "${SIGN_ARGS[@]}" "$APPEX"
else
  codesign "${SIGN_ARGS[@]}" --timestamp "$APPEX" || { sleep 5; codesign "${SIGN_ARGS[@]}" --timestamp "$APPEX"; }
fi
echo "built $APPEX"
