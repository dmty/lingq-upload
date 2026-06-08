#!/usr/bin/env bash
# Stand-in for ffmpeg used by cancellation tests. Sleeps for SLOW_TRANSCODE_SLEEP
# seconds (default 10), then exits 0. Honours -y / -i / -ss / -to etc. by
# ignoring them. Writes a stub file to the last positional arg if
# SLOW_TRANSCODE_WRITE_STUB=1.
# Use exec so the bash wrapper is replaced by sleep — killing the spawned
# child then terminates the actual sleep process, not just the shell.
if [ "${SLOW_TRANSCODE_WRITE_STUB:-0}" = "1" ]; then
  for arg in "$@"; do dst="$arg"; done
  : > "$dst"
fi
exec sleep "${SLOW_TRANSCODE_SLEEP:-10}"
