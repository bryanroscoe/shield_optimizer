#!/bin/bash
# Stitch frames/ into gallery.gif (two-pass palette). Run after capture.mjs, or
# just `npm run screenshots`, which does both.

set -euo pipefail
cd "$(dirname "$0")"

WIDTH=412         # phone width in CSS px, downscaled from the 2x captures
SECONDS_PER=2.0

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg not found - install it (brew install ffmpeg) to build the GIF." >&2
  exit 1
fi

frames=( frames/*.png )
if [[ ${#frames[@]} -eq 0 || ! -e "${frames[0]}" ]]; then
  echo "No frames in frames/ - run capture.mjs first." >&2
  exit 1
fi

# The concat demuxer ignores the final entry's duration, so list the last twice.
list="$(mktemp)"
palette="$(mktemp -t palette.XXXXXX).png"
trap 'rm -f "$list" "$palette"' EXIT
for f in "${frames[@]}"; do
  printf "file '%s'\nduration %s\n" "$PWD/$f" "$SECONDS_PER" >> "$list"
done
printf "file '%s'\n" "$PWD/${frames[$((${#frames[@]} - 1))]}" >> "$list"

ffmpeg -y -f concat -safe 0 -i "$list" \
  -vf "scale=${WIDTH}:-1:flags=lanczos,palettegen=stats_mode=full" "$palette" >/dev/null 2>&1
ffmpeg -y -f concat -safe 0 -i "$list" -i "$palette" \
  -lavfi "scale=${WIDTH}:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3" \
  -loop 0 gallery.gif >/dev/null 2>&1
echo "OK $(pwd)/gallery.gif ($(du -h gallery.gif | cut -f1))"
