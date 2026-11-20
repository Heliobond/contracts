#!/usr/bin/env bash
# Example end-to-end happy-path walkthrough (#259): deposit -> fund_project ->
# certify_project, against an already-deployed ProjectRegistry + InvestmentVault
# (local Soroban network or testnet). Intended as an onboarding reference for
# new contributors, not a production script — it prints each `stellar contract
# invoke` call before running it so you can follow along, and pauses on error.
#
# Prerequisites:
#   - `stellar` CLI installed and a funded identity available.
#   - ProjectRegistry and InvestmentVault already deployed (see MIGRATION.md /
#     `make deploy-testnet`, or `stellar contract deploy` against a local
#     network). Fill in the contract IDs and addresses below, or export them
#     as environment variables before running.
#
# Usage:
#   REGISTRY_ID=C... VAULT_ID=C... USDC_SAC_ID=C... \
#   ADMIN=G... WHITELISTER=G... CREATOR=G... INVESTOR=G... \
#   NETWORK=testnet ./scripts/example_full_flow.sh

set -euo pipefail

: "${REGISTRY_ID:?Set REGISTRY_ID to the deployed ProjectRegistry contract id}"
: "${VAULT_ID:?Set VAULT_ID to the deployed InvestmentVault contract id}"
: "${USDC_SAC_ID:?Set USDC_SAC_ID to the USDC Stellar Asset Contract id}"
: "${ADMIN:?Set ADMIN to the source/identity used for admin-only calls}"
: "${WHITELISTER:?Set WHITELISTER to the source/identity used for whitelister-only calls}"
: "${CREATOR:?Set CREATOR to the project creator's address}"
: "${INVESTOR:?Set INVESTOR to the investor's address}"
NETWORK="${NETWORK:-testnet}"

run() {
  echo
  echo "\$ $*"
  "$@"
}

echo "== 1. Whitelist the project creator (whitelister-only) =="
run stellar contract invoke \
  --id "$REGISTRY_ID" --source "$WHITELISTER" --network "$NETWORK" \
  -- set_whitelist --account "$CREATOR" --status true

echo
echo "== 2. Create a project (creator-authorized) =="
# metadata_hash is a placeholder sha256 of whatever off-chain metadata blob
# `uri` points to (see #44 / verify_metadata_hash) — replace with a real hash.
run stellar contract invoke \
  --id "$REGISTRY_ID" --source "$CREATOR" --network "$NETWORK" \
  -- create_project \
  --creator "$CREATOR" \
  --uri "ipfs://QmExampleFlowMetadata" \
  --maturity_date 0 \
  --metadata_hash 0000000000000000000000000000000000000000000000000000000000000000

echo
echo "Note the project_id printed above (u32) — used as PROJECT_ID below."
: "${PROJECT_ID:?Set PROJECT_ID to the id returned by create_project above, then re-run from step 3}"

echo
echo "== 3. Investor deposits USDC into the vault (mints HBS shares) =="
run stellar contract invoke \
  --id "$VAULT_ID" --source "$INVESTOR" --network "$NETWORK" \
  -- deposit --from "$INVESTOR" --usdc_amount 1000_0000000

echo
echo "== 4. Admin sets impact scores so the project clears any funding thresholds =="
run stellar contract invoke \
  --id "$REGISTRY_ID" --source "$ADMIN" --network "$NETWORK" \
  -- update_impact_score \
  --project_id "$PROJECT_ID" --credit_quality 80 --green_impact 70

echo
echo "== 5. Admin funds the project from the vault =="
run stellar contract invoke \
  --id "$VAULT_ID" --source "$ADMIN" --network "$NETWORK" \
  -- fund_project --project_id "$PROJECT_ID" --amount 100_0000000

echo
echo "== 6. Whitelister (or admin) certifies the project =="
run stellar contract invoke \
  --id "$REGISTRY_ID" --source "$WHITELISTER" --network "$NETWORK" \
  -- certify_project \
  --caller "$WHITELISTER" --project_id "$PROJECT_ID" --status '{"Certified":[]}'

echo
echo "== Done. Inspect final state: =="
run stellar contract invoke \
  --id "$REGISTRY_ID" --source "$ADMIN" --network "$NETWORK" \
  -- get_project --id "$PROJECT_ID"
run stellar contract invoke \
  --id "$VAULT_ID" --source "$ADMIN" --network "$NETWORK" \
  -- get_project_investment --project_id "$PROJECT_ID"
