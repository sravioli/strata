#!/usr/bin/env bash
# Builds nsjail from source at the pinned tag and commit (.github/requirements/tools.env) into DEST.
# A tag whose commit is not the pin stops the script (exit 1). Nothing is built or installed.
# Usage: scripts/install-nsjail.sh DEST
# The tests set TOOLS_ENV (a pins file) and NSJAIL_REPO (the source repository).
set -euo pipefail
dest=${1:?usage: install-nsjail.sh DEST}
# shellcheck source=/dev/null
. "${TOOLS_ENV:-$(dirname "$0")/../.github/requirements/tools.env}"
repo=${NSJAIL_REPO:-https://github.com/google/nsjail}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git -c advice.detachedHead=false clone -q --depth 1 --branch "$NSJAIL_TAG" \
  --recurse-submodules --shallow-submodules "$repo" "$tmp/src"
head=$(git -C "$tmp/src" rev-parse HEAD)
if [ "$head" != "$NSJAIL_COMMIT" ]; then
  echo "fail: tag $NSJAIL_TAG of $repo is $head, not the pinned $NSJAIL_COMMIT" >&2
  exit 1
fi
make -C "$tmp/src" -j"$(nproc)" nsjail
mkdir -p "$dest"
install -m755 "$tmp/src/nsjail" "$dest/nsjail"
echo "installed nsjail $NSJAIL_TAG ($NSJAIL_COMMIT) into $dest"
