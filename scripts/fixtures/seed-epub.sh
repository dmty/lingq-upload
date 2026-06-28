#!/usr/bin/env bash
#
# Seed EPUB fixtures from the local Downloads directory.
#
# Symlinks decrypted EPUBs from $HOME/Downloads into
# src-tauri/tests/fixtures/epub/{dialects,}. Files are personal Kindle
# decrypts that we cannot ship; the fixture directory is gitignored.
#
# Safe to re-run; idempotent. Missing files are warnings, not errors.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="${REPO_ROOT}/src-tauri/tests/fixtures/epub"
DEST_DIALECTS="${DEST}/dialects"
SRC="${HOME}/Downloads"

mkdir -p "${DEST_DIALECTS}"

linked=0
missing=0

link_if_exists() {
    local src="$1"
    local dst="$2"
    if [[ -f "$src" ]]; then
        ln -sfn "$src" "$dst"
        echo "  linked $(basename "$dst") -> $src"
        linked=$((linked + 1))
    else
        echo "  WARN missing: $src" >&2
        missing=$((missing + 1))
    fi
}

echo "Seeding EPUB fixtures from ${SRC} into ${DEST}"

# Kafka 下巻 — primary snapshot fixture for Kindle dialect.
# The user's combined file holds both 上 and 下 in one EPUB; the companion
# extract_shimo.sh splits out the 下 half into kafka_shimo.epub.
KAFKA_COMBINED="${SRC}/海辺のカフカ（上下）合本版（新潮文庫）.epub"
KAFKA_SHIMO="${DEST}/kafka_shimo.epub"

if [[ -f "$KAFKA_COMBINED" ]]; then
    if [[ ! -f "$KAFKA_SHIMO" && ! -L "$KAFKA_SHIMO" ]]; then
        # extract_shimo.sh is intentionally a TODO today — it exits non-zero
        # without producing kafka_shimo.epub. Surface that loudly so the user
        # is not surprised when kafka_snapshot_test keeps skipping.
        echo "  combined Kafka EPUB found, but extract_shimo.sh is not yet implemented" >&2
        echo "  -> kafka_shimo.epub NOT seeded; kafka_snapshot_test will skip" >&2
        missing=$((missing + 1))
    else
        echo "  kafka_shimo.epub already present"
    fi
else
    echo "  WARN missing combined Kafka EPUB at ${KAFKA_COMBINED}" >&2
    missing=$((missing + 1))
fi

# Dialect samples — link any *.epub in Downloads that look like Kindle / Kobo /
# generic dialect samples. The exact list is user-curated; we just mirror
# anything that matches the dialect glob.
shopt -s nullglob
for f in "${SRC}"/*kindle*.epub "${SRC}"/*kobo*.epub "${SRC}"/*navdoc*.epub; do
    link_if_exists "$f" "${DEST_DIALECTS}/$(basename "$f")"
done
shopt -u nullglob

echo
echo "Seed complete: ${linked} linked, ${missing} missing."
echo "Tests that depend on these fixtures will auto-skip if absent."

# Exit 0 even when nothing was linked — CI must remain green without fixtures.
exit 0
