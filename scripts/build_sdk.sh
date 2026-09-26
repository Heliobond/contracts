#!/usr/bin/env bash
# Generate the @heliobond/contracts-sdk TypeScript package (#624).
#
# 1. `stellar contract bindings typescript` for each contract WASM, copied into
#    sdk/src/<contract>/ (generated, not committed).
# 2. sdk/src/networks.ts with per-network contract IDs from deploy/*.json.
#
# Usage: scripts/build_sdk.sh [version]
#   Expects `stellar contract build` to have run (WASMs under target/).
#   If a version is given (e.g. v1.2.0 or 1.2.0) it is written to
#   sdk/package.json.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WASM_DIR="$ROOT/target/wasm32v1-none/release"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

for contract in project_registry investment_vault; do
  wasm="$WASM_DIR/$contract.wasm"
  [ -f "$wasm" ] || { echo "missing $wasm — run \`stellar contract build\` first" >&2; exit 1; }
  stellar contract bindings typescript --wasm "$wasm" --output-dir "$TMP/$contract" --overwrite
  rm -rf "$ROOT/sdk/src/$contract"
  mkdir -p "$ROOT/sdk/src/$contract"
  cp "$TMP/$contract/src/index.ts" "$ROOT/sdk/src/$contract/index.ts"
  echo "generated sdk/src/$contract"
done

python3 "$ROOT/scripts/gen_sdk_networks.py"

if [ "${1:-}" != "" ]; then
  version="${1#v}"
  (cd "$ROOT/sdk" && npm pkg set version="$version")
  echo "sdk version -> $version"
fi
