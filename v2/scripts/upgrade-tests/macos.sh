#!/usr/bin/env bash
# One macOS upgrade scenario across the 2.5.0 rename: install a published
# release from its DMG, seed and use it, replace the bundle the way
# tauri-plugin-updater does, then check the migrated data, the old data and
# the bundle name after a graceful quit.
#
# Every check is recorded (upgrade_check.py record/check-*) rather than
# aborting, so one run reports every failure. Evidence lands in $EVIDENCE.
#
# Environment: SCENARIO, OLD_VERSION, OLD_DMG, NEW_TARBALL, NEW_VERSION,
# EVIDENCE, and optionally HOMEBREW=1 (a cask is recorded for the app) or
# RETRY=1 (the first copy is made to fail).

set -uo pipefail

: "${SCENARIO:?}" "${OLD_VERSION:?}" "${OLD_DMG:?}" "${NEW_TARBALL:?}" "${NEW_VERSION:?}" "${EVIDENCE:?}"
HOMEBREW="${HOMEBREW:-0}"
RETRY="${RETRY:-0}"

here="$(cd "$(dirname "$0")" && pwd)"
tool="$here/upgrade_check.py"
telemetry_host="atv-optimizer-telemetry.bryanroscoe.workers.dev"
marker="shieldopt.ciUpgradeMarker"
marker_value="kept-across-rename-$SCENARIO"
run_seconds=20

mkdir -p "$EVIDENCE"
results="$EVIDENCE/results.jsonl"

support="$HOME/Library/Application Support"
old_data="$support/ShieldOptimizer"
new_data="$support/ATVOptimizer"
old_profile="$HOME/Library/WebKit/com.shieldoptimizer.app"
new_profile="$HOME/Library/WebKit/com.atvoptimizer.app"
old_id="com.shieldoptimizer.app"
new_id="com.atvoptimizer.app"
# 2.3.0 renamed the product (and so the bundle) to ATV Optimizer.
if python -I -c 'import sys; sys.exit(0 if tuple(map(int, sys.argv[1].split("."))) < (2, 3, 0) else 1)' "$OLD_VERSION"; then
  old_bundle="Shield Optimizer.app"
  old_ls_key="shieldopt.lastSeenVersion"
else
  old_bundle="ATV Optimizer.app"
  old_ls_key="shieldopt.notesSeenVersion"
fi
app_path="/Applications/$old_bundle"
caskroom=""

py() { python -I "$tool" "$@"; }
record() { # name status detail
  py record --results "$results" "--name=$1" --status "$2" "--detail=${3:--}"
}
ok_if() { # name condition-exit-status detail [soft]
  local status=ok
  if [[ "$2" != 0 ]]; then status=$([[ "${4:-}" == soft ]] && echo warn || echo fail); fi
  record "$1" "$status" "${3:-}"
}
fatal() {
  echo "::error::$1"
  record "harness completed without an error" fail "$1"
  finish
}
group() { echo "::group::$1"; }
endgroup() { echo "::endgroup::"; }

block_telemetry() {
  printf '\n0.0.0.0 %s\n:: %s\n' "$telemetry_host" "$telemetry_host" | sudo tee -a /etc/hosts >/dev/null
  sudo dscacheutil -flushcache
  sudo killall -HUP mDNSResponder 2>/dev/null || true
  sleep 1
  local addrs
  addrs=$(python -I -c "import socket,sys; print(' '.join(sorted({a[4][0] for a in socket.getaddrinfo(sys.argv[1], 443)})))" "$telemetry_host" 2>&1)
  if [[ -z "$addrs" ]] || grep -qvE '^((0\.0\.0\.0|::) ?)+$' <<<"$addrs"; then
    fatal "telemetry host still resolves to: $addrs"
  fi
  record "telemetry host blocked before any launch" ok "$addrs"
}

pids_of() { # bundle path
  pgrep -f "$1/Contents/MacOS/" || true
}

quit_app() { # bundle-id bundle-path label
  local id="$1" path="$2" label="$3" how
  osascript -e "tell application id \"$id\" to quit" >"$EVIDENCE/osascript-$label.txt" 2>&1 &
  local osa=$!
  for _ in $(seq 1 30); do
    [[ -z "$(pids_of "$path")" ]] && break
    sleep 1
  done
  kill "$osa" 2>/dev/null || true
  if [[ -z "$(pids_of "$path")" ]]; then
    how="quit gracefully"
    ok_if "$label quit gracefully" 0 "via Apple Event quit to $id"
  else
    how="killed"
    pkill -9 -f "$path/Contents/MacOS/" || true
    ok_if "$label quit gracefully" 1 "still running 30s after the quit event; killed ($(cat "$EVIDENCE/osascript-$label.txt"))"
  fi
  # WebKit's networking process flushes localStorage on its way out.
  sleep 5
  echo "$label: $how"
}

run_app() { # mode(exec|open) bundle-path bundle-id label
  local mode="$1" path="$2" id="$3" label="$4" exe
  exe=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$path/Contents/Info.plist")
  if [[ "$mode" == exec ]]; then
    # tauri::process::restart reads CFBundleExecutable and spawns it directly.
    "$path/Contents/MacOS/$exe" >"$EVIDENCE/stdout-$label.txt" 2>&1 &
  else
    open -n --stdout "$EVIDENCE/stdout-$label.txt" --stderr "$EVIDENCE/stdout-$label.txt" "$path"
  fi
  sleep "$run_seconds"
  if [[ -n "$(pids_of "$path")" ]]; then
    ok_if "$label stayed up for ${run_seconds}s" 0 "$path ($mode)"
  else
    ok_if "$label stayed up for ${run_seconds}s" 1 "not running after ${run_seconds}s ($path, $mode)"
  fi
  quit_app "$id" "$path" "$label"
}

save_state() { # label
  py listing --out "$EVIDENCE/listing-$1.txt" "$old_data" "$new_data" || true
  py listing --max-depth 6 --out "$EVIDENCE/listing-profiles-$1.txt" "$old_profile" "$new_profile" || true
  py listing --max-depth 2 --out "$EVIDENCE/listing-apps-$1.txt" "/Applications/Shield Optimizer.app" "/Applications/ATV Optimizer.app" "$HOME/Library/WebKit" || true
  for b in "Shield Optimizer.app" "ATV Optimizer.app"; do
    plist="/Applications/$b/Contents/Info.plist"
    [[ -f "$plist" ]] && plutil -convert xml1 -o "$EVIDENCE/Info-$1-${b%.app}.plist" "$plist"
  done
  pgrep -fl 'Optimizer|WebKit' >"$EVIDENCE/processes-$1.txt" || true
}

count_copied() { # item
  py count-log --logs "$new_data/logs" --pattern 'rename migration' --pattern 'copied' --pattern "$1" 2>/dev/null || echo 0
}

check_migrated() { # label
  local label="$1" extra=()
  py check-seeded --results "$results" --name "seeded data copied byte-identical ($label)" --old "$old_data" --new "$new_data"
  py check-telemetry --results "$results" --name "telemetry.json kept (same install id, still off) ($label)" --new "$new_data"
  py check-ls --results "$results" --name "localStorage marker survived in the new profile ($label)" \
    --kind webkit --profile "$new_profile" --key "$marker" --value "$marker_value"
  py check-ls --results "$results" --name "old app's own localStorage key survived ($label)" \
    --kind webkit --profile "$new_profile" --key "$old_ls_key" --soft
  py check-no-temps --results "$results" --name "no *.migrating-* folders left ($label)" "$support" "$HOME/Library/WebKit"
  # A launch whose data copy failed runs on the legacy folder, so it adds
  # its own log there. Every file that was already there must stay as it was.
  [[ "$RETRY" == 1 ]] && extra=(--allow-added logs)
  py check-unchanged --results "$results" --name "old data folder unchanged ($label)" \
    --manifest "$EVIDENCE/manifest-old-data.json" --root "$old_data" ${extra[@]+"${extra[@]}"}
  py check-unchanged --results "$results" --name "old profile unchanged ($label)" \
    --manifest "$EVIDENCE/manifest-old-profile.json" --root "$old_profile"
}

check_bundle() { # expected-path
  local want="$1" other id version
  if [[ "$want" == "/Applications/ATV Optimizer.app" ]]; then other="/Applications/Shield Optimizer.app"; else other="/Applications/ATV Optimizer.app"; fi
  local present rc
  present=$(find /Applications -maxdepth 1 -name '*Optimizer*.app' | tr '\n' ' ')
  [[ -d "$want" ]]; rc=$?
  ok_if "bundle is at $want" "$rc" "$present"
  [[ ! -e "$other" ]]; rc=$?
  ok_if "no bundle left at $other" "$rc" "$present"
  if [[ -f "$want/Contents/Info.plist" ]]; then
    id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$want/Contents/Info.plist")
    version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$want/Contents/Info.plist")
    [[ "$id" == "$new_id" && "$version" == "$NEW_VERSION" ]]; rc=$?
    ok_if "Info.plist has $new_id $NEW_VERSION" "$rc" "$id $version"
  fi
}

finish() {
  pkill -9 -f "Optimizer.app/Contents/MacOS/" 2>/dev/null || true
  [[ -n "$caskroom" ]] && rm -rf "$caskroom"
  [[ "$RETRY" == 1 ]] && chmod -R u+rw "$old_data" "$old_profile" 2>/dev/null
  save_state final
  for pair in "$old_data:old-data-logs" "$new_data:new-data-logs"; do
    src="${pair%%:*}/logs"
    [[ -d "$src" ]] && cp -R "$src" "$EVIDENCE/${pair##*:}"
  done
  [[ -d "$new_profile" ]] && py ls-dump --kind webkit --profile "$new_profile" --out "$EVIDENCE/localstorage-new-final.json"
  py manifest --root "$new_data" --out "$EVIDENCE/manifest-new-data-final.json" || true
  exit 0
}

group "Preconditions"
block_telemetry
for p in "$old_data" "$new_data" "$old_profile" "$new_profile" "/Applications/Shield Optimizer.app" "/Applications/ATV Optimizer.app"; do
  [[ -e "$p" ]] && fatal "$p exists before the test started"
done
for root in /opt/homebrew/Caskroom /usr/local/Caskroom; do
  if compgen -G "$root/shield-optimizer*" >/dev/null || compgen -G "$root/atv-optimizer*" >/dev/null; then
    fatal "a real cask for the app is already installed under $root"
  fi
done
endgroup

group "Install $old_bundle $OLD_VERSION from the DMG"
mnt=$(mktemp -d)
hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$OLD_DMG" >/dev/null || fatal "could not mount $OLD_DMG"
ditto "$mnt/$old_bundle" "$app_path" || fatal "could not copy $old_bundle to /Applications"
hdiutil detach "$mnt" >/dev/null || true
xattr -dr com.apple.quarantine "$app_path" 2>/dev/null || true
record "old $OLD_VERSION installed" ok "$app_path, owner $(stat -f %Su "$app_path")"
if [[ "$HOMEBREW" == 1 ]]; then
  # What `brew install --cask` leaves behind that the app looks for. The real
  # tap now serves 2.4.1 under a new token, so a 2.2.0 cask install can't be
  # reproduced; the Caskroom entry is what the app's check reads.
  for root in /opt/homebrew/Caskroom /usr/local/Caskroom; do
    if [[ -d "$(dirname "$root")" ]]; then
      caskroom="$root/shield-optimizer"
      mkdir -p "$caskroom/$OLD_VERSION" 2>/dev/null || sudo mkdir -p "$caskroom/$OLD_VERSION"
      break
    fi
  done
  [[ -n "$caskroom" ]] || fatal "no Homebrew prefix on this runner"
  record "Homebrew cask recorded (simulated Caskroom entry)" ok "$caskroom/$OLD_VERSION"
fi
endgroup

group "Seed the data folder before the old app first runs"
py seed --data "$old_data" || fatal "seeding failed"
endgroup

group "Run $OLD_VERSION once"
run_app open "$app_path" "$old_id" "old-$OLD_VERSION"
[[ -d "$old_profile" ]] || fatal "the old app did not create its WebKit profile at $old_profile"
py ls-dump --kind webkit --profile "$old_profile" --out "$EVIDENCE/localstorage-old.json"
py check-ls --results "$results" --name "old app wrote $old_ls_key on its own" \
  --kind webkit --profile "$old_profile" --key "$old_ls_key" --soft
py ls-inject --kind webkit --profile "$old_profile" --key "$marker" --value "$marker_value" ||
  fatal "could not add the localStorage marker to the old profile"
py manifest --root "$old_data" --out "$EVIDENCE/manifest-old-data.json"
py manifest --root "$old_profile" --out "$EVIDENCE/manifest-old-profile.json"
save_state before-upgrade
endgroup

if [[ "$RETRY" == 1 ]]; then
  group "Make one old data file and one old profile file unreadable"
  locked_data=$(find "$old_data/snapshots" -name '*.json' -type f | sort | head -1)
  locked_profile=$(find "$old_profile" -type f -name 'localstorage.sqlite3' | head -1)
  [[ -n "$locked_data" && -n "$locked_profile" ]] || fatal "nothing to lock"
  chmod 000 "$locked_data" "$locked_profile"
  record "files unreadable for the first launch" ok "$locked_data, $locked_profile"
  endgroup
fi

group "Replace the bundle the way tauri-plugin-updater 2.10 does"
# Extract the archive (dropping its top folder) into a temp dir, move the
# current bundle out, rename the temp dir to the bundle path, touch it.
extract=$(mktemp -d -t tauri_updated_app)
backup=$(mktemp -d -t tauri_current_app)
tar -xzf "$NEW_TARBALL" -C "$extract" --strip-components 1 || fatal "could not extract $NEW_TARBALL"
mv "$app_path" "$backup/current_app" || fatal "could not move the old bundle aside"
mv "$extract" "$app_path" || fatal "could not move the new bundle into place"
touch "$app_path"
rm -rf "$backup"
record "bundle replaced in place" ok "$app_path now holds $(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app_path/Contents/Info.plist")"
endgroup

if [[ "$HOMEBREW" == 1 ]]; then
  final_path="$app_path"
else
  final_path="/Applications/ATV Optimizer.app"
fi

group "First launch of the new build (relaunched like an update restart)"
run_app exec "$app_path" "$new_id" "new-first"
endgroup

log_dirs=(--logs "$new_data/logs" --logs "$old_data/logs")
if [[ "$RETRY" == 1 ]]; then
  group "Check the failed first copy, then remove the obstacle"
  py check-log --results "$results" --name "first launch logged the failed copy" "${log_dirs[@]}" \
    --pattern 'rename migration' --pattern 'fail' --min 1
  save_state after-failed-launch
  chmod 644 "$locked_data" "$locked_profile"
  endgroup
  group "Second launch: the retry"
  run_app open "$final_path" "$new_id" "new-retry"
  endgroup
fi

group "Check the migration and the bundle"
py ls-dump --kind webkit --profile "$new_profile" --out "$EVIDENCE/localstorage-new.json"
py check-log --results "$results" --name "log reports the data folder copied once" \
  --logs "$new_data/logs" --pattern 'rename migration' --pattern 'copied' --pattern 'data folder' --min 1 --max 1
py check-log --results "$results" --name "log reports the webview profile copied once" \
  --logs "$new_data/logs" --pattern 'rename migration' --pattern 'copied' --pattern 'webview profile' --min 1 --max 1
if [[ "$RETRY" != 1 ]]; then
  py check-log --results "$results" --name "no failed copy logged" "${log_dirs[@]}" \
    --pattern 'rename migration' --pattern 'fail' --min 0 --max 0
fi
check_migrated "after migration"
new_ls=$(find "$new_profile" -name 'localstorage.sqlite3' 2>/dev/null)
rc=0; [[ -n "$new_ls" ]] || rc=1
ok_if "new profile has a localStorage database" "$rc" "$new_profile"
check_bundle "$final_path"
if [[ "$HOMEBREW" == 1 ]]; then
  py check-log --results "$results" --name "log says the bundle was left for Homebrew" \
    --logs "$new_data/logs" --pattern 'legacy bundle rename' --pattern 'HomebrewManaged' --min 1
elif [[ "$old_bundle" == "Shield Optimizer.app" ]]; then
  py check-log --results "$results" --name "log reports the bundle renamed" \
    --logs "$new_data/logs" --pattern 'legacy bundle rename' --pattern 'Renamed' --min 1 --max 1
fi
save_state after-upgrade
endgroup

group "Next launch is a no-op"
data_before=$(count_copied 'data folder')
profile_before=$(count_copied 'webview profile')
run_app open "$final_path" "$new_id" "new-noop"
data_after=$(count_copied 'data folder')
profile_after=$(count_copied 'webview profile')
[[ "$data_before" == "$data_after" && "$profile_before" == "$profile_after" ]]; rc=$?
ok_if "no-op launch copied nothing again" "$rc" "copy lines: data $data_before -> $data_after, profile $profile_before -> $profile_after"
check_migrated "after the no-op launch"
check_bundle "$final_path"
endgroup

finish
