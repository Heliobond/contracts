# Heliobond Contract Reference

Complete public-interface specification for both Soroban smart contracts.
All functions live in the `InvestmentVault` or `ProjectRegistry` crates.

---

## Glossary (#242)

Domain terms used throughout this document and the rest of the `contracts` docs.

| Term                                   | Definition                                                                                                                                                                                                                                                                                                   |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Credit quality score**               | Oracle-set score (0–100) on a project's `ProjectData.credit_quality`, reflecting the creditworthiness of the underlying bond. Feeds into the project's interest rate via `compute_rate`; updated with `update_impact_score` or `update_credit_quality_score`.                                                |
| **Green impact score**                 | Oracle-set score (0–100) on a project's `ProjectData.green_impact`, reflecting the environmental/climate benefit of the project. Feeds into the interest rate alongside credit quality and into `calculate_carbon_credits`; updated with `update_impact_score`.                                              |
| **HBS token**                          | "Heliobond Shares" — the SEP-41 fungible token minted by `InvestmentVault` to represent an investor's proportional claim on the pooled USDC. Minted on `deposit`, burned on `withdraw`; see [Secondary Market Trading](#secondary-market-trading-issue-126) below.                                           |
| **Whitelister**                        | The address authorised to grant or revoke project-creation rights via `set_whitelist`. A separate role from the contract owner/admin.                                                                                                                                                                        |
| **Certification status**               | `ProjectData.certification_status` (`None`, `Pending`, `Certified`, `Revoked`) — an independent attestation of a project's legitimacy, set by the whitelister or admin via `certify_project`. Distinct from the numeric credit/green scores.                                                                 |
| **Maturity date**                      | Unix timestamp on `ProjectData.maturity_date` after which a project is considered mature (`is_mature`). `0` means open-ended (never matures). `release_collateral` requires maturity when one is set; `compact_archive` does not check it directly — it requires the project to already be archived instead. |
| **Interest rate (bps)**                | The annualized rate, in basis points (10,000 bps = 100%), that `get_interest_rate` derives from a project's credit quality and green impact scores via `compute_rate`.                                                                                                                                       |
| **Insurance premium / insurance fund** | A fixed 50 bps (`INSURANCE_PREMIUM_BPS`) cut of every vault deposit that accumulates in the vault's insurance fund. Paid out via `claim_insurance` to compensate investors when a project defaults.                                                                                                          |
| **Management fee**                     | An optional, admin-configured fee (in bps, capped at `MAX_MANAGEMENT_FEE_BPS` = 500) deducted from each deposit before shares are minted, sent to a configured recipient via `set_management_fee`.                                                                                                           |
| **Yield-per-share accumulator**        | The vault's global, monotonically increasing `YieldPerShareAccum` value (scaled by `YIELD_SCALE`), used with each investor's last-claim checkpoint (`YieldDebt`) to compute claimable yield in O(1) without iterating investors.                                                                             |
| **Multi-sig admin**                    | An optional `(signers, threshold)` configuration (`set_multisig_admin`) that requires `threshold` distinct signer approvals for critical operations instead of a single owner signature. `threshold = 0` disables it.                                                                                        |
| **Collateral**                         | Tokens deposited against a specific project (`deposit_collateral`) as security, released to the owner at maturity (`release_collateral`) or seized by the admin on default (`liquidate_collateral`).                                                                                                         |
| **Archive / compaction**               | Two-step lifecycle for retiring a project's storage footprint: `archive_project` flags a project inactive, and `compact_archive` later replaces its full `ProjectData` with a much smaller `ArchiveSummary` to reduce ongoing rent.                                                                          |
| **Governance proposal**                | A time-boxed on-chain vote (`create_proposal`, `cast_vote`, `execute_proposal`) that HBS holders use to approve or reject a described action; passes if `votes_for > votes_against` once voting closes.                                                                                                      |
| **Carbon credits**                     | Units calculated from a project's green impact score and funding amount (`calculate_carbon_credits`), issuable to an address (`issue_carbon_credits`) and transferable independently of HBS or USDC balances.                                                                                                |
| **Bridge transfer**                    | Cross-chain movement of HBS value via a Wormhole-style message: `initiate_bridge_transfer` burns HBS and emits a VAA-verifiable message; `complete_bridge_transfer` verifies the VAA against trusted emitters and mints HBS on the destination side.                                                         |
| **State version / migration**          | `STATE_VERSION` is the storage schema version a given contract build supports; `stored_state_version()` is what's actually persisted on-chain. `migrate_state` upgrades storage from an older version. See [MIGRATION.md](MIGRATION.md).                                                                     |

---

## ProjectRegistry

Crate: `project_registry`
Constructor args: `admin: Address, whitelister: Address`

### Public Functions

| Function                                                        | Auth                  | Args                                                                          | Returns                   | Events                                                      |
| --------------------------------------------------------------- | --------------------- | ----------------------------------------------------------------------------- | ------------------------- | ----------------------------------------------------------- |
| `set_whitelist(account, status)`                                | whitelister           | `account: Address, status: bool`                                              | `()`                      | `WhitelistSet { account, status }`                          |
| `create_project(creator, uri, maturity_date)`                   | creator (whitelisted) | `creator: Address, uri: String, maturity_date: u64`                           | `u32` (project\_id)       | `ProjectCreated { project_id, owner }`                      |
| `get_project(id)`                                               | none                  | `id: u32`                                                                     | `ProjectData`             | —                                                           |
| `total_projects()`                                              | none                  | —                                                                             | `u32`                     | —                                                           |
| `get_all_projects()`                                            | none                  | —                                                                             | `Vec<(u32, ProjectData)>` | —                                                           |
| `update_impact_score(project_id, credit_quality, green_impact)` | admin (owner)         | `project_id: u32, credit_quality: u32, green_impact: u32`                     | `()`                      | `ProjectUpdated`, `RateUpdated`, `ScoreChanged`             |
| `update_credit_quality_score(project_id, credit_quality)`       | admin (owner)         | `project_id: u32, credit_quality: u32` (0–100)                                | `()`                      | `ScoreChanged`                                              |
| `certify_project(caller, project_id, status)`                   | whitelister or admin  | `caller: Address, project_id: u32, status: CertificationStatus`               | `()`                      | `ProjectCertified { project_id, status }`                   |
| `is_mature(project_id)`                                         | none                  | `project_id: u32`                                                             | `bool`                    | —                                                           |
| `create_proposal(proposer, description, voting_duration_secs)`  | proposer              | `proposer: Address, description: String, voting_duration_secs: u64` (≥ 86400) | `u32` (proposal\_id)      | `ProposalCreated { proposal_id, proposer, voting_ends_at }` |
| `cast_vote(voter, proposal_id, support, weight)`                | voter                 | `voter: Address, proposal_id: u32, support: bool, weight: i128`               | `()`                      | `VoteCast { proposal_id, voter, support, weight }`          |
| `execute_proposal(proposal_id)`                                 | none                  | `proposal_id: u32`                                                            | `bool` (passed)           | `ProposalExecuted { proposal_id, passed }`                  |
| `get_proposal(proposal_id)`                                     | none                  | `proposal_id: u32`                                                            | `Proposal`                | —                                                           |

### Types

```rust
pub struct ProjectData {
    pub owner: Address,
    pub uri: String,
    pub credit_quality: u32,   // 0–100, oracle-set
    pub green_impact: u32,     // 0–100, oracle-set
    pub maturity_date: u64,    // Unix timestamp; 0 = open-ended
    pub certification_status: CertificationStatus,
}

pub enum CertificationStatus { None, Certified, Revoked }

pub struct Proposal {
    pub description: String,
    pub proposer: Address,
    pub voting_ends_at: u64,
    pub votes_for: i128,
    pub votes_against: i128,
    pub executed: bool,
}
```

### Score Functions Comparison

| Function                      | Scope                                                       | Emitted Events                                  |
| ----------------------------- | ----------------------------------------------------------- | ----------------------------------------------- |
| `update_impact_score`         | Sets both `credit_quality` AND `green_impact` atomically    | `ProjectUpdated`, `RateUpdated`, `ScoreChanged` |
| `update_credit_quality_score` | Sets only `credit_quality`, leaves `green_impact` unchanged | `ScoreChanged`                                  |

The `ScoreChanged` event (#131) includes both old and new score values plus old and new interest rates, enabling off-chain notification services to calculate the exact delta without querying historical state.

---

## InvestmentVault

Crate: `investment_vault`
Constructor args: `admin: Address, usdc_sac: Address, registry: Address`
Token: HBS (Heliobond Shares) — SEP-41 fungible token via `FungibleToken` trait

### Constants

| Name                     | Value                 | Purpose                                          |
| ------------------------ | --------------------- | ------------------------------------------------ |
| `MAX_DEPOSIT`            | 1 billion USDC (7 dp) | Single-deposit ceiling                           |
| `INSURANCE_PREMIUM_BPS`  | 50                    | 0.5% of each deposit reserved for insurance fund |
| `MAX_MANAGEMENT_FEE_BPS` | 500                   | 5% hard cap on admin-set management fee          |
| `YIELD_SCALE`            | 1e18                  | Precision for yield-per-share accumulator        |

### Public Functions

#### Core Deposit / Withdraw

| Function                        | Auth              | Args                                                | Returns                | Events                                            |
| ------------------------------- | ----------------- | --------------------------------------------------- | ---------------------- | ------------------------------------------------- |
| `deposit(from, usdc_amount)`    | from              | `from: Address, usdc_amount: i128` (≤ MAX\_DEPOSIT) | `i128` (shares minted) | `Deposit { from, usdc_amount, shares_minted }`    |
| `withdraw(from, shares_amount)` | from (via `burn`) | `from: Address, shares_amount: i128`                | `i128` (USDC returned) | `Withdraw { from, shares_burned, usdc_returned }` |

**Deposit fee deduction order:**

1. `insurance_premium = usdc_amount × 50 / 10_000`
2. `management_fee = usdc_amount × fee_bps / 10_000`
3. `investable = usdc_amount − insurance_premium − management_fee`
4. `shares = convert_to_shares(investable)`

#### Project Funding

| Function                           | Auth  | Args                            | Returns | Events                                            |
| ---------------------------------- | ----- | ------------------------------- | ------- | ------------------------------------------------- |
| `fund_project(project_id, amount)` | admin | `project_id: u32, amount: i128` | `()`    | `ProjectFunded { project_id, amount, recipient }` |

The insurance reserve is subtracted from available USDC before the check, preventing the admin from accidentally funding projects with insurance money.

#### NAV Helpers

| Function                           | Auth | Args                  | Returns                   |
| ---------------------------------- | ---- | --------------------- | ------------------------- |
| `total_assets()`                   | none | —                     | `i128` (total USDC value) |
| `convert_to_shares(usdc_amount)`   | none | `usdc_amount: i128`   | `i128`                    |
| `convert_to_assets(shares_amount)` | none | `shares_amount: i128` | `i128`                    |
| `get_expected_returns()`           | none | —                     | `i128`                    |

#### Yield Distribution

| Function                      | Auth  | Args                          | Returns         | Events                           |
| ----------------------------- | ----- | ----------------------------- | --------------- | -------------------------------- |
| `receive_yield(from, amount)` | admin | `from: Address, amount: i128` | `()`            | `YieldReceived { from, amount }` |
| `claimable_yield(account)`    | none  | `account: Address`            | `i128`          | —                                |
| `claim_yield(from)`           | from  | `from: Address`               | `i128`          | `YieldClaimed { to, amount }`    |
| `get_portfolio(account)`      | none  | `account: Address`            | `PortfolioInfo` | —                                |

#### Insurance Fund

| Function                                         | Auth  | Args                                                | Returns | Events                                               |
| ------------------------------------------------ | ----- | --------------------------------------------------- | ------- | ---------------------------------------------------- |
| `insurance_fund_balance()`                       | none  | —                                                   | `i128`  | —                                                    |
| `claim_insurance(project_id, recipient, amount)` | admin | `project_id: u32, recipient: Address, amount: i128` | `()`    | `InsuranceClaimed { project_id, recipient, amount }` |

#### Management Fee (issue #7)

| Function                                 | Auth  | Args                                         | Returns | Events                                    |
| ---------------------------------------- | ----- | -------------------------------------------- | ------- | ----------------------------------------- |
| `set_management_fee(fee_bps, recipient)` | admin | `fee_bps: u32` (≤ 500), `recipient: Address` | `()`    | `ManagementFeeSet { recipient, fee_bps }` |
| `get_management_fee_bps()`               | none  | —                                            | `u32`   | —                                         |

The fee is `0` by default. Passing `fee_bps = 0` disables it. The hard cap of 500 bps (5%) is enforced on-chain and cannot be overridden.

#### Secondary Market Trading (issue #126)

HBS is a SEP-41 fungible token and is natively tradeable on the Stellar DEX. These functions surface the official listing status so UIs and aggregators can discover the trading pair.

| Function                     | Auth  | Args | Returns        | Events                             |
| ---------------------------- | ----- | ---- | -------------- | ---------------------------------- |
| `enable_secondary_trading()` | admin | —    | `()`           | `TradingEnabled { enabled: true }` |
| `is_trading_enabled()`       | none  | —    | `bool`         | —                                  |
| `get_hbs_token_info()`       | none  | —    | `HBSTokenInfo` | —                                  |

```rust
pub struct HBSTokenInfo {
    pub name: String,           // "Heliobond Shares"
    pub symbol: String,         // "HBS"
    pub decimals: u32,          // 7
    pub trading_enabled: bool,  // mirrors is_trading_enabled()
}
```

**DEX integration notes:**

- HBS contract ID (the vault address) is the SEP-41 asset identifier on Stellar
- To list on Stellar DEX, create an offer using the Stellar SDK: `ManageOfferOp` or `PathPaymentOp` using the vault contract address as the asset code
- Liquidity pools can be created via `ChangeTrustOp` against the HBS/USDC pair
- The `get_hbs_token_info()` function returns all metadata required for DEX listing discovery

#### Misc

| Function           | Auth | Args | Returns              |
| ------------------ | ---- | ---- | -------------------- |
| `accepted_asset()` | none | —    | `Address` (USDC SAC) |

### Types

```rust
pub struct PortfolioInfo {
    pub shares: i128,
    pub usdc_value: i128,
    pub claimable_yield: i128,
    pub share_of_pool_bps: i128,
    pub total_deposited: i128,
}
```

---

## Cross-Contract Flow: deposit → fund\_project → withdraw

```
Investor                  InvestmentVault               ProjectRegistry
    |                           |                              |
    |-- deposit(usdc_amount) -->|                              |
    |                           |-- (deduct insurance + fee)   |
    |                           |-- mint HBS shares to Investor|
    |                           |                              |
    |                           |                              |
Admin                          |                              |
    |-- fund_project(id, amt) ->|                              |
    |                           |-- get_project(id) ---------->|
    |                           |<-- ProjectData { owner } ----|
    |                           |-- transfer(USDC → owner)     |
    |                           |                              |
    |                           |                              |
Investor                       |                              |
    |-- withdraw(shares) ------>|                              |
    |                           |-- burn HBS shares            |
    |                           |-- transfer(USDC → Investor)  |
    |<-- USDC returned ---------|                              |
```

**Step-by-step:**

1. **Investor calls `deposit(from, usdc_amount)`**
   - Vault deducts insurance premium (50 bps) + management fee (if set)
   - Calls `convert_to_shares(investable)` — 1:1 on first deposit, proportional thereafter
   - Transfers `usdc_amount` from investor to vault contract
   - Mints `shares` of HBS to investor
   - Emits `Deposit`

2. **Admin calls `fund_project(project_id, amount)`**
   - Calls `registry.get_project(project_id)` to resolve the project owner address
   - Verifies `amount ≤ liquid_balance − insurance_reserve`
   - Transfers USDC from vault to project owner
   - Updates `TotalInvestments` and `ProjectInvestment(project_id)` ledger entries
   - Emits `ProjectFunded`

3. **Investor calls `withdraw(from, shares_amount)`**
   - Calls `convert_to_assets(shares_amount)` to determine USDC to return
   - Burns HBS shares via `Base::burn`
   - Transfers USDC from vault to investor

---

## Upgrading a Deployed Contract (#262)

Both contracts support in-place WASM upgrade (`upgrade(new_wasm_hash)`, owner-only)
plus a versioned storage-migration mechanism (`STATE_VERSION` /
`state_version()` / `stored_state_version()` / `migrate_state(from_version)`).
This section is the short operational summary; **`MIGRATION.md` is the
authoritative reference** — read it in full before performing a real upgrade,
especially its "Adding a New Storage Layout Version" and "Build Order for
Cross-Contract Dependencies" sections and the hard-prerequisite warning about
upgrade ordering between the two contracts.

**Summary of the procedure** (see `MIGRATION.md` for the full command-by-command
walkthrough):

1. Build the new WASM for both contracts and run the full test suite.
2. Upload the new WASM to the network (`stellar contract upload`) for both
   contracts.
3. Pause both contracts (`pause()`, owner-only) to prevent state changes
   during the upgrade window.
4. Invoke `upgrade(new_wasm_hash)` on **ProjectRegistry first, then
   InvestmentVault** — the vault calls into the registry's interface, so
   upgrading the vault first against an un-upgraded registry can break
   cross-contract calls with no automatic rollback.
5. If `STATE_VERSION` was incremented, call `migrate_state(from_version)` on
   each contract that changed. It panics with `UnsupportedStateVersion` if
   `from_version` doesn't match the currently stored version, which prevents
   double-migration.
6. Verify `stored_state_version()` reflects the new version on both
   contracts, then `unpause()` both.

**Rollback:** Soroban WASM upgrades are irreversible on-chain — the only
"rollback" is re-uploading and re-invoking `upgrade` with the previous WASM
hash (keep every uploaded hash recorded, see `deploy/testnet.json` and
`scripts/check_deploy_wasm_hash.py`). State written under the new version may
not be readable by the old WASM if the storage layout changed, so this is not
a true undo.

- Emits `Withdraw`
