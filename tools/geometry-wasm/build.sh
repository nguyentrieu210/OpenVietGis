#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
rustc +1.85.1 --edition 2021 --crate-name ovg_geometry --crate-type cdylib \
  --target wasm32-unknown-unknown -C opt-level=z -C panic=abort -C strip=symbols \
  tools/geometry-wasm/lib.rs -o apps/web-next/src/wasm/geometry.wasm
sha256sum apps/web-next/src/wasm/geometry.wasm
