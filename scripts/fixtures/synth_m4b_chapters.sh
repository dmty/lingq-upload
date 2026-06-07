#!/usr/bin/env bash
#
# Generate synthetic m4b fixtures with embedded chapter atoms for unit and
# integration tests of the chapter-probe + proportional-pack pipeline.
#
# Three variants are produced, each covering a real-world contract edge case:
#
#   1. synth_chapters_generic.m4b   — 60 s, 3 contiguous atoms with generic
#      indexed titles ("Chapter 1"). Models Kotonoha / Honzuki-style atom
#      title quality.
#
#   2. synth_chapters_narrative.m4b — 60 s, 3 contiguous atoms with narrative
#      Japanese titles ("序章", "第一章", "第二章"). Models Audible-merge
#      delivery (Kafka, Tensura) atom title quality.
#
#   3. synth_chapters_intro.m4b     — 120 s, 3 atoms, first atom is a 5 s
#      branding / preamble that the probe filter must drop. Models the
#      "知っておきたい日本の神話" tiny-first-atom case.
#
# Outputs at src-tauri/tests/fixtures/audio/. The audio stream is silence to
# keep the artefacts small.
#
# Re-encoding to AAC is non-deterministic across libavcodec builds, so no
# expected sha256 is asserted — call sites that depend on byte-identical
# artefacts should rely on the structural assertions instead (nb_chapters,
# atom start/end, atom titles).
#
# Known ffmpeg limitation — chapter time_base normalisation:
# The mp4 muxer rewrites every chapter atom's time_base to 1/1000 on output,
# regardless of what TIMEBASE the input metadata file specifies. Real-world
# Audible files carry 1/44100, but we cannot synthesise that here. This is
# safe because the parser reads the float `start_time` / `end_time` fields
# (always in seconds) and ignores the integer `start` / `end` + `time_base`
# fields. The contract in the spike already calls this out; fixtures exercise
# the float-path correctness, real-world Audible files exercise time_base
# variance separately via the integration test that drops in a real Kafka
# m4b (if locally available).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="${REPO_ROOT}/src-tauri/tests/fixtures/audio"
mkdir -p "$DEST_DIR"

if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "ERROR ffmpeg is required" >&2
    exit 1
fi
if ! command -v ffprobe >/dev/null 2>&1; then
    echo "ERROR ffprobe is required" >&2
    exit 1
fi

WORK="$(mktemp -d -t synth_m4b.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------
# Variant 1 — 60 s, 3 atoms at 0..20, 20..40, 40..60 s, time_base 1/1000.
# ---------------------------------------------------------------------------
cat >"${WORK}/meta_ms.txt" <<'EOF'
;FFMETADATA1
title=Synthetic Chapters (ms)
artist=Synth Publisher

[CHAPTER]
TIMEBASE=1/1000
START=0
END=20000
title=Chapter 1

[CHAPTER]
TIMEBASE=1/1000
START=20000
END=40000
title=Chapter 2

[CHAPTER]
TIMEBASE=1/1000
START=40000
END=60000
title=Chapter 3
EOF

ffmpeg -y -hide_banner -loglevel error \
    -f lavfi -i "anullsrc=r=22050:cl=stereo" \
    -i "${WORK}/meta_ms.txt" \
    -map 0:a -map_metadata 1 \
    -t 60 \
    -c:a aac -b:a 32k \
    -f mp4 \
    "${DEST_DIR}/synth_chapters_generic.m4b"

# ---------------------------------------------------------------------------
# Variant 2 — 60 s, 3 atoms at 0..20, 20..40, 40..60 s with narrative
# Japanese atom titles. Note: TIMEBASE=1/44100 is written into the metadata
# file but the ffmpeg mp4 muxer rewrites it to 1/1000 on output (known
# limitation, see header). The 20 s boundaries are still ms-aligned so the
# tick values land cleanly either way.
# ---------------------------------------------------------------------------
cat >"${WORK}/meta_narrative.txt" <<'EOF'
;FFMETADATA1
title=Synthetic Chapters (narrative)
artist=Synth Publisher

[CHAPTER]
TIMEBASE=1/1000
START=0
END=20000
title=序章

[CHAPTER]
TIMEBASE=1/1000
START=20000
END=40000
title=第一章

[CHAPTER]
TIMEBASE=1/1000
START=40000
END=60000
title=第二章
EOF

ffmpeg -y -hide_banner -loglevel error \
    -f lavfi -i "anullsrc=r=44100:cl=stereo" \
    -i "${WORK}/meta_narrative.txt" \
    -map 0:a -map_metadata 1 \
    -t 60 \
    -c:a aac -b:a 48k \
    -f mp4 \
    "${DEST_DIR}/synth_chapters_narrative.m4b"

# ---------------------------------------------------------------------------
# Variant 3 — 120 s, 3 atoms with a 5 s intro that should be filtered.
# 0..5 s   (intro, sub-threshold)
# 5..60 s  (chapter 1, 55 s)
# 60..120s (chapter 2, 60 s)
# Note: spec asked for "3 atoms with sub-60s intro" — concretely a 5 s intro
# atom labelled as "第1章" so the probe filter has to drop it by duration,
# not by title.
# ---------------------------------------------------------------------------
cat >"${WORK}/meta_intro.txt" <<'EOF'
;FFMETADATA1
title=Synthetic Chapters (intro)
artist=Synth Publisher

[CHAPTER]
TIMEBASE=1/1000
START=0
END=5000
title=第1章

[CHAPTER]
TIMEBASE=1/1000
START=5000
END=60000
title=第2章

[CHAPTER]
TIMEBASE=1/1000
START=60000
END=120000
title=第3章
EOF

ffmpeg -y -hide_banner -loglevel error \
    -f lavfi -i "anullsrc=r=22050:cl=stereo" \
    -i "${WORK}/meta_intro.txt" \
    -map 0:a -map_metadata 1 \
    -t 120 \
    -c:a aac -b:a 32k \
    -f mp4 \
    "${DEST_DIR}/synth_chapters_intro.m4b"

# ---------------------------------------------------------------------------
# Verify each artefact: nb_chapters, duration, time_base, atom titles.
# ---------------------------------------------------------------------------
for f in synth_chapters_generic.m4b synth_chapters_narrative.m4b synth_chapters_intro.m4b; do
    echo "--- ${f} ---"
    ffprobe -v error -hide_banner -show_chapters -show_format -print_format json \
        "${DEST_DIR}/${f}" \
        | jq '{
            nb_chapters: (.chapters | length),
            duration: .format.duration,
            atom_time_bases: ([.chapters[] | .time_base] | unique),
            atoms: ([.chapters[] | {
                title: .tags.title,
                start_time: .start_time,
                end_time: .end_time
            }])
        }'
    sz=$(stat -f '%z' "${DEST_DIR}/${f}" 2>/dev/null || stat -c '%s' "${DEST_DIR}/${f}" 2>/dev/null)
    sha=$(shasum -a 256 "${DEST_DIR}/${f}" | awk '{print $1}')
    printf "size=%s bytes\nsha256=%s\n" "$sz" "$sha"
done

echo ""
echo "OK three synthetic fixtures written to ${DEST_DIR}/"
