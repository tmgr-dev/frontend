#!/bin/sh
# tmgr installer: https://tmgr.dev/install.sh
#
#   curl -fsSL https://tmgr.dev/install.sh | sh
#
# Installs the tmgr binary into ~/.local/bin (or $TMGR_INSTALL_DIR). Never uses sudo.
#   TMGR_CHANNEL=beta        also consider prereleases (default: stable)
#   TMGR_INSTALL_DIR=DIR     install directory
#   TMGR_ALLOW_DOWNGRADE=1   allow replacing a newer installed tmgr
#   TMGR_ALLOW_UNSIGNED=1    install without verifying the release signature
#
# The release manifest is verified with minisign (required), then the archive
# is checked against the sha256 listed in the signed manifest.

set -eu

REPO="tmgr-dev/frontend"
MINISIGN_PUBKEY="TMGR-UPDATE-KEY-PLACEHOLDER"
PLACEHOLDER="TMGR-UPDATE-KEY-PLACEHOLDER"
MAX_API=4194304
MAX_MANIFEST=262144
MAX_ARCHIVE=125829120

TMP=""
STAGED=""

cleanup() {
  if [ -n "$TMP" ]; then
    rm -rf "$TMP"
  fi
  if [ -n "$STAGED" ]; then
    rm -f "$STAGED"
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

fetch() {
  have curl || die "curl is required"
  case "$1" in
    https://*)
      curl -fsS -L --proto '=https' --proto-redir '=https' --tlsv1.2 \
        --max-time 300 --max-filesize "$3" --retry 2 -o "$2" "$1" || return 1
      ;;
    http://127.0.0.1 | http://127.0.0.1[:/]* | http://localhost | http://localhost[:/]*)
      curl -fsS --proto '=http,https' --max-time 300 --max-filesize "$3" -o "$2" "$1" || return 1
      ;;
    *) die "refusing a non-https URL: $1" ;;
  esac
  size=$(wc -c <"$2" | tr -d ' ')
  [ "$size" -le "$3" ] || die "download from $1 is larger than allowed"
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
  : >"$TMP/rows"
  while [ "$page" -le 10 ]; do
    fetch "$API/repos/$REPO/releases?per_page=100&page=$page" "$TMP/releases.json" "$MAX_API" || return 1
    release_rows <"$TMP/releases.json" >>"$TMP/rows"
    if [ "$(grep -c '"tag_name"' "$TMP/releases.json" || true)" -lt 100 ]; then
      select_tag "$channel" <"$TMP/rows"
      return
    fi
    page=$((page + 1))
  done
  die "incomplete search: more than 10 pages of releases; a newer release may exist"
}

manifest_field() {
  awk -v k="$1" '
    $0 ~ "^  \"" k "\": " {
      v = $0
      sub("^  \"" k "\": *\"?", "", v)
      sub("\"?,?[ ]*$", "", v)
      print v
      exit
    }
  ' "$TMP/manifest.json"
}

manifest_artifacts() {
  awk '
    /^      "target": / { t = $2; gsub(/[",]/, "", t) }
    /^      "name": / { n = $2; gsub(/[",]/, "", n) }
    /^      "sha256": / { h = $2; gsub(/[",]/, "", h) }
    /^      "size": / { z = $2; gsub(/[",]/, "", z) }
    /^    }/ { if (t != "" && n != "" && h != "" && z != "") print t, n, h, z; t = n = h = z = "" }
  ' "$TMP/manifest.json"
}

ver_cmp() {
  awk -v a="$1" -v b="$2" '
    function parse(s, out,   core, pre, p, n, i) {
      sub(/\+.*/, "", s)
      pre = ""
      p = index(s, "-")
      if (p > 0) { pre = substr(s, p + 1); s = substr(s, 1, p - 1) }
      n = split(s, c, ".")
      for (i = 1; i <= 3; i++) out[i] = (i <= n ? c[i] + 0 : 0)
      out[4] = pre
    }
    BEGIN {
      parse(a, x); parse(b, y)
      for (i = 1; i <= 3; i++) {
        if (x[i] < y[i]) { print "lt"; exit }
        if (x[i] > y[i]) { print "gt"; exit }
      }
      if (x[4] == y[4]) { print "eq"; exit }
      if (x[4] == "") { print "gt"; exit }
      if (y[4] == "") { print "lt"; exit }
      na = split(x[4], p, "."); nb = split(y[4], q, ".")
      n = (na < nb ? na : nb)
      for (i = 1; i <= n; i++) {
        pn = (p[i] ~ /^[0-9]+$/); qn = (q[i] ~ /^[0-9]+$/)
        if (pn && qn) {
          if (p[i] + 0 < q[i] + 0) { print "lt"; exit }
          if (p[i] + 0 > q[i] + 0) { print "gt"; exit }
        } else if (pn) { print "lt"; exit }
        else if (qn) { print "gt"; exit }
        else {
          if (p[i] < q[i]) { print "lt"; exit }
          if (p[i] > q[i]) { print "gt"; exit }
        }
      }
      if (na < nb) print "lt"; else if (na > nb) print "gt"; else print "eq"
    }'
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

verify_signature() {
  allow=${TMGR_ALLOW_UNSIGNED:-0}
  if [ "$MINISIGN_PUBKEY" = "$PLACEHOLDER" ]; then
    [ "$allow" = "1" ] ||
      die "this installer has no signing key yet, so the release cannot be verified. Set TMGR_ALLOW_UNSIGNED=1 to install without verification."
    warn "WARNING: installing WITHOUT signature verification (TMGR_ALLOW_UNSIGNED=1)."
    return 0
  fi
  if ! have minisign; then
    [ "$allow" = "1" ] ||
      die "minisign is required to verify the release signature (install it, or set TMGR_ALLOW_UNSIGNED=1 to skip verification)."
    warn "WARNING: minisign not found; installing WITHOUT signature verification (TMGR_ALLOW_UNSIGNED=1)."
    return 0
  fi
  if ! fetch "$DL/$1/manifest.json.minisig" "$TMP/manifest.json.minisig" "$MAX_MANIFEST" 2>/dev/null; then
    [ "$allow" = "1" ] || die "release $1 has no signature; refusing to install. Set TMGR_ALLOW_UNSIGNED=1 to override."
    warn "WARNING: installing an UNSIGNED release (TMGR_ALLOW_UNSIGNED=1)."
    return 0
  fi
  minisign -Vm "$TMP/manifest.json" -x "$TMP/manifest.json.minisig" -P "$MINISIGN_PUBKEY" >/dev/null ||
    die "signature verification FAILED; nothing was installed"
  echo "signature verified"
}

check_manifest() {
  tag=$1
  version=$2
  channel=$3
  [ "$(manifest_field product)" = "tmgr-cli" ] || die "the manifest is for a different product"
  [ "$(manifest_field version)" = "$version" ] || die "the manifest version does not match the release tag"
  [ "$(manifest_field key_epoch)" = "1" ] || die "the manifest uses an unsupported key epoch; get a newer installer from https://tmgr.dev/install.sh"
  mchannel=$(manifest_field channel)
  case "$channel:$mchannel" in
    stable:stable | beta:stable | beta:beta) ;;
    *) die "the release is on channel $mchannel, not $channel" ;;
  esac
  expires=$(manifest_field expires_at)
  case "$expires" in
    [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z) ;;
    *) die "the manifest carries an invalid expiry" ;;
  esac
  expires_digits=$(printf '%s' "$expires" | tr -d '\n:TZ-')
  now_digits=$(date -u +%Y%m%d%H%M%S)
  [ "$expires_digits" -gt "$now_digits" ] || die "the manifest of $tag has expired; refusing a stale release"
}

check_not_older() {
  [ -x "$1/tmgr" ] || return 0
  installed=$("$1/tmgr" version --json 2>/dev/null |
    sed -n 's/.*"data":{"version":"\([^"]*\)".*/\1/p' | head -n 1) || installed=""
  if [ -z "$installed" ]; then
    warn "could not read the version of the installed tmgr; not checking for a downgrade"
    return 0
  fi
  if [ "$(ver_cmp "$2" "$installed")" = "lt" ]; then
    [ "${TMGR_ALLOW_DOWNGRADE:-0}" = "1" ] ||
      die "installed tmgr $installed is newer than $2; refusing to downgrade (TMGR_ALLOW_DOWNGRADE=1 to override)"
    warn "WARNING: downgrading tmgr $installed to $2 (TMGR_ALLOW_DOWNGRADE=1)."
  fi
}

check_dir() {
  uid=$(id -u)
  [ -n "$(find -L "$1" -prune -user "$uid" 2>/dev/null)" ] ||
    die "$1 is not owned by you; refusing to install there"
  # shellcheck disable=SC2012
  perm=$(ls -ldL "$1" | cut -c1-10)
  case "$perm" in
    ?????w???? | ????????w?) die "$1 is writable by group or others; refusing to install there" ;;
  esac
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

  echo "Installing tmgr $version ($target)"
  fetch "$DL/$tag/manifest.json" "$TMP/manifest.json" "$MAX_MANIFEST"
  verify_signature "$tag"
  check_manifest "$tag" "$version" "$channel"

  archive="tmgr-$version-$target.tar.gz"
  row=$(manifest_artifacts | awk -v t="$target" -v n="$archive" '$1 == t && $2 == n { print; exit }')
  [ -n "$row" ] || die "release $version has no build for $target"
  expected=$(echo "$row" | cut -d' ' -f3)
  size=$(echo "$row" | cut -d' ' -f4)
  case "$size" in
    "" | *[!0-9]*) die "the manifest carries an invalid size" ;;
  esac
  [ "$size" -gt 0 ] && [ "$size" -le "$MAX_ARCHIVE" ] || die "the manifest declares an invalid archive size"

  fetch "$DL/$tag/$archive" "$TMP/$archive" "$size"
  [ "$(wc -c <"$TMP/$archive" | tr -d ' ')" = "$size" ] || die "archive size does not match the manifest; nothing was installed"
  [ "$(sha256_of "$TMP/$archive")" = "$expected" ] || die "checksum mismatch for $archive; nothing was installed"

  check_not_older "$dest" "$version"

  mkdir "$TMP/x"
  tar -xzf "$TMP/$archive" -C "$TMP/x" "tmgr-$version-$target/tmgr"
  [ -f "$TMP/x/tmgr-$version-$target/tmgr" ] || die "the archive does not contain tmgr"

  (umask 022 && mkdir -p -- "$dest") || die "cannot create $dest"
  check_dir "$dest"
  STAGED=$(mktemp "$dest/.tmgr.new.XXXXXX")
  cat "$TMP/x/tmgr-$version-$target/tmgr" >"$STAGED"
  chmod 755 "$STAGED"
  mv -f "$STAGED" "$dest/tmgr"
  STAGED=""

  echo "Installed $dest/tmgr"
  case ":$PATH:" in
    *":$dest:"*) ;;
    *) echo "Add $dest to your PATH to run tmgr." ;;
  esac
  "$dest/tmgr" --version || true
}

main "$@"
