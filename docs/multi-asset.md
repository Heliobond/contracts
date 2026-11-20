# Multi-Asset Support

**Issue:** [#133](https://github.com/Heliobond/contracts/issues/133)

**Status: Planned, not implemented.** The current `InvestmentVault` is single-asset: it accepts one USDC SAC address fixed at construction time. Everything under "Extension path for multi-asset" below describes a design proposal — none of it exists in the contract today. This document exists so readers don't assume multi-asset support, an asset whitelist, or a price oracle are already live; see "Current state" for what actually ships now.

---

## Current state (implemented)

`InvestmentVault.__constructor` stores one `usdc_sac: Address` in instance storage (`VaultKey::UsdcSac`). All `deposit`, `withdraw`, `fund_project`, and `claim_yield` flows use this single asset.

The `accepted_asset()` view function exposes the current asset address:

```rust
pub fn accepted_asset(env: Env) -> Address
```

---

## Extension path for multi-asset (planned, not implemented)

None of the storage keys, functions, or oracle integration described in this section exist in `investment_vault/src/lib.rs` today. They describe the intended design for a future change.

### 1. Asset whitelist in instance storage

Add a `VaultKey::AcceptedAssets` key holding a `soroban_sdk::Vec<Address>`:

```rust
AcceptedAssets,   // Vec<Address> of accepted SAC addresses
```

Add admin functions:

```rust
pub fn add_accepted_asset(env: Env, asset: Address)
pub fn remove_accepted_asset(env: Env, asset: Address)
pub fn get_accepted_assets(env: Env) -> Vec<Address>
```

### 2. Per-asset accounting

Generalise `VaultKey` to be asset-aware:

```rust
TotalInvestments,               // total across all assets, denominated in a common unit
ProjectInvestmentAsset(u32, Address),  // invested per project per asset
```

### 3. Oracle price feed

To quote cross-asset positions in a single unit of account (e.g. USDC), integrate a price oracle:

```rust
mod oracle_interface {
    soroban_sdk::contractimport!(file = "../target/.../price_oracle.wasm");
}
// price_oracle.get_price(base, quote) -> i128 (scaled by 1e7)
```

### 4. Vault shares remain single-denomination

Vault shares (HBS) continue to represent a claim on the entire pool, regardless of which asset was deposited. Deposits in non-USDC assets are converted to USDC equivalent at oracle price before minting shares. Withdrawals pay out in the requested asset if the vault holds sufficient liquidity.

---

## Integration checklist (planned)

This checklist describes the intended workflow once the extension path above is implemented; `add_accepted_asset` and the other functions referenced below do not exist yet. Before adding a new asset:

- [ ] Asset must be a Stellar SAC (SEP-41 compatible).
- [ ] A price oracle feed must exist for `(asset, USDC)`.
- [ ] Run `add_accepted_asset` via the admin multisig.
- [ ] Update the frontend to display the new asset as a deposit option.
- [ ] Add integration tests covering deposit, withdraw, and `fund_project` for the new asset.

---

## Security considerations (for the planned design)

- Never accept unverified assets — malicious tokens can emit fake transfer events. Only allow whitelisted SACs.
- Oracle price manipulation risk: use time-weighted average prices (TWAP) where possible.
- Liquidity mismatch: if investors deposit in many assets but projects pay returns in USDC, the vault may be unable to pay withdrawals in the requested asset. Implement a liquidity buffer requirement per asset.
