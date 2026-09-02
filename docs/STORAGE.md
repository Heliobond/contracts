# Storage Layout

**Issues:** [#85](https://github.com/Heliobond/contracts/issues/85), [#82](https://github.com/Heliobond/contracts/issues/82), [#73](https://github.com/Heliobond/contracts/issues/73)

This document enumerates every storage key used by the Heliobond contracts, the tier each key lives in, its value type, and cost / access notes.

---

## Storage tiers

| Tier | Lifetime | Rent | Typical use |
|------|----------|------|-------------|
| **Instance** | As long as the contract instance is live | Bumped automatically on every invocation | Config set once, read often; global state read on almost every call |
| **Persistent** | Until TTL expires (rent must be paid) | Charged per byte per ledger | Long-lived per-entity state |
| **Temporary** | Automatic expiry after TTL | Cheapest writes | Not currently used |

See [ADR-002](../adr/002-storage-patterns.md) for the rationale behind this partitioning.

---

## Storage key prefix summary (#243)

Every storage key prefix across both contracts, its Rust value type, storage tier, and
the contract/module that owns it. This is a flat index for quick lookup; see the
per-contract sections below for field-level detail, size estimates, and access patterns.

| Prefix | Rust type | Tier | Owner |
|--------|-----------|------|-------|
| `StateVersion` | `u32` | Instance | `project_registry` |
| `Whitelister` | `Address` | Instance | `project_registry` |
| `ProjectCounter` | `u32` | Instance | `project_registry` |
| `ProposalCounter` | `u32` | Instance | `project_registry` |
| `MultiSigSigners` | `Vec<Address>` | Instance | `project_registry` |
| `MultiSigThreshold` | `u32` | Instance | `project_registry` |
| `Paused` | `bool` | Instance | `project_registry` |
| `EmergencyAdmin` | `Option<Address>` | Instance | `project_registry` |
| `Project(u32)` | `ProjectData` | Persistent | `project_registry` |
| `Whitelist(Address)` | `bool` | Persistent | `project_registry` |
| `Proposal(u32)` | `Proposal` | Persistent | `project_registry` |
| `HasVoted(u32, Address)` | `bool` | Persistent | `project_registry` |
| `Collateral(u32, Address)` | `i128` | Persistent | `project_registry` |
| `CreatorReputation(Address)` | `u32` | Persistent | `project_registry` |
| `Arch(u32)` | `ArchiveSummary` | Persistent | `project_registry` |
| `ScoreHistorySlot(u32, u32)` | `ScoreHistoryEntry` | Persistent | `project_registry` |
| `ScoreHistoryTotal(u32)` | `u32` | Persistent | `project_registry` |
| `StateVersion` | `u32` | Instance | `investment_vault` |
| `UsdcSac` | `Address` | Instance | `investment_vault` |
| `Registry` | `Address` | Instance | `investment_vault` |
| `CachedTotalAssets` | `i128` | Instance | `investment_vault` |
| `ManagementFeeBps` | `u32` | Instance | `investment_vault` |
| `ManagementFeeRecipient` | `Address` | Instance | `investment_vault` |
| `TradingEnabled` | `bool` | Instance | `investment_vault` |
| `MinCreditQuality` | `u32` | Instance | `investment_vault` |
| `MinGreenImpact` | `u32` | Instance | `investment_vault` |
| `Bridge` | `Address` | Instance | `investment_vault` |
| `FlashLoanFee` | `i128` | Instance | `investment_vault` |
| `CarbonOracle` | `Address` | Instance | `investment_vault` |
| `CarbonCreditPrice` | `i128` | Instance | `investment_vault` |
| `MaxTransactionAmount` | `i128` | Instance | `investment_vault` |
| `MultiSigSigners` | `Vec<Address>` | Instance | `investment_vault` |
| `MultiSigThreshold` | `u32` | Instance | `investment_vault` |
| `Paused` | `bool` | Instance | `investment_vault` |
| `ComplianceEventCounter` | `u64` | Instance | `investment_vault` |
| `ReportingSnapshot` | `ReportingSnapshotData` | Instance | `investment_vault` |
| `EmergencyAdmin` | `Option<Address>` | Instance | `investment_vault` |
| `CachedExpectedReturns` | `i128` | Persistent (dead — never read, see Migration notes) | `investment_vault` |
| `TotalInvestments` | `i128` | Persistent | `investment_vault` |
| `ProjectInvestment(u32)` | `i128` | Persistent | `investment_vault` |
| `YieldPerShareAccum` | `i128` | Persistent | `investment_vault` |
| `YieldDebt(Address)` | `i128` | Persistent | `investment_vault` |
| `InsuranceFund` | `i128` | Persistent | `investment_vault` |
| `InsuranceClaimed(u32)` | `bool` | Persistent | `investment_vault` |
| `TotalDeposited(Address)` | `i128` | Persistent | `investment_vault` |
| `QueueHead` | `u64` | Persistent | `investment_vault` |
| `QueueTail` | `u64` | Persistent | `investment_vault` |
| `QueueEntry(u64)` | `QueuedClaim` | Persistent | `investment_vault` |
| `CarbonCreditBalance(Address)` | `i128` | Persistent | `investment_vault` |
| `ComplianceEvent(u64)` | `ComplianceEventData` | Persistent | `investment_vault` |
| `LastDeposit(Address)` | `u32` | Persistent | `investment_vault` |
| `WormholeCore` | `Address` | Instance | `investment_vault` (`BridgeDataKey`) |
| `TrustedEmitter(u32, BytesN<32>)` | `bool` | Persistent | `investment_vault` (`BridgeDataKey`) |
| `ConsumedVaa(BytesN<32>)` | `bool` | Persistent | `investment_vault` (`BridgeDataKey`) |

`StateVersion`, `MultiSigSigners`, `MultiSigThreshold`, `Paused`, and `EmergencyAdmin` are
independent per-contract prefixes: each contract's `DataKey`/`VaultKey` enum is scoped to
its own instance storage, so identical prefix names on both rows never collide on-chain.
The last three rows come from `investment_vault`'s separate `BridgeDataKey` enum, which
lives alongside `VaultKey` for the Wormhole bridge feature.

---

## Storage key encoding and size (#82)

Soroban encodes `#[contracttype]` enum keys as XDR `SCVal`. The variant name is stored as a `Symbol`; shorter names reduce per-key byte cost.

| Key shape | XDR encoding | Approximate key bytes |
|-----------|-------------|----------------------|
| Unit variant (e.g. `StateVersion`) | `Symbol("StateVersion")` | name length + 4 overhead |
| Tuple variant (e.g. `Project(u32)`) | `Map {Symbol("Project") → u32}` | name length + 4 + 4 (u32) |
| Tuple variant with Address (e.g. `Whitelist(addr)`) | `Map {Symbol("Whitelist") → addr}` | name length + 4 + 32 (addr) |

**New keys should use short variant names** (4 characters or fewer where practical) to reduce per-entry cost. Existing names are stable after deployment — do not rename variants without a migration.

Example: `DataKey::Arch(u32)` (4 chars) vs `ArchiveSummary(u32)` (13 chars) saves 9 bytes per archive key.

---

## `project_registry`

### Instance storage (#85 — verified correct)

All configuration and counters are in instance storage. The instance TTL is bumped on every contract invocation, so no explicit TTL management is needed.

| Key (`DataKey` variant) | Rust type | Key bytes | Description |
|-------------------------|-----------|-----------|-------------|
| `StateVersion` | `u32` | ~16 | Storage schema version |
| `Whitelister` | `Address` | ~18 | Address authorised to whitelist creators |
| `ProjectCounter` | `u32` | ~18 | Auto-incrementing project ID |
| `ProposalCounter` | `u32` | ~20 | Auto-incrementing governance proposal ID |
| `MultiSigSigners` | `Vec<Address>` | ~18 | Multi-sig signer set for admin ops |
| `MultiSigThreshold` | `u32` | ~22 | Required approval count |

### Persistent storage

| Key | Rust type | Key bytes | Value bytes (approx) | Description |
|-----|-----------|-----------|---------------------|-------------|
| `DataKey::Project(u32)` | `ProjectData` | ~12 | ~172–620 | Full project record keyed by ID |
| `DataKey::Whitelist(Address)` | `bool` | ~42 | 1 | `true` if address is whitelisted |
| `DataKey::Proposal(u32)` | `Proposal` | ~13 | ~100+ | Governance proposal keyed by ID |
| `DataKey::HasVoted(u32, Address)` | `bool` | ~47 | 1 | `true` if address has voted on proposal |
| `DataKey::Collateral(u32, Address)` | `i128` | ~47 | 16 | Collateral balance for (project, token) |
| `DataKey::CreatorReputation(Address)` | `u32` | ~49 | 4 | Reputation score 0–100 for a creator |
| `DataKey::Arch(u32)` | `ArchiveSummary` | ~9 | ~52 | Compact record for compacted projects (#73) |

#### `ProjectData` layout

```rust
pub struct ProjectData {
    pub owner: Address,                          // 32 bytes
    pub uri: String,                             // up to 512 bytes (MAX_URI_LEN)
    pub credit_quality: u32,                     // 4 bytes
    pub green_impact: u32,                       // 4 bytes
    pub maturity_date: u64,                      // 8 bytes
    pub certification_status: CertificationStatus, // 4 bytes
    pub last_update_timestamp: u64,              // 8 bytes
    pub status: ProjectStatus,                   // 4 bytes
    pub created_at: u64,                         // 8 bytes
    pub metadata_hash: BytesN<32>,               // 32 bytes
}
```

Approximate encoded size: **~172 bytes** (typical 64-byte IPFS URI) to **~620 bytes** (max 512-byte URI).

#### `ArchiveSummary` layout (#73)

```rust
pub struct ArchiveSummary {
    pub owner: Address,              // 32 bytes
    pub final_credit_quality: u32,   // 4 bytes
    pub final_green_impact: u32,     // 4 bytes
    pub maturity_date: u64,          // 8 bytes
    pub certification_status: CertificationStatus, // 4 bytes
    pub metadata_hash: BytesN<32>,   // 32 bytes (preserved after compaction, #448)
}
```

Approximate encoded size: **~84 bytes** — a **~86% reduction** versus a max-URI `ProjectData`.

#### `Proposal` layout

```rust
pub struct Proposal {
    pub description: String,  // variable
    pub proposer: Address,    // 32 bytes
    pub voting_ends_at: u64,  // 8 bytes
    pub votes_for: i128,      // 16 bytes
    pub votes_against: i128,  // 16 bytes
    pub executed: bool,       // 1 byte
}
```

---

## `investment_vault`

### Instance storage (#85 — verified correct)

All configuration and global aggregate caches are in instance storage.

| Key (`VaultKey` variant) | Rust type | Key bytes | Description |
|--------------------------|-----------|-----------|-------------|
| `StateVersion` | `u32` | ~16 | Storage schema version |
| `UsdcSac` | `Address` | ~11 | USDC Stellar Asset Contract address |
| `Registry` | `Address` | ~12 | `project_registry` contract address |
| `ManagementFeeBps` | `u32` | ~20 | Optional management fee in bps (0–500) |
| `ManagementFeeRecipient` | `Address` | ~27 | Fee recipient address |
| `TradingEnabled` | `bool` | ~18 | Whether secondary market trading is active |
| `MinCreditQuality` | `u32` | ~20 | Minimum credit quality threshold for funding |
| `MinGreenImpact` | `u32` | ~17 | Minimum green impact threshold for funding |
| `Bridge` | `Address` | ~9 | Bridge contract address |
| `FlashLoanFee` | `i128` | ~16 | Flash loan fee in bps |
| `CarbonOracle` | `Address` | ~15 | Carbon credit oracle address |
| `CarbonCreditPrice` | `i128` | ~21 | Carbon credit price in USD micro-units |
| `MaxTransactionAmount` | `i128` | ~25 | Compliance transaction limit (0 = no limit) |
| `MultiSigSigners` | `Vec<Address>` | ~18 | Multi-sig signer set |
| `MultiSigThreshold` | `u32` | ~22 | Required approval count |
| `Paused` | `bool` | ~9 | Circuit-breaker pause state |
| `ComplianceEventCounter` | `u64` | ~27 | Compliance event sequence counter |
| `ReportingSnapshot` | `ReportingSnapshotData` | ~22 | Latest regulatory snapshot |
| `CachedTotalAssets` | `i128` | ~21 | NAV cache — updated on deposit/withdraw/yield (#85) |

`CachedTotalAssets` was moved from persistent to instance storage (#85): it is written on almost every state-changing operation and read on every asset query, so instance storage eliminates separate persistent reads and removes its individual rent obligation.

`CachedExpectedReturns` initialisation was removed from the constructor (#85): the key was written once and never subsequently read, making it dead persistent storage.

### Persistent storage

Writes go through `storage::set_persistent`, which calls `extend_ttl` so rent is
refreshed on every write. See [TTL policy](#persistent-ttl-policy-317) below.

| Key | Rust type | Key bytes | Value bytes | Description |
|-----|-----------|-----------|-------------|-------------|
| `VaultKey::TotalInvestments` | `i128` | ~21 | 16 | Cumulative USDC sent to projects |
| `VaultKey::ProjectInvestment(u32)` | `i128` | ~25 | 16 | USDC invested in a specific project |
| `VaultKey::YieldPerShareAccum` | `i128` | ~24 | 16 | Global yield-per-share accumulator (×10¹⁸) |
| `VaultKey::YieldDebt(Address)` | `i128` | ~42 | 16 | Per-investor yield checkpoint at last claim |
| `VaultKey::InsuranceFund` | `i128` | ~17 | 16 | Insurance fund USDC balance |
| `VaultKey::InsuranceClaimed(u32)` | `bool` | ~23 | 1 | `true` once insurance payout made for project |
| `VaultKey::TotalDeposited(Address)` | `i128` | ~46 | 16 | Lifetime USDC deposited by an investor |
| `VaultKey::QueueHead` | `u64` | ~14 | 8 | Oldest unprocessed redemption queue entry |
| `VaultKey::QueueTail` | `u64` | ~14 | 8 | Next free redemption queue index |
| `VaultKey::QueueEntry(u64)` | `QueuedClaim` | ~15 | ~48 | A queued redemption by index |
| `VaultKey::CarbonCreditBalance(Address)` | `i128` | ~30 | 16 | Carbon credit balance per address |
| `VaultKey::ComplianceEvent(u64)` | `ComplianceEventData` | ~22 | ~100+ | A compliance event record |
| `VaultKey::LastDeposit(Address)` | `u64` | ~24 | 8 | Timestamp of the investor's last deposit (#33) |
| `VaultKey::InvestmentTimestamp(u32)` | `u64` | ~28 | 8 | Ledger timestamp of first funding for a project (#34) |
| `BridgeDataKey::TrustedEmitter(u32, BytesN<32>)` | `bool` | ~50 | 1 | Trusted Wormhole emitter flag |
| `BridgeDataKey::ConsumedVaa(BytesN<32>)` | `bool` | ~40 | 1 | Replay-guard flag for a consumed VAA |

### Persistent TTL policy (#317)

Soroban does **not** auto-extend persistent TTL on read or write beyond the
network's default minimum. A write that only calls `.set()` leaves the entry at
that minimum; a vault left idle for weeks can archive yield, queue, insurance,
and carbon-credit state, and the next access then requires an explicit restore.

Every persistent write in `investment_vault` therefore goes through
`storage::set_persistent`, which sets the value and then calls `extend_ttl`
with the policy below. Instance storage is unchanged — it is bumped
automatically on any contract invocation.

| Constant | Value (ledgers) | Wall-clock at 5 s/ledger | Meaning |
|----------|-----------------|--------------------------|---------|
| `TTL_EXTEND_THRESHOLD_LEDGERS` | 17 280 | ~1 day | Remaining TTL below which a write re-extends |
| `TTL_EXTEND_TO_LEDGERS` | 518 400 | ~30 days | Target live window after each write |

| Key | Why it must stay live | Refresh trigger |
|-----|----------------------|-----------------|
| `YieldPerShareAccum` | Global yield accounting; archival would strand unclaimed yield | `receive_yield` |
| `YieldDebt(Address)` | Per-investor claim checkpoint | `claim_yield` |
| `ProjectInvestment(u32)` | Per-project deployed capital | `fund_project` |
| `InsuranceFund` | Premium reserve for default claims | `deposit`, `claim_insurance` |
| `InsuranceClaimed(u32)` | Prevents double-paying a default | `claim_insurance` |
| `QueueEntry(u64)` / `QueueHead` / `QueueTail` | FIFO redemption claims | `withdraw` (enqueue), `claim` (dequeue) |
| `ComplianceEvent(u64)` | Audit trail | `record_compliance_event` |
| `CarbonCreditBalance(Address)` | Issued credit balances | `issue_carbon_credits`, `transfer_carbon_credits` |
| `LastDeposit(Address)` | Withdrawal lock origin | `deposit`, `bridge_mint`, share `transfer` |
| `TotalInvestments` / `TotalDeposited(Address)` / `InvestmentTimestamp(u32)` | Portfolio / returns accounting | `fund_project` / `deposit` |
| `TrustedEmitter` / `ConsumedVaa` | Bridge trust + replay guard | `set_trusted_emitter` / `complete_bridge_transfer` |

Reads do **not** extend TTL. An operator whose vault is idle for approaching
30 days should invoke a state-changing entrypoint that rewrites the relevant
keys so rent is paid before archival.

---

## Data archival policy (#73)

As projects accumulate, full `ProjectData` entries (~132–580 bytes each) remain in persistent storage indefinitely, incurring ongoing rent even for projects that are long completed.

### Archival lifecycle

```
create_project() → ProjectData (active)
    ↓
archive_project() → ProjectData (archived flag = true)
    ↓  [optional: admin calls compact_archive after data is no longer needed]
compact_archive() → ArchiveSummary (~52 bytes) + ProjectData removed
```

### Compaction criteria

Call `compact_archive(project_id)` when ALL of the following hold:

1. The project has been archived (`archive_project` called).
2. The project has reached maturity (`is_mature()` returns true, or `maturity_date == 0` for open-ended projects with no further claims).
3. Any collateral positions for the project have been released or liquidated.
4. Off-chain indexers have already captured the full `ProjectData` for historical records.

### What compaction removes

- The full `ProjectData` including the URI string (the dominant byte cost).

### What compaction retains on-chain

- `ArchiveSummary`: owner address, final scores, maturity date, and certification status.
- `DataKey::Collateral(id, token)` entries are **not** automatically removed — release or liquidate collateral before compacting.

### Historical queries after compaction

- `get_project(id)` → panics `ProjectNotFound`.
- `get_archive_summary(id)` → returns the compact `ArchiveSummary`.
- Off-chain indexers should capture full `ProjectData` from events before compaction.

---

## Storage cost estimates (#82)

Soroban charges rent based on **entry size in bytes × ledger TTL**. The following are rough estimates.

| Entry | Key bytes | Value bytes | Total | Notes |
|-------|-----------|-------------|-------|-------|
| `ProjectData` (max URI) | ~12 | ~620 | ~632 | Dominant cost per project |
| `ProjectData` (64-byte IPFS CID) | ~12 | ~172 | ~184 | Typical cost |
| `ArchiveSummary` | ~9 | ~84 | ~93 | After `compact_archive` |
| `Proposal` (short description) | ~13 | ~100 | ~113 | Depends on description length |
| `HasVoted(id, addr)` | ~47 | 1 | ~48 | One per voter per proposal |
| `YieldDebt(addr)` | ~42 | 16 | ~58 | One per investor who claims yield |
| `TotalDeposited(addr)` | ~46 | 16 | ~62 | One per depositing investor |
| `ProjectInvestment(id)` | ~25 | 16 | ~41 | One per funded project |

Instance storage is billed as a single ledger entry for all instance keys combined; total instance size for the vault is approximately 400–600 bytes (configuration only, no per-entity data).

---

## Cross-contract call costs (#87)

Cross-contract calls are the most expensive single operation in Soroban. Each call costs several thousand instructions beyond the callee's own work.

| Operation | Cross-contract calls | Notes |
|-----------|---------------------|-------|
| `fund_project` | 1 (`get_project`) | Previously 2 — `total_projects()` removed (#87) |
| `get_expected_returns` | 1 + N (`total_projects` + per-funded project `get_project`) | N = number of funded projects |
| `calculate_carbon_credits` | 1 (`get_project`) | Cannot be reduced further |

`fund_project_internal` was optimised to make a single cross-contract call (`get_project`) instead of two (`total_projects` + `get_project`). The project-not-found case is handled by `get_project`'s own error path. A local check (`project_id == 0`) rejects the invalid zero ID without a cross-contract call.

---

## Access patterns

| Operation | Keys read | Keys written |
|-----------|-----------|--------------|
| `create_project` | `StateVersion`, `Whitelist(creator)`, `ProjectCounter` | `Project(id)`, `ProjectCounter` |
| `archive_project` | `Project(id)` | `Project(id)` |
| `compact_archive` | `Project(id)` | `Arch(id)` — removes `Project(id)` |
| `get_archive_summary` | `Arch(id)` | — |
| `update_impact_score` | `Project(id)` | `Project(id)` (skipped if no-op) |
| `certify_project` | `Whitelister`, owner (via `get_owner`) | `Project(id)` |
| `create_proposal` | `ProposalCounter` | `Proposal(id)`, `ProposalCounter` |
| `cast_vote` | `HasVoted(id, addr)`, `Proposal(id)` | `Proposal(id)`, `HasVoted(id, addr)` |
| `execute_proposal` | `Proposal(id)` | `Proposal(id)` |
| `deposit` | `UsdcSac`, `InsuranceFund`, `TotalDeposited(from)`, `CachedTotalAssets` | `InsuranceFund`, `TotalDeposited(from)`, `CachedTotalAssets` |
| `withdraw` | `UsdcSac`, `CachedTotalAssets`; queued-payout branch (insufficient liquidity) also reads `QueueTail` | `CachedTotalAssets`; queued-payout branch also writes `QueueTail` and `QueueEntry(u64)` |
| `fund_project` | `Registry`, `UsdcSac`, `InsuranceFund`, `ProjectInvestment(id)`, `TotalInvestments` + 1 cross-contract `get_project` | `ProjectInvestment(id)`, `TotalInvestments` |
| `receive_yield` | `YieldPerShareAccum` | `YieldPerShareAccum` |
| `claim_yield` | `YieldPerShareAccum`, `YieldDebt(from)`, `UsdcSac`, `CachedTotalAssets` | `YieldDebt(from)`, `CachedTotalAssets` |
| `get_portfolio` | `YieldPerShareAccum`, `YieldDebt(addr)`, `TotalDeposited(addr)` | — |
| `claim_insurance` | `InsuranceFund`, `InsuranceClaimed(id)` | `InsuranceFund`, `InsuranceClaimed(id)` |
| `deposit_collateral` | `Project(id)`, `Collateral(id, token)` | `Collateral(id, token)` |
| `release_collateral` | `Project(id)`, `Collateral(id, token)` | removes `Collateral(id, token)` |
| `liquidate_collateral` / `liquidate_collateral_approved` | `Project(id)`, `Collateral(id, token)` | removes `Collateral(id, token)` |
| `set_creator_reputation` | `Whitelister` | `CreatorReputation(creator)` |
| `batch_fund_projects` | Same keys as `fund_project`, once per funding entry (+ 1 cross-contract `get_project` per entry); also reads/writes nothing extra for the duplicate-ID check, which is held in a local `Vec` for the call's duration | Same keys as `fund_project`, once per funding entry |
| `bridge_mint` | `Bridge` | `LastDeposit(to)` (via `lock_deposit`) |
| `bridge_burn` | — | — (burns HBS shares via the token base's own storage, outside `VaultKey`) |
| `initiate_bridge_transfer` | `WormholeCore` | — (burns HBS shares) + 1 cross-contract `publish_message` to the Wormhole Core contract |
| `complete_bridge_transfer` | `WormholeCore`, `TrustedEmitter(chain_id, emitter)`, `ConsumedVaa(digest)` | `ConsumedVaa(digest)`, `LastDeposit(to)` (via `lock_deposit`) + 1 cross-contract `verify_vaa` to the Wormhole Core contract |
| `execute_flash_loan` | `FlashLoanFeeBps` (via `flash_loan_fee`) | — (mints then burns HBS shares transiently via the token base's own storage) + 1 cross-contract callback to the borrower |
| `set_carbon_oracle` | `CarbonOracle` | `CarbonOracle` (skipped if no-op) |
| `set_carbon_credit_price` | `CarbonOracle`, `CarbonCreditPrice` | `CarbonCreditPrice` (skipped if no-op) |
| `calculate_carbon_credits` | `Registry` + 1 cross-contract `get_project` | — (pure calculation, no storage written) |
| `issue_carbon_credits` | `Registry` (via `calculate_carbon_credits`) + 1 cross-contract `get_project`, `CarbonCreditBalance(to)` | `CarbonCreditBalance(to)` |
| `transfer_carbon_credits` | `CarbonCreditBalance(from)`, `CarbonCreditBalance(to)` | `CarbonCreditBalance(from)`, `CarbonCreditBalance(to)` |
| `set_max_transaction_amount` | `MaxTransactionAmount` | `MaxTransactionAmount` (skipped if no-op) |
| `record_compliance_event` | `ComplianceEventCounter` | `ComplianceEvent(seq)`, `ComplianceEventCounter`; also removes the oldest `ComplianceEvent(prune)` once the count exceeds `MAX_COMPLIANCE_EVENTS` |
| `take_reporting_snapshot` | `TotalInvestments` | `ReportingSnapshot` |
| `compact_storage` | `Registry` + 1 cross-contract `total_projects`, then `ProjectInvestment(id)` for each project ID | removes `ProjectInvestment(id)` for each zero-value entry found |
| `set_multisig_admin` | — | `MultiSigSigners`, `MultiSigThreshold` |

---

## Migration notes

Both contracts store `StateVersion = 1` in instance storage during construction.

`CachedTotalAssets` moved from persistent to instance storage (#85). Existing deployments on version 1 that have `CachedTotalAssets` in persistent storage should call `migrate_state` after upgrading; the migration function can be extended to copy the persistent value to instance and remove the persistent entry.

Future schema changes should increment `STATE_VERSION`, keep old variants stable, and extend `migrate_state` with deterministic per-version upgrade steps. See [ADR-004](../adr/004-security-model.md) for the upgrade and admin model.
