#!/bin/bash
# Stitch the captured frames into gallery GIFs:
#   frames/       -> gallery.gif        (dark theme)
#   frames-light/ -> gallery-light.gif  (light theme)
# Two-pass palette (palettegen -> paletteuse) for clean color.
# Run after capture.mjs (or just `npm run screenshots`, which does both).

set -euo pipefail
cd "$(dirname "$0")"

WIDTH=1280        # downscale from the 2560px retina captures
SECONDS_PER=2.2   # hold each screen this long

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg not found - install it (brew install ffmpeg) to build the GIF." >&2
  exit 1
fi

build_gif() {
  local frames_dir="$1" out="$2"
  local frames=( "$frames_dir"/*.png )
  if [[ ${#frames[@]} -eq 0 || ! -e "${frames[0]}" ]]; then
    echo "No frames in $frames_dir - skipping $out." >&2
    return 0
  fi

  # concat demuxer playlist: each frame held SECONDS_PER. The concat demuxer
  # ignores the final entry's duration, so the last frame is listed twice.
  local list palette
  list="$(mktemp)"
  palette="$(mktemp -t palette.XXXXXX).png"
  for f in "${frames[@]}"; do
    printf "file '%s'\nduration %s\n" "$PWD/$f" "$SECONDS_PER" >> "$list"
  done
  local last="${frames[$((${#frames[@]} - 1))]}"  # bash 3.2 (macOS): no negative indices
  printf "file '%s'\n" "$PWD/$last" >> "$list"

  echo "Building palette for ${out}..."
  ffmpeg -y -f concat -safe 0 -i "$list" \
    -vf "scale=${WIDTH}:-1:flags=lanczos,palettegen=stats_mode=full" \
    "$palette" >/dev/null 2>&1

  echo "Encoding ${out}..."
  ffmpeg -y -f concat -safe 0 -i "$list" -i "$palette" \
    -lavfi "scale=${WIDTH}:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3" \
    -loop 0 "${out}" >/dev/null 2>&1

  rm -f "$list" "$palette"
  echo "OK $(pwd)/${out} ($(du -h "${out}" | cut -f1))"
}

build_gif "frames" "gallery.gif"
build_gif "frames-light" "gallery-light.gif"
