#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: nix-bump.sh <version>}"
version="${version#desktop-v}"
version="${version#v}"

root="$(cd "$(dirname "$0")/.." && pwd)"
base="https://github.com/tmgr-dev/frontend/releases/download/desktop-v${version}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

hash_of() {
  curl -fsSL --retry 3 -o "$tmp/tmgr.deb" "$base/TMGR_${version}_$1.deb"
  nix hash file --type sha256 --sri "$tmp/tmgr.deb"
}

x86="$(hash_of amd64)"
arm="$(hash_of arm64)"

printf '{\n  "version": "%s",\n  "hashes": {\n    "x86_64-linux": "%s",\n    "aarch64-linux": "%s"\n  }\n}\n' "$version" "$x86" "$arm" > "$root/nix/source.json"
echo "nix/source.json -> ${version} ${x86} ${arm}"
