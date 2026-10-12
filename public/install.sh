#!/bin/sh
# tmgr installer: https://tmgr.dev/install.sh
#
#   curl -fsSL https://tmgr.dev/install.sh | sh
#
# Installs the tmgr binary into ~/.local/bin (or $TMGR_INSTALL_DIR). Never uses sudo.
#   TMGR_CHANNEL=beta        also consider prereleases (default: stable)
#   TMGR_INSTALL_DIR=DIR     install directory
#   TMGR_ALLOW_UNSIGNED=1    install even though the signature cannot be verified
#
# The archive checksum (SHA256SUMS) is always verified. The signature of
# SHA256SUMS is verified with minisign when minisign is installed.

set -eu

REPO="tmgr-dev/frontend"
MINISIGN_PUBKEY="TMGR-UPDATE-KEY-PLACEHOLDER"
PLACEHOLDER="TMGR-UPDATE-KEY-PLACEHOLDER"

TMP=""

cleanup() {
  if [ -n "$TMP" ]; then
    rm -rf "$TMP"
  fi
}

die() {
  printf 'tmgr install: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf 'tmgr install: %s\n' "$*" >&2
}

have() {
  command -v "$1" >/dev/null 2>&1
}

safe_url() {
  case "$1" in
    https://* | http://127.0.0.1 | http://127.0.0.1[:/]* | http://localhost | http://localhost[:/]*) ;;
    *) die "refusing a non-https URL: $1" ;;
  esac
}

fetch() {
  safe_url "$1"
  if have curl; then
    curl -fsSL --proto-redir '=https' --retry 2 -o "$2" "$1"
  elif have wget; then
    wget -q -O "$2" "$1"
  else
    die "curl or wget is required"
  fi
}

detect_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Linux)
      case "$arch" in
        x86_64 | amd64) echo "x86_64-unknown-linux-musl" ;;
        aarch64 | arm64) echo "aarch64-unknown-linux-musl" ;;
        *) die "unsupported CPU architecture: $arch" ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        arm64 | aarch64) echo "aarch64-apple-darwin" ;;
        x86_64) echo "x86_64-apple-darwin" ;;
        *) die "unsupported CPU architecture: $arch" ;;
      esac
      ;;
    MINGW* | MSYS* | CYGWIN* | Windows*)
      die "Windows is not supported natively. Install WSL2 and run this installer inside it."
      ;;
    *)
      die "unsupported operating system: $os (Linux and macOS are supported; on Windows use WSL2)"
      ;;
  esac
}

release_rows() {
  awk '
    /^    "tag_name": *"/ { t = $0; sub(/^    "tag_name": *"/, "", t); sub(/".*/, "", t) }
    /^    "prerelease": *(true|false)/ {
      if (t != "") { print t, ($0 ~ /true/ ? "true" : "false") }
      t = ""
    }
  '
}

select_tag() {
  while read -r tag pre; do
    case "$tag" in
      cli-v*) ;;
      *) continue ;;
    esac
    if [ "$1" = "stable" ]; then
      [ "$pre" = "false" ] || continue
      case "${tag#cli-v}" in
        *-*) continue ;;
      esac
    fi
    echo "$tag"
    return 0
  done
  return 1
}

find_tag() {
  channel=$1
  page=1
  while [ "$page" -le 3 ]; do
    fetch "$API/repos/$REPO/releases?per_page=100&page=$page" "$TMP/releases.json" || return 1
    if tag=$(release_rows <"$TMP/releases.json" | select_tag "$channel"); then
      echo "$tag"
      return 0
    fi
    page=$((page + 1))
  done
  return 1
}

sha256_of() {
  if have sha256sum; then
    sha256sum "$1" | cut -d' ' -f1
  elif have shasum; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    die "sha256sum or shasum is required"
  fi
}

verify_checksum() {
  archive_name=$1
  expected=$(awk -v f="$archive_name" '$2 == f || $2 == "*" f { print $1; exit }' "$TMP/SHA256SUMS")
  [ -n "$expected" ] || die "SHA256SUMS has no entry for $archive_name"
  actual=$(sha256_of "$TMP/$archive_name")
  [ "$expected" = "$actual" ] || die "checksum mismatch for $archive_name; nothing was installed"
}

verify_signature() {
  allow=${TMGR_ALLOW_UNSIGNED:-0}
  signed=0
  if fetch "$DL/$1/SHA256SUMS.minisig" "$TMP/SHA256SUMS.minisig" 2>/dev/null; then
    signed=1
  fi

  if [ "$MINISIGN_PUBKEY" = "$PLACEHOLDER" ]; then
    if [ "$allow" != "1" ]; then
      die "this installer has no signing key yet, so the release cannot be verified. Set TMGR_ALLOW_UNSIGNED=1 to install on the strength of the SHA256SUMS checksum alone."
    fi
    warn "WARNING: installing WITHOUT signature verification (TMGR_ALLOW_UNSIGNED=1). Only the SHA256SUMS checksum from the same release was checked."
    return 0
  fi

  if [ "$signed" -eq 0 ]; then
    if [ "$allow" != "1" ]; then
      die "release $1 is not signed; refusing to install. Set TMGR_ALLOW_UNSIGNED=1 to override."
    fi
    warn "WARNING: installing an UNSIGNED release (TMGR_ALLOW_UNSIGNED=1)."
    return 0
  fi

  if have minisign; then
    minisign -Vm "$TMP/SHA256SUMS" -x "$TMP/SHA256SUMS.minisig" -P "$MINISIGN_PUBKEY" >/dev/null ||
      die "signature verification FAILED; nothing was installed"
    echo "signature verified"
  else
    warn "minisign is not installed: only the SHA256SUMS checksum was verified, not the signature."
  fi
}

main() {
  [ -n "${HOME:-}" ] || die "HOME is not set"
  API=${TMGR_API_URL:-https://api.github.com}
  DL=${TMGR_DOWNLOAD_BASE:-https://github.com/$REPO/releases/download}
  channel=${TMGR_CHANNEL:-stable}
  case "$channel" in
    stable | beta) ;;
    *) die "TMGR_CHANNEL must be stable or beta" ;;
  esac
  dest=${TMGR_INSTALL_DIR:-$HOME/.local/bin}

  target=$(detect_target)
  TMP=$(mktemp -d "${TMPDIR:-/tmp}/tmgr-install.XXXXXX")
  trap cleanup EXIT
  trap 'exit 1' HUP INT TERM

  tag=$(find_tag "$channel") || die "no $channel release found. Try TMGR_CHANNEL=beta."
  version=${tag#cli-v}
  case "$version" in
    "" | *[!0-9A-Za-z.+-]*) die "unexpected release tag: $tag" ;;
  esac

  archive="tmgr-$version-$target.tar.gz"
  echo "Installing tmgr $version ($target)"
  fetch "$DL/$tag/$archive" "$TMP/$archive"
  fetch "$DL/$tag/SHA256SUMS" "$TMP/SHA256SUMS"
  verify_checksum "$archive"
  verify_signature "$tag"

  mkdir "$TMP/x"
  tar -xzf "$TMP/$archive" -C "$TMP/x" "tmgr-$version-$target/tmgr"
  [ -f "$TMP/x/tmgr-$version-$target/tmgr" ] || die "the archive does not contain tmgr"

  mkdir -p "$dest"
  staged="$dest/.tmgr.new.$$"
  cp "$TMP/x/tmgr-$version-$target/tmgr" "$staged"
  chmod 755 "$staged"
  mv -f "$staged" "$dest/tmgr"

  echo "Installed $dest/tmgr"
  case ":$PATH:" in
    *":$dest:"*) ;;
    *) echo "Add $dest to your PATH to run tmgr." ;;
  esac
  "$dest/tmgr" --version || true
}

main "$@"
