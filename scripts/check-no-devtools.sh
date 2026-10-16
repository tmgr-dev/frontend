#!/usr/bin/env bash
# Fails if wry's "devtools" feature (Web Inspector) is enabled in the desktop release build,
# directly or through any dependency. Usage: scripts/check-no-devtools.sh [target-triple...]
# Without arguments it checks every target the desktop release builds.
set -euo pipefail

cd "$(dirname "$0")/.."

# The release builds with default features only. If tauri.conf.json starts passing
# build.features, this check has to pass the same features to cargo tree.
if grep -q '"features"' src-tauri/tauri*.conf.json; then
	echo "error: src-tauri/tauri*.conf.json sets features; add them to scripts/check-no-devtools.sh" >&2
	exit 1
fi

targets=("$@")
if [[ ${#targets[@]} -eq 0 ]]; then
	targets=(aarch64-apple-darwin x86_64-pc-windows-msvc x86_64-unknown-linux-gnu)
fi

status=0
for target in "${targets[@]}"; do
	tree="$(cargo tree --manifest-path src-tauri/Cargo.toml -e features,normal -i wry --target "$target")"
	if [[ "$tree" != "wry v"* ]]; then
		echo "error: $target: no wry in the dependency tree, the check cannot run" >&2
		printf '%s\n' "$tree" >&2
		status=1
	elif grep -q 'wry feature "devtools"' <<<"$tree"; then
		echo "error: $target: wry \"devtools\" is enabled, the release would ship the Web Inspector:" >&2
		printf '%s\n' "$tree" >&2
		status=1
	else
		echo "ok: $target: wry without devtools"
	fi
done
exit "$status"
