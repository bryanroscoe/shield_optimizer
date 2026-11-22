#!/usr/bin/env bash
set -euo pipefail

stage=${1:?Supply an empty runner directory for the emulator libraries}
mkdir -p "$stage/lists/partial" "$stage/archives/partial"
apt_options=(
  -o "Dir::State::lists=$stage/lists"
  -o "Dir::Cache::archives=$stage/archives"
  -o 'Dir::Cache::pkgcache='
  -o 'Dir::Cache::srcpkgcache='
)
apt-get "${apt_options[@]}" update
# Resolve dependencies against the runner's installed packages without writing to dpkg.
apt-get "${apt_options[@]}" -o Debug::NoLocking=true \
  --download-only --no-install-recommends --no-upgrade --assume-yes install libpulse0
for package in "$stage/archives/"*.deb; do
  [[ -f "$package" ]] || continue
  dpkg-deb -x "$package" "$stage"
done

lib="$stage/usr/lib/x86_64-linux-gnu"
pulse="$lib/libpulse.so.0"
if [[ ! -f "$pulse" ]]; then
  pulse=/usr/lib/x86_64-linux-gnu/libpulse.so.0
fi
LD_LIBRARY_PATH="$lib:$lib/pulseaudio${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  ldd "$pulse" | tee "$stage/pulse-dependencies.txt"
if grep -Fq 'not found' "$stage/pulse-dependencies.txt"; then
  echo 'PulseAudio has unresolved shared-library dependencies' >&2
  exit 1
fi
