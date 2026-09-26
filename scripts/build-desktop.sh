#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -f .env.desktop-signing ]]; then
	set -a
	source .env.desktop-signing
	set +a
fi

for var in APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
	[[ -z "${!var:-}" ]] && unset "$var"
done

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
	APPLE_SIGNING_IDENTITY="$(security find-identity -v -p codesigning | grep -o '"Developer ID Application[^"]*"' | head -1 | tr -d '"' || true)"
	export APPLE_SIGNING_IDENTITY
fi

if [[ -z "${APPLE_SIGNING_IDENTITY}" ]]; then
	echo "No Developer ID Application identity found; the app will be unsigned." >&2
fi

UPDATER_KEY="${TAURI_UPDATER_KEY_FILE:-$HOME/.tauri/tmgr-updater.key}"
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -f "$UPDATER_KEY" ]]; then
	export TAURI_SIGNING_PRIVATE_KEY="$(cat "$UPDATER_KEY")"
	[[ -f "$UPDATER_KEY.password" ]] && export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat "$UPDATER_KEY.password")"
fi

extra_args=()
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
	echo "No updater signing key; building without update artifacts." >&2
	extra_args+=(--config '{"bundle":{"createUpdaterArtifacts":false}}')
fi

CI="${CI:-true}" npx tauri build ${extra_args[@]+"${extra_args[@]}"} "$@"
