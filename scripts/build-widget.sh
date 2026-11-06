#!/usr/bin/env bash
set -euo pipefail

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
rm -rf "$OUT/obj"

SIGN_ARGS=(--force --sign "$IDENTITY" --options runtime --entitlements "$SRC/TMGRWidget.entitlements")
if [ "$IDENTITY" != "-" ]; then
  SIGN_ARGS+=(--timestamp)
fi
codesign "${SIGN_ARGS[@]}" "$APPEX"
echo "built $APPEX"
