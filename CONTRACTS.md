# Heliobond Contract Reference

Complete public-interface specification for both Soroban smart contracts.
All functions live in the `InvestmentVault` or `ProjectRegistry` crates.

---

## ProjectRegistry

Crate: `project_registry`
Constructor args: `admin: Address, whitelister: Address`

### Public Functions

| Function | Auth | Args | Returns | Events |
|---|---|---|---|---|
| `set_whitelist(account, status)` | whitelister | `account: Address, status: bool` | `()` | `WhitelistSet { account, status }` |
| `create_project(creator, uri, maturity_date)` | creator (whitelisted) | `creator: Address, uri: String, maturity_date: u64` | `u32` (project\_id) | `ProjectCreated { project_id, owner, uri }` |
| `get_project(id)` | none | `id: u32` | `ProjectData` | — |
| `total_projects()` | none | — | `u32` | — |
| `get_all_projects()` | none | — | `Vec<(u32, ProjectData)>` | — |
| `update_impact_score(project_id, credit_quality, green_impact)` | admin (owner) | `project_id: u32, credit_quality: u32, green_impact: u32` | `()` | `ProjectUpdated`, `RateUpdated`, `ScoreChanged` |
| `update_credit_quality_score(project_id, credit_quality)` | admin (owner) | `project_id: u32, credit_quality: u32` (0–100) | `()` | `CreditQualityUpdated`, `ScoreChanged` |
| `certify_project(caller, project_id, status)` | whitelister or admin | `caller: Address, project_id: u32, status: CertificationStatus` | `()` | `ProjectCertified { project_id, status }` |
| `is_mature(project_id)` | none | `project_id: u32` | `bool` | — |
| `create_proposal(proposer, description, voting_duration_secs)` | proposer | `proposer: Address, description: String, voting_duration_secs: u64` (≥ 86400) | `u32` (proposal\_id) | `ProposalCreated { proposal_id, proposer, voting_ends_at }` |
| `cast_vote(voter, proposal_id, support, weight)` | voter | `voter: Address, proposal_id: u32, support: bool, weight: i128` | `()` | `VoteCast { proposal_id, voter, support, weight }` |
| `execute_proposal(proposal_id)` | none | `proposal_id: u32` | `bool` (passed) | `ProposalExecuted { proposal_id, passed }` |
| `get_proposal(proposal_id)` | none | `proposal_id: u32` | `Proposal` | — |

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

| Function | Scope | Emitted Events |
|---|---|---|
| `update_impact_score` | Sets both `credit_quality` AND `green_impact` atomically | `ProjectUpdated`, `RateUpdated`, `ScoreChanged` |
| `update_credit_quality_score` | Sets only `credit_quality`, leaves `green_impact` unchanged | `CreditQualityUpdated`, `ScoreChanged` |

The `ScoreChanged` event (#131) includes both old and new score values plus old and new interest rates, enabling off-chain notification services to calculate the exact delta without querying historical state.

---

## InvestmentVault

Crate: `investment_vault`
Constructor args: `admin: Address, usdc_sac: Address, registry: Address`
Token: HBS (Heliobond Shares) — SEP-41 fungible token via `FungibleToken` trait

### Constants

| Name | Value | Purpose |
|---|---|---|
| `MAX_DEPOSIT` | 1 billion USDC (7 dp) | Single-deposit ceiling |
| `INSURANCE_PREMIUM_BPS` | 50 | 0.5% of each deposit reserved for insurance fund |
| `MAX_MANAGEMENT_FEE_BPS` | 500 | 5% hard cap on admin-set management fee |
| `YIELD_SCALE` | 1e18 | Precision for yield-per-share accumulator |

### Public Functions

#### Core Deposit / Withdraw

| Function | Auth | Args | Returns | Events |
|---|---|---|---|---|
| `deposit(from, usdc_amount)` | from | `from: Address, usdc_amount: i128` (≤ MAX\_DEPOSIT) | `i128` (shares minted) | `Deposit { from, usdc_amount, shares_minted }` |
| `withdraw(from, shares_amount)` | from (via `burn`) | `from: Address, shares_amount: i128` | `i128` (USDC returned) | `Withdraw { from, shares_burned, usdc_returned }` |

**Deposit fee deduction order:**
1. `insurance_premium = usdc_amount × 50 / 10_000`
2. `management_fee = usdc_amount × fee_bps / 10_000`
3. `investable = usdc_amount − insurance_premium − management_fee`
4. `shares = convert_to_shares(investable)`

#### Project Funding

| Function | Auth | Args | Returns | Events |
|---|---|---|---|---|
| `fund_project(project_id, amount)` | admin | `project_id: u32, amount: i128` | `()` | `ProjectFunded { project_id, amount, recipient }` |

The insurance reserve is subtracted from available USDC before the check, preventing the admin from accidentally funding projects with insurance money.

#### NAV Helpers

| Function | Auth | Args | Returns |
|---|---|---|---|
| `total_assets()` | none | — | `i128` (total USDC value) |
| `convert_to_shares(usdc_amount)` | none | `usdc_amount: i128` | `i128` |
| `convert_to_assets(shares_amount)` | none | `shares_amount: i128` | `i128` |
| `get_expected_returns()` | none | — | `i128` |

#### Yield Distribution

| Function | Auth | Args | Returns | Events |
|---|---|---|---|---|
| `receive_yield(from, amount)` | admin | `from: Address, amount: i128` | `()` | `YieldReceived { from, amount }` |
| `claimable_yield(account)` | none | `account: Address` | `i128` | — |
| `claim_yield(from)` | from | `from: Address` | `i128` | `YieldClaimed { to, amount }` |
| `get_portfolio(account)` | none | `account: Address` | `PortfolioInfo` | — |

#### Insurance Fund

| Function | Auth | Args | Returns | Events |
|---|---|---|---|---|
| `insurance_fund_balance()` | none | — | `i128` | — |
| `claim_insurance(project_id, recipient, amount)` | admin | `project_id: u32, recipient: Address, amount: i128` | `()` | `InsuranceClaimed { project_id, recipient, amount }` |

#### Management Fee (issue #7)

| Function | Auth | Args | Returns | Events |
|---|---|---|---|---|
| `set_management_fee(fee_bps, recipient)` | admin | `fee_bps: u32` (≤ 500), `recipient: Address` | `()` | `ManagementFeeSet { recipient, fee_bps }` |
| `get_management_fee_bps()` | none | — | `u32` | — |

The fee is `0` by default. Passing `fee_bps = 0` disables it. The hard cap of 500 bps (5%) is enforced on-chain and cannot be overridden.

#### Secondary Market Trading (issue #126)

HBS is a SEP-41 fungible token and is natively tradeable on the Stellar DEX. These functions surface the official listing status so UIs and aggregators can discover the trading pair.

| Function | Auth | Args | Returns | Events |
|---|---|---|---|---|
| `enable_secondary_trading()` | admin | — | `()` | `TradingEnabled { enabled: true }` |
| `is_trading_enabled()` | none | — | `bool` | — |
| `get_hbs_token_info()` | none | — | `HBSTokenInfo` | — |

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

| Function | Auth | Args | Returns |
|---|---|---|---|
| `accepted_asset()` | none | — | `Address` (USDC SAC) |

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
   - Emits `Withdraw`
