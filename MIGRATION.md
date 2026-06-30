# Contract Upgrade and Migration Strategy

This document covers the strategy for upgrading deployed `ProjectRegistry` and
`InvestmentVault` contracts while preserving on-chain state. It addresses
issue #64.

## Overview

Both contracts implement:

- **In-place WASM upgrade** via `upgrade(new_wasm_hash)` — replaces bytecode
  while storage is preserved.
- **State schema versioning** via `STATE_VERSION` constant, `state_version()`,
  `stored_state_version()`, and `migrate_state(from_version)`.

The current schema version for both contracts is **1**. Increment this constant
whenever a storage layout change requires a migration step.

## Storage Layout Versioning

| Key | Type | Notes |
|-----|------|-------|
| `StateVersion` (instance) | `u32` | Written at construction; read by `require_current_state`. |

`require_current_state` rejects calls if the stored version does not match the
compiled `STATE_VERSION`. This prevents accidentally running new logic against
an old storage layout.

`stored_state_version` returns 0 for pre-versioned deployments (before v1 was
introduced).

## Upgrade Procedure

### Step 1 — Build the new WASM

```bash
stellar contract build
# Artifacts: target/wasm32v1-none/release/project_registry.wasm
#             target/wasm32v1-none/release/investment_vault.wasm
```

Run the full test suite before proceeding:

```bash
cargo test --all --quiet
```

### Step 2 — Upload the new WASM

```bash
REGISTRY_HASH=$(stellar contract upload \
  --wasm target/wasm32v1-none/release/project_registry.wasm \
  --source "$STELLAR_SECRET_KEY" \
  --network testnet)

VAULT_HASH=$(stellar contract upload \
  --wasm target/wasm32v1-none/release/investment_vault.wasm \
  --source "$STELLAR_SECRET_KEY" \
  --network testnet)
```

### Step 3 — Pause both contracts (recommended)

Before upgrading, pause user-facing operations to prevent state changes during
the upgrade window:

```bash
stellar contract invoke --id "$REGISTRY_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- pause

stellar contract invoke --id "$VAULT_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- pause
```

### Step 4 — Run the upgrade

Upgrade the registry first (vault depends on its interface):

```bash
stellar contract invoke --id "$REGISTRY_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- upgrade --new_wasm_hash "$REGISTRY_HASH"

stellar contract invoke --id "$VAULT_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- upgrade --new_wasm_hash "$VAULT_HASH"
```

### Step 5 — Run state migration (if STATE_VERSION changed)

If `STATE_VERSION` was incremented (e.g., from 1 to 2), run `migrate_state`
on each contract:

```bash
stellar contract invoke --id "$REGISTRY_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- migrate_state --from_version 1

stellar contract invoke --id "$VAULT_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- migrate_state --from_version 1
```

`migrate_state` panics with `UnsupportedStateVersion` if `from_version` does
not match the stored version, preventing double-migration.

### Step 6 — Verify and unpause

```bash
stellar contract invoke --id "$REGISTRY_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- stored_state_version
# Expected: 2 (or the new version)

stellar contract invoke --id "$VAULT_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- stored_state_version

stellar contract invoke --id "$REGISTRY_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- unpause

stellar contract invoke --id "$VAULT_ID" --source "$STELLAR_SECRET_KEY" \
  --network testnet -- unpause
```

## Rollback

Soroban WASM upgrades are irreversible on-chain. The only rollback option is to
re-upload and re-invoke `upgrade` with the previous WASM hash (which must have
been retained). State written by the new version may be incompatible with the
old WASM if the storage layout changed.

**Best practice:** keep all uploaded WASM hashes in `deploy/testnet.json` and
always test migration end-to-end on testnet before mainnet.

## Adding a New Storage Layout Version

When a storage layout change is required:

1. Increment `STATE_VERSION` in the affected contract (e.g., from 1 to 2).
2. Implement the migration logic inside `migrate_state`:

```rust
pub fn migrate_state(env: Env, from_version: u32) -> u32 {
    let current = read_state_version(&env);
    if current != from_version || current > STATE_VERSION {
        panic_with_error!(&env, RegistryError::UnsupportedStateVersion);
    }
    if current == 1 {
        // v1 → v2: example — populate new FooBar key from existing data
        // let old: OldType = env.storage().persistent().get(&DataKey::OldKey).unwrap();
        // env.storage().persistent().set(&DataKey::NewKey, &NewType::from(old));
        env.storage().instance().set(&DataKey::StateVersion, &2u32);
    }
    STATE_VERSION
}
```

3. Write an integration test that exercises the full v(n-1) → vn path.
4. Update this document with the new version entry.

## Version History

| Version | Contract | Description |
|---------|----------|-------------|
| 0 | Both | Pre-versioning deployments (treat as v1 state layout). |
| 1 | Both | Initial versioned deployment. No layout changes from v0. |

## Versioned Storage Patterns

- Instance storage: use for small, frequently-read values (admin, counters).
  Billed per ledger close regardless of reads.
- Persistent storage: use for per-project or per-address data. Billed only
  when entries exist; remove entries (not set-to-zero) when they become empty.
- Temporary storage: not used; would be lost after ledger expiry.

## Build Order for Cross-Contract Dependencies

The vault imports the registry ABI at build time via `contractimport!`. Always
build and upload the registry before the vault when both change in the same
release.

```bash
stellar contract build --package project-registry
stellar contract build --package investment-vault
```

> **Hard prerequisite — upgrading the vault before the registry is a
> point of no return.** If the vault WASM is deployed first (calling
> `registry.is_paused()`) while the registry still runs old WASM (without
> that method), every `fund_project` call will fail with a host-level error
> until the registry is also upgraded. There is no automatic rollback. Always
> upgrade the registry first; only proceed to vault upgrade once
> `registry.stored_state_version()` confirms the new version is live.
