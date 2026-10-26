#!/bin/sh
set -eu
cd "$(dirname "$0")"
unset VITE_DEMO VITE_E2E
export VITE_LOCAL_PREVIEW=1
export SHIELD_OPTIMIZER_LOCAL_PREVIEW=1
export SHIELD_OPTIMIZER_TRANSPORT=subprocess
exec npm run tauri -- dev --no-watch --config '{"build":{"beforeDevCommand":"npm run dev -- --port 1439","devUrl":"http://localhost:1439"}}'
