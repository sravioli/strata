#!/usr/bin/env bash
# Installs the pinned release binary NAME (pins in .github/requirements/tools.env) into DEST.
# A download whose SHA-256 is not the pin stops the script (exit 1). It is never extracted.
# Usage: scripts/install-tool.sh NAME DEST
# The tests set TOOLS_ENV (a pins file).
set -euo pipefail
name=${1:?usage: install-tool.sh NAME DEST}
dest=${2:?usage: install-tool.sh NAME DEST}
# shellcheck source=/dev/null
. "${TOOLS_ENV:-$(dirname "$0")/../.github/requirements/tools.env}"

key=$(tr 'a-z-' 'A-Z_' <<<"$name")
url_var=${key}_URL sha_var=${key}_SHA256
url=${!url_var:-} sha=${!sha_var:-}
if [ -z "$url" ] || [ -z "$sha" ]; then
  echo "fail: no pin for $name (need ${url_var} and ${sha_var})" >&2
  exit 2
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --retry 2 -o "$tmp/pkg.tar.gz" "$url"
if ! (cd "$tmp" && echo "$sha  pkg.tar.gz" | sha256sum -c --quiet -); then
  echo "fail: the SHA-256 of $url is not the pinned $sha" >&2
  exit 1
fi
mkdir -p "$tmp/x" "$dest"
tar xzf "$tmp/pkg.tar.gz" -C "$tmp/x"
bin=$(find "$tmp/x" -type f -name "$name" | head -n 1)
if [ -z "$bin" ]; then
  echo "fail: $url holds no file named $name" >&2
  exit 1
fi
install -m755 "$bin" "$dest/$name"
echo "installed $name from $url into $dest"
