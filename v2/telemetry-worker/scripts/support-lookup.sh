#!/usr/bin/env bash
# Everything stored for one install, by the support ID a user quotes: the
# short form the app shows (7D0B-0B9E, dash optional) or the full UUID from a
# bug report or Send logs.
#
#   export ATVOPT_ADMIN_TOKEN="$(op read 'op://Bryan Code/ATV Optimizer telemetry ADMIN_TOKEN/credential' --account my.1password.com)"
#   scripts/support-lookup.sh 7D0B-0B9E              # readable report
#   scripts/support-lookup.sh 7D0B-0B9E --limit 200  # more recent events (max 200)
#   scripts/support-lookup.sh 7D0B-0B9E --json       # the raw JSON
#   scripts/support-lookup.sh --upload 12            # the text of one Send logs upload
#
# ATV_TELEMETRY_URL overrides the production URL. The token goes to curl as a
# header on stdin, so it never shows up in a process list, and nothing here
# prints it. Needs curl and Node 22.18+.
set -euo pipefail

url="${ATV_TELEMETRY_URL:-https://atv-optimizer-telemetry.bryanroscoe.workers.dev}"
url="${url%/}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  sed -n '2,13p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

if [ -z "${ATVOPT_ADMIN_TOKEN:-}" ]; then
  echo "Set ATVOPT_ADMIN_TOKEN first, for example:" >&2
  echo "  export ATVOPT_ADMIN_TOKEN=\"\$(op read 'op://Bryan Code/ATV Optimizer telemetry ADMIN_TOKEN/credential' --account my.1password.com)\"" >&2
  exit 2
fi

get() {
  printf 'Authorization: Bearer %s\n' "$ATVOPT_ADMIN_TOKEN" |
    curl --silent --show-error --fail-with-body --max-time 30 -H @- "$url$1"
}

id=""
json=0
limit=50
upload=""
while [ $# -gt 0 ]; do
  case "$1" in
    --json) json=1 ;;
    --limit) shift; limit="${1:-}" ;;
    --upload) shift; upload="${1:-}" ;;
    -h|--help) usage ;;
    -*) usage ;;
    *) id="$1" ;;
  esac
  shift
done

if [ -n "$upload" ]; then
  [[ "$upload" =~ ^[0-9]{1,12}$ ]] || { echo "--upload takes the upload number from the report." >&2; exit 2; }
  get "/v1/admin/uploads/$upload"
  echo
  exit 0
fi

shopt -s nocasematch
if ! [[ "$id" =~ ^[0-9a-f]{4}-?[0-9a-f]{4}$ || "$id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]]; then
  echo "Not a support ID: '$id'. Use the 8-character form (7D0B-0B9E) or the full UUID." >&2
  exit 2
fi
shopt -u nocasematch
[[ "$limit" =~ ^[0-9]{1,3}$ ]] || { echo "--limit takes a number from 1 to 200." >&2; exit 2; }

if [ "$json" = 1 ]; then
  get "/v1/admin/support/$id?limit=$limit"
  echo
else
  get "/v1/admin/support/$id?limit=$limit" | node "$here/support-lookup.ts"
fi
