#!/usr/bin/env bash
# Installs the pinned Aeneas and Charon (package of the Aeneas release) into DEST.
# Sources, in sequence: the Aeneas release, the copy in a Strata release, a build from source.
# A package whose SHA-256 is not the pin stops the script (exit 1). It is never extracted.
# Usage: scripts/install-aeneas.sh DEST
# The tests set AENEAS_ENV (a pins file) and AENEAS_URLS (space-separated download sources).
set -euo pipefail
dest=${1:?usage: install-aeneas.sh DEST}
# shellcheck source=/dev/null
. "${AENEAS_ENV:-$(dirname "$0")/../toolchain.env}"

urls=${AENEAS_URLS:-"https://github.com/AeneasVerif/aeneas/releases/download/$AENEAS_RELEASE/$AENEAS_PACKAGE https://github.com/sravioli/strata/releases/download/aeneas-$AENEAS_RELEASE/$AENEAS_PACKAGE"}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$dest"

for url in $urls; do
  if ! curl -fsSL --retry 2 -o "$tmp/$AENEAS_PACKAGE" "$url"; then
    echo "not available: $url" >&2
    continue
  fi
  if ! (cd "$tmp" && echo "$AENEAS_SHA256  $AENEAS_PACKAGE" | sha256sum -c --quiet -); then
    echo "fail: the SHA-256 of $url is not the pinned $AENEAS_SHA256" >&2
    exit 1
  fi
  tar xzf "$tmp/$AENEAS_PACKAGE" -C "$dest" aeneas charon charon-driver rust-toolchain
  echo "installed Aeneas $AENEAS_RELEASE from $url into $dest"
  exit 0
done

# shortcut: the build from source is not tested in CI; upgrade when a run needs it (both downloads gone).
echo "no download source is available: building Aeneas $AENEAS_COMMIT from source" >&2
git clone -q https://github.com/AeneasVerif/aeneas "$tmp/src"
git -C "$tmp/src" checkout -q "$AENEAS_COMMIT"
make -C "$tmp/src" setup-charon  # clones Charon at the pin of this Aeneas commit
opam switch create "$tmp/src" 5.3.0 --no-install -y
opam install --switch="$tmp/src" "$tmp/src/src" "$tmp/src/charon" --deps-only -y
opam exec --switch="$tmp/src" -- make -C "$tmp/src"
cp "$tmp/src/bin/aeneas" "$tmp/src/charon/bin/charon" "$tmp/src/charon/bin/charon-driver" "$dest/"
cp "$tmp/src/charon/rust-toolchain" "$dest/" 2>/dev/null || cp "$tmp/src/charon/charon/rust-toolchain" "$dest/"
