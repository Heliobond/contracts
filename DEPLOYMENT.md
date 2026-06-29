# Deployment Guide

This guide covers deploying the Heliobond Soroban contracts to Stellar testnet or mainnet.

## Prerequisites

- Rust stable with the Soroban target:

```bash
rustup target add wasm32v1-none
```

- Stellar CLI `26.1.0`, matching CI:

```bash
curl -sSL https://github.com/stellar/stellar-cli/releases/download/v26.1.0/stellar-cli-26.1.0-x86_64-unknown-linux-gnu.tar.gz | tar -xz
sudo mv stellar /usr/local/bin/stellar
stellar --version
```

- A funded deployer account for the target network.
- An admin address that will own both contracts.
- A whitelister address for `ProjectRegistry`.
- The USDC Stellar Asset Contract address for the target network.

## Environment

Set these values before local deployment:

```bash
export STELLAR_SECRET_KEY=S...
export ADMIN_ADDRESS=G...
export WHITELISTER_ADDRESS=G...
export USDC_SAC_ADDRESS=C...
```

`STELLAR_SECRET_KEY` signs the deployment transaction. `ADMIN_ADDRESS` becomes the Ownable owner. `WHITELISTER_ADDRESS` can call `set_whitelist`. `USDC_SAC_ADDRESS` is wired into `InvestmentVault`.

## Build And Test

Run the same pre-deploy checks used by CI:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
stellar contract build
cargo test --all --quiet
cargo test --all --quiet cost_estimate -- --nocapture 2>&1 | tee gas-profile.txt
python3 scripts/check_gas_budgets.py gas-budgets.json gas-profile.txt gas-report.md
```

The build produces:

```text
target/wasm32v1-none/release/project_registry.wasm
target/wasm32v1-none/release/investment_vault.wasm
```

## Local Testnet Deployment

Use the Makefile for the standard testnet flow:

```bash
make deploy-testnet
```

The command deploys `ProjectRegistry` first, then deploys `InvestmentVault` with the registry contract ID. It writes the deployed IDs to:

```text
deploy/testnet.json
```

## Manual Deployment

For testnet:

```bash
REGISTRY_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/project_registry.wasm \
  --source "$STELLAR_SECRET_KEY" \
  --network testnet \
  -- \
  --admin "$ADMIN_ADDRESS" \
  --whitelister "$WHITELISTER_ADDRESS")

VAULT_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/investment_vault.wasm \
  --source "$STELLAR_SECRET_KEY" \
  --network testnet \
  -- \
  --admin "$ADMIN_ADDRESS" \
  --usdc_sac "$USDC_SAC_ADDRESS" \
  --registry "$REGISTRY_ID")
```

For mainnet, replace `--network testnet` with `--network mainnet` after confirming the deployer is funded and all addresses are mainnet addresses.

## GitHub Actions Deployment

Use the `Deploy` workflow from GitHub Actions. Required inputs:

- `network`: `testnet` or `mainnet`
- `admin_address`
- `whitelister_address`
- `usdc_sac_address`

Required secret:

- `STELLAR_SECRET_KEY`

The workflow builds, tests, checks WASM size budgets, deploys both contracts, invokes read-only verification calls, and compares on-chain WASM hashes with the local artifacts.

## Verification

Confirm both contracts respond:

```bash
stellar contract invoke \
  --id "$REGISTRY_ID" \
  --source "$STELLAR_SECRET_KEY" \
  --network testnet \
  -- total_projects

stellar contract invoke \
  --id "$VAULT_ID" \
  --source "$STELLAR_SECRET_KEY" \
  --network testnet \
  -- total_assets
```

Confirm state versioning:

```bash
stellar contract invoke --id "$REGISTRY_ID" --source "$STELLAR_SECRET_KEY" --network testnet -- stored_state_version
stellar contract invoke --id "$VAULT_ID" --source "$STELLAR_SECRET_KEY" --network testnet -- stored_state_version
```

Both calls should return `1`.

## Troubleshooting

- `target wasm32v1-none not found`: run `rustup target add wasm32v1-none`.
- `contract invocation failed during vault deploy`: confirm `REGISTRY_ID` points to a deployed `ProjectRegistry`; the vault constructor validates it by calling `total_projects`.
- `insufficient balance`: fund the deployer account on the selected network.
- `invalid address`: ensure contract IDs start with `C` and account addresses start with `G`.
- `gas budget exceeded`: open `gas-report.md`, compare the changed function to `gas-budgets.json`, and either optimize the code or intentionally raise the budget with justification.
- `stored_state_version` returns `0` after an upgrade: call `migrate_state --from_version 0` as the contract owner, then re-run verification.
