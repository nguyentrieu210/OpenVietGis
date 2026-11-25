#!/usr/bin/env bash
# Isolated Chromium/API-fixture regression. Run in a clean development checkout.
# No live server, DB, credentials or production data are used.
set -euo pipefail
cd "$(dirname "$0")/.."
temp="$(mktemp -d)"
server=""
cleanup() {
  if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; fi
  rm -f "$temp/rewrite" "$temp/http.log"
  rmdir "$temp"
}
trap cleanup EXIT
ln -s "$PWD/apps/web-next/dist" "$temp/rewrite"
port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
python3 -m http.server "$port" --bind 127.0.0.1 --directory "$temp" >"$temp/http.log" 2>&1 &
server=$!
export OVG_TEST_URL="http://127.0.0.1:$port/rewrite/?live=1"
pnpm --filter @openvietgis/web-next typecheck
export VITE_OVG_TERRA_DRAFT_VALIDATION=1
pnpm --filter @openvietgis/web-next exec vite build --base /rewrite/
node apps/web-next/terra-draft-validation-smoke.mjs
export VITE_OVG_TERRA_DRAFT_VALIDATION=0
pnpm --filter @openvietgis/web-next exec vite build --base /rewrite/
if find apps/web-next/dist/assets -maxdepth 1 -name 'TerraDraftValidation-*' | grep -q .; then
  echo "ERROR: optional engine included in default build"; exit 1
fi
node apps/web-next/advanced-editor-smoke.mjs
node apps/web-next/core-toolkit-smoke.mjs
node apps/web-next/spatial-cache-smoke.mjs
pnpm --filter @openvietgis/spatial-workspace test
node --test apps/web-next/spatial-sync-core.test.mjs

# Mock relay is isolated on another loopback origin; no external CAD fetches.
export VITE_OVG_CAD_RELAY_ORIGIN=http://127.0.0.1:4281
pnpm --filter @openvietgis/web-next exec vite build --base /rewrite/
node apps/web-next/cad-editor-georef-smoke.mjs
node apps/web-next/cad-spatial-linkage-smoke.mjs
node apps/web-next/cesium-tiles-smoke.mjs
unset VITE_OVG_CAD_RELAY_ORIGIN
pnpm --filter @openvietgis/web-next exec vite build --base /rewrite/
