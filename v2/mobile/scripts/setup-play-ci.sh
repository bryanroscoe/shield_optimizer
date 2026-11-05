#!/usr/bin/env bash
# Load the Play release inputs into the `google-play` GitHub environment that
# .github/workflows/mobile-play-release.yml uses. Run it once, and again after
# any of these rotates. It reads the same local files as build-play-release.py:
#
#   ~/.config/atv-optimizer/play-upload.json         storeFile/storePassword/keyAlias/keyPassword
#   ~/.config/atv-optimizer/play-license-key         Play Console base64 RSA public key
#   ~/.config/atv-optimizer/play-service-account.json Google Cloud service-account key (JSON)
#
# Usage: v2/mobile/scripts/setup-play-ci.sh [--dry-run]
#
# Secret values go to `gh` on stdin, never on a command line, and are never
# printed. The environment only deploys from `main` and `mobile-v*` tags.
set -euo pipefail

REPO="${ATVOPT_REPO:-bryanroscoe/shield_optimizer}"
ENVIRONMENT=google-play
CONFIG="${ATVOPT_CONFIG_DIR:-$HOME/.config/atv-optimizer}"
SIGNING="$CONFIG/play-upload.json"
LICENSE="$CONFIG/play-license-key"
ACCOUNT="$CONFIG/play-service-account.json"
DRY_RUN=0
[[ "${1:-}" == --dry-run ]] && DRY_RUN=1

for file in "$SIGNING" "$LICENSE" "$ACCOUNT"; do
  if [[ ! -f "$file" ]]; then
    echo "error: missing $file (see v2/mobile/RELEASE.md \"Automated release (CI)\")" >&2
    exit 1
  fi
done

field() { python3 -c 'import json, sys; sys.stdout.write(json.load(open(sys.argv[1]))[sys.argv[2]])' "$SIGNING" "$1"; }
python3 -c 'import json, sys; a = json.load(open(sys.argv[1])); assert a["type"] == "service_account" and a["client_email"] and a["private_key"]' "$ACCOUNT" \
  || { echo "error: $ACCOUNT is not a service-account key" >&2; exit 1; }

STORE_FILE="$(field storeFile)"
KEY_ALIAS="$(field keyAlias)"
ATVOPT_KEYSTORE_PASSWORD="$(field storePassword)"
export ATVOPT_KEYSTORE_PASSWORD
CERT_SHA256="$(keytool -exportcert -keystore "$STORE_FILE" -alias "$KEY_ALIAS" -storepass:env ATVOPT_KEYSTORE_PASSWORD \
  | shasum -a 256 | cut -d' ' -f1)"
LICENSE_KEY="$(tr -d '[:space:]' < "$LICENSE")"
SERVICE_EMAIL="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["client_email"])' "$ACCOUNT")"

echo "repository       $REPO"
echo "environment      $ENVIRONMENT (deploys from main and mobile-v* tags)"
echo "upload cert      $CERT_SHA256"
echo "key alias        $KEY_ALIAS"
echo "service account  $SERVICE_EMAIL"
if (( DRY_RUN )); then
  echo "dry run: nothing sent"
  exit 0
fi

gh api --silent -X PUT "repos/$REPO/environments/$ENVIRONMENT" --input - <<'JSON'
{"deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}}
JSON
existing="$(gh api "repos/$REPO/environments/$ENVIRONMENT/deployment-branch-policies" --jq '.branch_policies[] | "\(.type):\(.name)"')"
for policy in branch:main 'tag:mobile-v*'; do
  if ! grep -qxF "$policy" <<<"$existing"; then
    gh api --silent -X POST "repos/$REPO/environments/$ENVIRONMENT/deployment-branch-policies" \
      -f type="${policy%%:*}" -f name="${policy#*:}"
  fi
done

secret() { gh secret set "$1" --env "$ENVIRONMENT" --repo "$REPO"; }
base64 < "$STORE_FILE" | tr -d '\n' | secret ATVOPT_UPLOAD_KEYSTORE_B64
field storePassword | secret ATVOPT_UPLOAD_KEYSTORE_PASSWORD
field keyPassword | secret ATVOPT_UPLOAD_KEY_PASSWORD
secret PLAY_SERVICE_ACCOUNT_JSON < "$ACCOUNT"

gh variable set ATVOPT_UPLOAD_KEY_ALIAS --env "$ENVIRONMENT" --repo "$REPO" --body "$KEY_ALIAS"
gh variable set ATVOPT_UPLOAD_CERT_SHA256 --env "$ENVIRONMENT" --repo "$REPO" --body "$CERT_SHA256"
gh variable set ATVOPT_PLAY_LICENSE_KEY --env "$ENVIRONMENT" --repo "$REPO" --body "$LICENSE_KEY"
echo "Done. Check: Settings -> Environments -> $ENVIRONMENT."
