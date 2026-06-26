# Storage Layout

**Issue:** [#96](https://github.com/Heliobond/contracts/issues/96)

This document enumerates every storage key used by the Heliobond contracts, the tier each key lives in, its value type, and cost / access notes.

---

## Storage tiers

| Tier | Lifetime | Rent | Typical use |
|------|----------|------|-------------|
| **Instance** | As long as the contract instance is live | Bumped automatically on every invocation | Config set once, read often |
| **Persistent** | Until TTL expires (rent must be paid) | Charged per byte per ledger | Long-lived per-entity state |
| **Temporary** | Automatic expiry after TTL | Cheapest writes | Not currently used |

See [ADR-002](../adr/002-storage-patterns.md) for the rationale behind this partitioning.

---

## `project_registry`

### Instance storage

| Key (`DataKey` variant) | Rust type | Description |
|-------------------------|-----------|-------------|
| `Whitelister` | `Address` | Address authorised to whitelist project creators and certify projects |
| `ProjectCounter` | `u32` | Auto-incrementing ID; assigned to the next project created |
| `ProposalCounter` | `u32` | Auto-incrementing governance proposal ID |

All instance keys are bumped automatically when any contract function is invoked. No explicit TTL management is required.

### Persistent storage

| Key | Rust type | Description |
|-----|-----------|-------------|
| `DataKey::Project(u32)` | `ProjectData` | Full project record keyed by ID |
| `DataKey::Whitelist(Address)` | `bool` | `true` if the address is whitelisted to create projects |
| `DataKey::Proposal(u32)` | `Proposal` | Governance proposal keyed by ID |
| `DataKey::HasVoted(u32, Address)` | `bool` | `true` if the address has voted on proposal `u32` |

#### `ProjectData` layout

```rust
pub struct ProjectData {
    pub owner: Address,                       // 32 bytes
    pub uri: String,                          // up to 512 bytes (MAX_URI_LEN)
    pub credit_quality: u32,                  // 4 bytes, range 0–100
    pub green_impact: u32,                    // 4 bytes, range 0–100
    pub maturity_date: u64,                   // 8 bytes, Unix timestamp; 0 = no maturity
    pub certification_status: CertificationStatus,  // 4 bytes (u32 repr)
}
```

Approximate encoded size per project: **~580 bytes** (worst case with 512-byte URI).

#### `Proposal` layout

```rust
pub struct Proposal {
    pub description: String,   // variable, no hard cap — keep short
    pub proposer: Address,     // 32 bytes
    pub voting_ends_at: u64,   // 8 bytes
    pub votes_for: i128,       // 16 bytes
    pub votes_against: i128,   // 16 bytes
    pub executed: bool,        // 1 byte
}
```

---

## `investment_vault`

### Instance storage

| Key (`VaultKey` variant) | Rust type | Description |
|--------------------------|-----------|-------------|
| `UsdcSac` | `Address` | USDC Stellar Asset Contract address (set at construction) |
| `Registry` | `Address` | `project_registry` contract address (set at construction) |

### Persistent storage

| Key | Rust type | Description |
|-----|-----------|-------------|
| `VaultKey::TotalInvestments` | `i128` | Cumulative USDC sent to projects (does not decrease on return) |
| `VaultKey::ProjectInvestment(u32)` | `i128` | USDC invested in a specific project |
| `VaultKey::YieldPerShareAccum` | `i128` | Global yield-per-share accumulator (scaled ×10¹⁸) |
| `VaultKey::YieldDebt(Address)` | `i128` | Per-investor yield-per-share checkpoint at last claim |
| `VaultKey::InsuranceFund` | `i128` | USDC balance of the insurance fund |
| `VaultKey::InsuranceClaimed(u32)` | `bool` | `true` once an insurance payout was made for project `u32` |
| `VaultKey::TotalDeposited(Address)` | `i128` | Lifetime USDC deposited by an investor (gross, before premium) |

---

## Storage cost estimates

Soroban charges rent based on **entry size in bytes × ledger TTL**. The following are rough estimates using the Stellar mainnet fee schedule (subject to change):

| Entry | Size (bytes) | Notes |
|-------|-------------|-------|
| `ProjectData` (max URI) | ~580 | Dominant cost per project |
| `ProjectData` (typical URI, 64 bytes) | ~132 | Typical IPFS CID |
| `Proposal` (short description) | ~100 | Depends on description length |
| `HasVoted(id, addr)` | ~40 | One per voter per proposal |
| `YieldDebt(addr)` | ~48 | One per investor who claims yield |
| `TotalDeposited(addr)` | ~48 | One per investor who deposits |
| `ProjectInvestment(id)` | ~20 | One per funded project |

Instance storage is billed as a single ledger entry for all instance keys combined; cost scales with the total byte count.

---

## Access patterns

| Operation | Keys read | Keys written |
|-----------|-----------|--------------|
| `create_project` | `Whitelist(creator)`, `ProjectCounter` | `Project(id)`, `ProjectCounter` |
| `update_impact_score` | `Project(id)` | `Project(id)` (skipped if no-op) |
| `certify_project` | `Whitelister`, owner (via `get_owner`) | `Project(id)` |
| `create_proposal` | `ProposalCounter` | `Proposal(id)`, `ProposalCounter` |
| `cast_vote` | `HasVoted(id, addr)`, `Proposal(id)` | `Proposal(id)`, `HasVoted(id, addr)` |
| `execute_proposal` | `Proposal(id)` | `Proposal(id)` |
| `deposit` | `UsdcSac`, `InsuranceFund`, `TotalDeposited(from)` | `InsuranceFund`, `TotalDeposited(from)` |
| `withdraw` | `UsdcSac`, `YieldPerShareAccum`, `YieldDebt(from)` | — |
| `fund_project` | `Registry`, `UsdcSac`, `InsuranceFund`, `ProjectInvestment(id)`, `TotalInvestments` | `ProjectInvestment(id)`, `TotalInvestments` |
| `receive_yield` | `YieldPerShareAccum` | `YieldPerShareAccum` |
| `claim_yield` | `YieldPerShareAccum`, `YieldDebt(from)`, `UsdcSac` | `YieldDebt(from)` |
| `get_portfolio` | `YieldPerShareAccum`, `YieldDebt(addr)`, `TotalDeposited(addr)` | — |
| `claim_insurance` | `InsuranceFund`, `InsuranceClaimed(id)` | `InsuranceFund`, `InsuranceClaimed(id)` |

---

## Migration notes

There is no on-chain migration mechanism. Storage schema changes require deploying a new contract and migrating state off-chain before switching frontend integrations to the new address. See [ADR-004](../adr/004-security-model.md) for the upgrade and admin model.
