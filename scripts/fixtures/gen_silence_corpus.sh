#!/usr/bin/env bash
#
# Regenerate the silence-corpus carver fixtures (clip_a, clip_b, clip_c).
#
# Output: src-tauri/tests/fixtures/audio/silence_corpus/clip_{a,b,c}.wav.
# Each clip is a 1 kHz sine tone at 16 kHz mono with `volume=0` muting over the
# gaps between named segments; clip_c ends mid-silence to exercise the EOF
# silence_end synthesis. The committed `clip_*.golden_offsets.json` files
# pin the expected silencedetect offsets per absorb policy; if you change the
# segment timings here, regenerate the goldens and update the sha256 constants
# in `src-tauri/tests/carver_absorb_test.rs`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST_DIR="${REPO_ROOT}/src-tauri/tests/fixtures/audio/silence_corpus"
mkdir -p "$DEST_DIR"

if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "ERROR ffmpeg is required" >&2
    exit 1
fi

# Render a single clip: 1 kHz sine at 16 kHz mono, muted over the gaps between
# segments and over any trailing gap before the total duration.
# $1 = output path, $2 = total duration, $3... = "start,end" tone segments.
gen_clip() {
    local out="$1"; shift
    local total="$1"; shift
    local segs=("$@")
    local filter="sine=frequency=1000:sample_rate=16000:duration=${total}"
    local i
    for (( i = 0; i < ${#segs[@]}; i++ )); do
        local gap_start="${segs[i]##*,}"
        local gap_end
        if (( i < ${#segs[@]} - 1 )); then
            gap_end="${segs[i+1]%%,*}"
        else
            gap_end="$total"
        fi
        if [[ "$gap_start" != "$gap_end" ]]; then
            filter+=",volume=enable='between(t,${gap_start},${gap_end})':volume=0"
        fi
    done
    rm -f "$out"
    ffmpeg -hide_banner -v error -y -f lavfi -i "$filter" -ac 1 -ar 16000 "$out"
}

gen_clip "${DEST_DIR}/clip_a.wav" "12.0" "0.0,5.0" "6.0,9.0" "10.0,12.0"
gen_clip "${DEST_DIR}/clip_b.wav" "11.0" "0.0,4.0" "5.5,7.0" "8.5,11.0"
gen_clip "${DEST_DIR}/clip_c.wav" "9" "0,3" "4,7"

echo "Wrote: ${DEST_DIR}/clip_a.wav"
echo "Wrote: ${DEST_DIR}/clip_b.wav"
echo "Wrote: ${DEST_DIR}/clip_c.wav"
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${DEST_DIR}"/clip_*.wav
elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${DEST_DIR}"/clip_*.wav
fi
