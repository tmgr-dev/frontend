#!/usr/bin/env bash
# Fails if the desktop binary links the espeak-ng engine (GPL-3): its exported API names or library name.
# Plain mentions in sherpa-onnx messages ("espeak-ng is no longer used") are not the engine and are allowed.
# Usage: scripts/check-no-espeak.sh [binary]
set -euo pipefail

cd "$(dirname "$0")/.."

bin="${1:-}"
if [[ -z "$bin" ]]; then
	for candidate in src-tauri/target/release/tmgr src-tauri/target/release/tmgr.exe; do
		[[ -f "$candidate" ]] && bin="$candidate" && break
	done
fi
if [[ -z "$bin" || ! -f "$bin" ]]; then
	echo "error: desktop binary not found" >&2
	exit 1
fi

if grep -a -q -E 'espeak_(Initialize|Synth|SetVoiceByName|Terminate|ng_)|libespeak' "$bin"; then
	echo "error: $bin contains espeak symbols" >&2
	exit 1
fi
echo "ok: no espeak symbols in $bin"
