#!/usr/bin/env bash
# Read-only capture of a connected Android device into a scrubbed fixture profile.
#
#   capture-device.sh <adb-serial-or-transport> <profile-name> [--leanback true|false|unknown] [--notes TEXT]
#
# Writes v2/crates/core/tests/fixtures/devices/<profile-name>/. Every device command below is a
# read: getprop, pm list, settings list, dumpsys, cmd package query/resolve, wm size/density
# (no argument), top, cmd role get-role-holders. Nothing here may change device state; keep it so.
#
# Extra strings to scrub (e.g. a person's name) can be passed as a newline-separated list in
# CAPTURE_EXTRA_REDACT; each is replaced case-insensitively with "REDACTED".
set -euo pipefail

ADB="${ADB:-$HOME/Library/Application Support/ShieldOptimizer/platform-tools/adb}"

usage() {
  echo "usage: $0 <adb-serial-or-transport> <profile-name> [--leanback true|false|unknown] [--notes TEXT]" >&2
  echo "          [--anonymize-third-party [--keep-package PKG]...]" >&2
  exit 2
}
[[ $# -ge 2 ]] || usage
# `--scrub-only <profile-dir>` scrubs an existing profile in place (one built
# from a recorded session by `e2e_server profile-from-session`) and runs the
# same leak checks, without touching any device.
scrub_only=0
if [[ "$1" == "--scrub-only" ]]; then
  scrub_only=1
  transport=""; name="$(basename "$2")"
  scrub_dir="$(cd "$2" && pwd)"; shift 2
else
  transport="$1"; name="$2"; shift 2
fi
leanback=null; notes=""; anonymize=0; keep_pkgs=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --leanback)
      case "${2:-}" in true) leanback=true ;; false) leanback=false ;; unknown) leanback=null ;; *) usage ;; esac
      shift 2 ;;
    --notes) notes="${2:-}"; shift 2 ;;
    --anonymize-third-party) anonymize=1; shift ;;
    --keep-package) [[ -n "${2:-}" ]] || usage; keep_pkgs+="$2"$'\n'; shift 2 ;;
    *) usage ;;
  esac
done
[[ "$name" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "profile name must be [A-Za-z0-9._-]+" >&2; exit 2; }
[[ $scrub_only -eq 1 || -x "$ADB" ]] || { echo "adb not found at $ADB" >&2; exit 1; }

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="$here/../crates/core/tests/fixtures/devices/$name"
raw="$(mktemp -d)"
trap 'rm -rf "$raw"' EXIT

if [[ $scrub_only -eq 1 ]]; then
  out="$scrub_dir"
  cp -R "$out/." "$raw/"
  rm -f "$raw/device.json"
  serial="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["serial"])' "$out/device.json")"
  model="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("model",""))' "$out/device.json")"
else
host_devices="$("$ADB" devices -l | tr -d '\r')"
state="$(awk -v t="$transport" '$1 == t { print $2 }' <<<"$host_devices")"
if [[ "$state" != "device" ]]; then
  echo "transport '$transport' is not listed as 'device' by adb devices -l (state: ${state:-absent}); not connecting, skipping" >&2
  exit 3
fi

# A command the device does not support still leaves its error text in the fixture, which is
# exactly what the simulator should replay, so a non-zero exit is not fatal here.
dev() { { "$ADB" -s "$transport" shell "$1" </dev/null 2>&1 || true; } | tr -d '\r'; }
[[ -n "${CAPTURE_TRACE:-}" ]] && set -x

dev 'getprop' >"$raw/getprop.txt"
dev 'pm list packages' >"$raw/pm-list-packages.txt"
for f in d e 3 s u; do dev "pm list packages -$f" >"$raw/pm-list-packages-$f.txt"; done
for ns in global secure system; do dev "settings list $ns" >"$raw/settings-$ns.txt"; done
dev 'dumpsys meminfo' >"$raw/dumpsys-meminfo.txt"
dev 'dumpsys diskstats' >"$raw/dumpsys-diskstats.txt"
dev 'cmd package query-activities -a android.intent.action.MAIN -c android.intent.category.HOME' >"$raw/home-query-activities.txt"
dev 'cmd package query-activities --components -a android.intent.action.MAIN -c android.intent.category.HOME' >"$raw/home-query-activities-components.txt"
dev 'cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME' >"$raw/home-resolve-activity.txt"
dev 'cmd role get-role-holders android.app.role.HOME' >"$raw/role-home.txt"
dev 'wm size' >"$raw/wm-size.txt"
dev 'wm density' >"$raw/wm-density.txt"
dev 'top -b -n 1' >"$raw/top.txt"

serial="$(sed -n 's/^\[ro\.serialno\]: \[\(.*\)\]$/\1/p' "$raw/getprop.txt")"
model="$(sed -n 's/^\[ro\.product\.model\]: \[\(.*\)\]$/\1/p' "$raw/getprop.txt")"
[[ -n "$serial" ]] || { echo "could not read ro.serialno" >&2; exit 1; }

mdns="$("$ADB" mdns services 2>&1 | tr -d '\r' || true)"
awk -v s="$serial" 'index($1, s) > 0' <<<"$mdns" >"$raw/mdns-services.txt"
mdns_addrs="$(awk '{ print $NF }' "$raw/mdns-services.txt")"
awk -v t="$transport" -v s="$serial" -v addrs="$mdns_addrs" '
  BEGIN { n = split(addrs, a, "\n"); for (i = 1; i <= n; i++) if (a[i] != "") want[a[i]] = 1 }
  NR > 1 && NF > 0 && ($1 == t || index($1, s) > 0 || ($1 in want))
' <<<"$host_devices" >"$raw/adb-devices.txt"

mkdir -p "$raw/dumpsys-package"
home_pkgs="$( { sed -n 's/.*packageName=\([A-Za-z0-9._]*\).*/\1/p' "$raw/home-query-activities.txt"
                grep -oE '^[A-Za-z0-9._]+/' "$raw/home-query-activities-components.txt" | tr -d '/' || true; } | sort -u)"
for pkg in $home_pkgs; do
  [[ "$pkg" =~ ^[A-Za-z0-9._]+$ ]] || continue
  dev "dumpsys package $pkg" | head -n 120 >"$raw/dumpsys-package/$pkg.txt" || true
done

rm -rf "$out"
mkdir -p "$out"
fi

CAPTURE_ANONYMIZE="$anonymize" CAPTURE_KEEP_PKGS="$keep_pkgs" python3 - "$raw" "$out" "$model" <<'PY'
import ipaddress, os, re, sys

raw, out, model = sys.argv[1], sys.argv[2], sys.argv[3]
neutral = f"Test {model}" if model else "Test Device"

files = {}
for root, _, names in os.walk(raw):
    for n in names:
        p = os.path.join(root, n)
        with open(p, encoding="utf-8", errors="replace") as fh:
            files[os.path.relpath(p, raw)] = fh.read().replace("\r\n", "\n").replace("\r", "\n")

PROP = re.compile(r"^(\[([^\]]+)\]: \[)(.*)(\])$")
SETTING = re.compile(r"^(([A-Za-z0-9_.\-]+)=)(.*)()$")

def kv(line):
    m = PROP.match(line) or SETTING.match(line)
    return (m.group(1), m.group(2), m.group(3), m.group(4)) if m else None

NAME_KEYS = re.compile(r"^(device_name|bluetooth_name|friendly_name|device_friendly_name|"
                       r"persist\.sys\.device_name|net\.hostname|persist\.sys\.friendly_name|"
                       r"persist\.vendor\.device_name|wifi_p2p_device_name|"
                       r"ro\.boot\.hostname|bluetooth\.device\.name|persist\.bluetooth\.name|"
                       r".*netbios.*)$", re.I)
TZ_KEYS = re.compile(r"^(persist\.sys\.timezone|time_zone|timezone)$", re.I)
SSID_KEYS = re.compile(r"ssid", re.I)
SECRET_KEYS = re.compile(r"(^android_id$|bluetooth_address|advertising_id|token|gaia|account|"
                         r"wifimacaddr|btmacaddr|ethaddr|^ro\.boot\.mac$|imei|meid|iccid|imsi|"
                         r"msisdn|phone_number|line1|^ro\.boot\.cpuid$|instance_id|install_id|session_id|"
                         r"client_id)", re.I)

originals, ssids = set(), set()
for text in files.values():
    for line in text.split("\n"):
        p = kv(line)
        if not p:
            continue
        _, k, v, _ = p
        v = v.strip()
        if not v or v in ("null", "0", "1"):
            continue
        if NAME_KEYS.search(k) and v != model:
            originals.add(v)
        if SSID_KEYS.search(k):
            ssids.add(v.strip('"'))
    for m in re.finditer(r'\bSSID\b["\']?\s*[:=]\s*"([^"\n]+)"', text, re.I):
        ssids.add(m.group(1))
ssids -= {"", "<unknown ssid>", "TestSSID"}

extra = [s.strip() for s in os.environ.get("CAPTURE_EXTRA_REDACT", "").splitlines() if s.strip()]

# A personal phone's installed apps say a lot about its owner (bank, doctor, city, employer), so
# --anonymize-third-party renames every non-system package to com.example.appN in every file,
# consistently, keeping the list's shape. System packages and --keep-package names stay.
pkg_map = {}
if os.environ.get("CAPTURE_ANONYMIZE") == "1":
    def pkgs(rel):
        return {l[len("package:"):].strip() for l in files.get(rel, "").split("\n") if l.startswith("package:")}
    keep = {p.strip() for p in os.environ.get("CAPTURE_KEEP_PKGS", "").splitlines() if p.strip()}
    third = (pkgs("pm-list-packages-3.txt") | (pkgs("pm-list-packages-u.txt") - pkgs("pm-list-packages-s.txt"))) - keep
    for i, p in enumerate(sorted(third), 1):
        pkg_map[p] = f"com.example.app{i}"
PKG_RE = (re.compile(r"(?<![\w.])(" + "|".join(re.escape(p) for p in sorted(pkg_map, key=len, reverse=True)) + r")(?![\w])")
          if pkg_map else None)

ip4_map, ip6_map, mac_map = {}, {}, {}
CGNAT = ipaddress.ip_network("100.64.0.0/10")
DOC4 = ipaddress.ip_network("192.0.2.0/24")

IP4 = re.compile(r"(?<![0-9A-Za-z.])((?:\d{1,3}\.){3}\d{1,3})(?![\w]|\.\d)")
def sub_ip4(m):
    s = m.group(1)
    try:
        a = ipaddress.IPv4Address(s)
    except ValueError:
        return s
    if a.is_loopback or a.is_unspecified or s.startswith("255.") or a in DOC4:
        return s
    if not (a.is_private or a in CGNAT):
        return s
    if s not in ip4_map:
        ip4_map[s] = f"192.0.2.{len(ip4_map) + 1}"
    return ip4_map[s]

MAC = re.compile(r"(?<![0-9A-Za-z:-])((?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2})(?![0-9A-Za-z:-])")
def sub_mac(m):
    s = m.group(1)
    low = s.lower().replace("-", ":")
    if low in ("00:00:00:00:00:00", "ff:ff:ff:ff:ff:ff") or low.startswith("02:00:00:00:00:"):
        return s
    if low not in mac_map:
        mac_map[low] = "02:00:00:00:00:%02x" % (len(mac_map) + 1)
    return mac_map[low]

IP6 = re.compile(r"(?<![0-9A-Za-z:])([0-9A-Fa-f]{0,4}(?::[0-9A-Fa-f]{0,4}){2,7})(?![\w:])")
def sub_ip6(m):
    s = m.group(1)
    if "::" not in s and s.count(":") != 7:
        return s
    try:
        a = ipaddress.IPv6Address(s)
    except ValueError:
        return s
    if a.is_loopback or a.is_unspecified or a in ipaddress.ip_network("2001:db8::/32"):
        return s
    if s not in ip6_map:
        ip6_map[s] = f"2001:db8::{len(ip6_map) + 1:x}"
    return ip6_map[s]

EMAIL = re.compile(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}")

def scrub_kv_line(line):
    p = kv(line)
    if not p:
        return line
    pre, key, val, post = p
    if not val.strip() or val.strip() == "null":
        return line
    if SECRET_KEYS.search(key):
        return pre + "REDACTED" + post
    if TZ_KEYS.search(key):
        return pre + "Etc/UTC" + post
    if SSID_KEYS.search(key):
        return pre + "TestSSID" + post
    if NAME_KEYS.search(key) and val != model:
        return pre + neutral + post
    return line

for rel in sorted(files):
    text = "\n".join(scrub_kv_line(l) for l in files[rel].split("\n"))
    if PKG_RE:
        text = PKG_RE.sub(lambda m: pkg_map[m.group(1)], text)
        text = re.sub(r"\b(com\.example\.app\d+)/[\w.$]+", r"\1/\1.Component", text)
        rel = PKG_RE.sub(lambda m: pkg_map[m.group(1)], rel)
    for o in sorted(originals, key=len, reverse=True):
        if len(o) >= 3:
            text = text.replace(o, neutral)
    for s in sorted(ssids, key=len, reverse=True):
        if len(s) >= 2:
            text = re.sub(r"(?<![\w])" + re.escape(s) + r"(?![\w])", "TestSSID", text)
    for e in extra:
        text = re.sub(re.escape(e), "REDACTED", text, flags=re.I)
    text = re.sub(r'(\bSSID\b["\']?\s*[:=]\s*)"[^"\n]*"', r'\1"TestSSID"', text, flags=re.I)
    text = EMAIL.sub("user@example.com", text)
    text = MAC.sub(sub_mac, text)
    text = IP6.sub(sub_ip6, text)
    text = IP4.sub(sub_ip4, text)
    dest = os.path.join(out, rel)
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    with open(dest, "w", encoding="utf-8") as fh:
        fh.write(text)
PY

[[ $scrub_only -eq 1 ]] || python3 - "$out/device.json" "$name" "$serial" "$model" "$(date +%Y-%m-%d)" "$leanback" "$notes" <<'PY'
import json, sys
path, name, serial, model, captured, leanback, notes = sys.argv[1:]
doc = {"name": name, "serial": serial, "model": model, "captured": captured,
       "leanback": {"true": True, "false": False}.get(leanback), "notes": notes}
with open(path, "w") as fh:
    json.dump(doc, fh, indent=2)
    fh.write("\n")
PY

fail=0
rfc1918='(^|[^0-9.])(10\.[0-9]{1,3}|172\.(1[6-9]|2[0-9]|3[01])|192\.168)\.[0-9]{1,3}\.[0-9]{1,3}([^0-9]|$)'
macs='(^|[^0-9A-Za-z:-])([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}([^0-9A-Za-z:-]|$)'
emails='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
if grep -rEn "$rfc1918" "$out"; then echo "FAIL: private IPv4 left in $out" >&2; fail=1; fi
if grep -rEn "$macs" "$out" | grep -viE '(00:00:00:00:00:00|ff:ff:ff:ff:ff:ff|02:00:00:00:00:[0-9a-f]{2})'; then
  echo "FAIL: MAC address left in $out" >&2; fail=1
fi
if grep -rEoh "$emails" "$out" | grep -vx 'user@example\.com'; then echo "FAIL: email left in $out" >&2; fail=1; fi
if [[ $fail -ne 0 ]]; then
  echo "scrub check failed; output kept at $out for inspection - do not commit it" >&2
  exit 1
fi

if [[ $scrub_only -eq 1 ]]; then echo "scrubbed $out ($model, $serial)"; else echo "captured $model ($serial) via $transport -> $out"; fi
