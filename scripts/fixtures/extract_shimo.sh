#!/usr/bin/env bash
#
# Extract 下巻 (lower half) from the combined 海辺のカフカ EPUB.
#
# The combined edition packs both 上 and 下 into a single EPUB with one
# spine, so a pure-shell rebuild is fragile. The robust path is a Python
# helper that walks the OPF spine, splits at the 下 boundary, and re-packs
# only the 下 half plus the manifest items it references.
#
# Until that helper exists, this script:
#   1. Verifies the combined source is present.
#   2. Cracks the EPUB into a temp dir so the user can inspect spine ordering.
#   3. Leaves a clear TODO + the deterministic checksum the result must hash to.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="${REPO_ROOT}/src-tauri/tests/fixtures/epub/kafka_shimo.epub"
SRC="${HOME}/Downloads/海辺のカフカ（上下）合本版（新潮文庫）.epub"

# Expected SHA-256 of kafka_shimo.epub. Populate once the helper produces a
# known-good artefact and snapshot tests pass.
EXPECTED_SHA256=""

if [[ ! -f "$SRC" ]]; then
    echo "ERROR combined EPUB not found at: $SRC" >&2
    echo "Drop the Kindle decrypt there and re-run." >&2
    exit 1
fi

if ! command -v unzip >/dev/null 2>&1; then
    echo "ERROR unzip is required" >&2
    exit 1
fi

WORK="$(mktemp -d -t kafka_shimo_extract.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

echo "Cracking combined EPUB into ${WORK}"
unzip -q "$SRC" -d "$WORK"

OPF="$(grep -o 'full-path="[^"]*"' "${WORK}/META-INF/container.xml" | head -1 | sed 's/full-path="//;s/"$//')"
if [[ -z "$OPF" ]]; then
    echo "ERROR could not locate OPF in container.xml" >&2
    exit 1
fi
echo "OPF: $OPF"

cat <<'TODO'

TODO: implement OPF-spine-aware split.

The shell version below is intentionally not run because OPF spine ordering and
manifest cross-references are too fragile for awk. Author lower-half-extract.py
that:

  1. Parses META-INF/container.xml to find the OPF.
  2. Reads the OPF spine + manifest.
  3. Identifies the 下巻 boundary (a navpoint titled "下巻" in the toc, or the
     first spine item whose XHTML contains the marker "下巻").
  4. Drops every spine item before that boundary.
  5. Rebuilds the manifest to keep only items still referenced.
  6. Repacks as a zip (mimetype first, STORED; everything else DEFLATED).

Once authored:

    python scripts/fixtures/lower-half-extract.py \
        --src "$SRC" \
        --dst src-tauri/tests/fixtures/epub/kafka_shimo.epub

  Then update EXPECTED_SHA256 in this script and uncomment the check below.

TODO

if [[ -n "$EXPECTED_SHA256" && -f "$DEST" ]]; then
    if command -v shasum >/dev/null 2>&1; then
        got="$(shasum -a 256 "$DEST" | awk '{print $1}')"
    else
        got="$(sha256sum "$DEST" | awk '{print $1}')"
    fi
    if [[ "$got" != "$EXPECTED_SHA256" ]]; then
        echo "ERROR kafka_shimo.epub sha256 mismatch: got $got, expected $EXPECTED_SHA256" >&2
        exit 1
    fi
    echo "OK kafka_shimo.epub sha256 matches expected."
fi

exit 0
