#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: nix-bump.sh <version>}"
version="${version#desktop-v}"
version="${version#v}"

root="$(cd "$(dirname "$0")/.." && pwd)"
url="https://github.com/tmgr-dev/frontend/releases/download/desktop-v${version}/TMGR_${version}_amd64.deb"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fsSL --retry 3 -o "$tmp/tmgr.deb" "$url"
hash="$(nix hash file --type sha256 --sri "$tmp/tmgr.deb")"

printf '{\n  "version": "%s",\n  "hash": "%s"\n}\n' "$version" "$hash" > "$root/nix/source.json"
echo "nix/source.json -> ${version} ${hash}"
