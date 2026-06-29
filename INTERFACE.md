# Heliobond Contract Interface Specification

This document is the source of truth for client integrations, SDK generation,
and interface tests. Types use Soroban SDK names.

## Common Types

- `Address`: Soroban account or contract address.
- `String`: Soroban string.
- `Bytes`, `BytesN<32>`: Soroban byte buffers.
- `Vec<T>`: Soroban vector.
- Amounts are `i128` in 7-decimal token units unless stated otherwise.
- Scores and basis points are `u32`; 10,000 bps = 100%.

## Multi-Sig Admin Pattern

Both contracts support `set_multisig_admin(signers: Vec<Address>, threshold: u32)`,
`clear_multisig_admin()`, and `get_multisig_admin() -> (Vec<Address>, u32)`.

The owner configures the signer set. `threshold = 0` means multi-sig is disabled
and legacy owner auth is used. When enabled, critical operations must use the
documented approval-aware or batch entrypoints and include at least `threshold`
unique configured signers. Each signer in `approvals` must authorize the
invocation.

Multi-sig errors:

| Error | Meaning |
| --- | --- |
| `InvalidMultiSigThreshold` | Threshold is 0, exceeds signer count, or stored config is invalid. |
| `TooManyMultiSigSigners` | More than 10 signers were supplied. |
| `NotMultiSigSigner` | An approval address is not in the configured signer set. |
| `DuplicateApproval` | The same signer appears more than once. |
| `InsufficientApprovals` | Fewer approvals than the configured threshold were supplied. |

## ProjectRegistry

### Data Types

`ProjectData { owner: Address, uri: String, credit_quality: u32, green_impact: u32, maturity_date: u64, certification_status: CertificationStatus }`

`CertificationStatus`: `None`, `Pending`, `Certified`, `Revoked`.

`Proposal { description: String, proposer: Address, voting_ends_at: u64, votes_for: i128, votes_against: i128, executed: bool }`

### Functions

| Function | Auth | Returns | Errors / Notes |
| --- | --- | --- | --- |
| `__constructor(admin: Address, whitelister: Address)` | none | none | Sets owner, whitelister, counters. |
| `set_whitelist(account: Address, status: bool)` | `whitelister` | none | Grants or revokes project creation rights. |
| `create_project(creator: Address, uri: String, maturity_date: u64)` | `creator` | `u32` | Requires whitelist, URI length 8..512, future maturity when nonzero. |
| `get_project(id: u32)` | none | `ProjectData` | `ProjectNotFound`. |
| `total_projects()` | none | `u32` | Highest assigned project id. |
| `update_impact_score(project_id: u32, credit_quality: u32, green_impact: u32)` | owner, or disabled when multi-sig is enabled | none | Scores 0..100. Use approval variant after enabling multi-sig. |
| `update_impact_score_approved(project_id: u32, credit_quality: u32, green_impact: u32, approvals: Vec<Address>)` | multi-sig signers | none | Critical operation. |
| `update_credit_quality_score(project_id: u32, credit_quality: u32)` | owner, or disabled when multi-sig is enabled | none | Updates credit score only. |
| `update_credit_quality_approved(project_id: u32, credit_quality: u32, approvals: Vec<Address>)` | multi-sig signers | none | Critical operation. |
| `certify_project(caller: Address, project_id: u32, status: CertificationStatus)` | `caller` | none | Caller must be whitelister or owner. |
| `is_mature(project_id: u32)` | none | `bool` | False for open-ended projects. |
| `get_all_projects()` | none | `Vec<(u32, ProjectData)>` | O(n) over registered ids. |
| `create_proposal(proposer: Address, description: String, voting_duration_secs: u64)` | `proposer` | `u32` | Duration must be at least 86,400 seconds. |
| `cast_vote(voter: Address, proposal_id: u32, support: bool, weight: i128)` | `voter` | none | Weight must be positive; callers must supply verified HBS balance. |
| `execute_proposal(proposal_id: u32)` | none | `bool` | Callable after voting ends. |
| `get_proposal(proposal_id: u32)` | none | `Proposal` | `ProposalNotFound`. |
| `deposit_collateral(project_id: u32, depositor: Address, token: Address, amount: i128)` | `depositor` | none | Depositor must be project owner; amount positive. |
| `get_collateral(project_id: u32, token: Address)` | none | `i128` | Returns 0 if absent. |
| `release_collateral(project_id: u32, caller: Address, token: Address)` | `caller` | none | Caller must be project owner; project must be mature when maturity exists. |
| `liquidate_collateral(project_id: u32, token: Address, recipient: Address)` | owner, or disabled when multi-sig is enabled | none | Critical operation. |
| `liquidate_collateral_approved(project_id: u32, token: Address, recipient: Address, approvals: Vec<Address>)` | multi-sig signers | none | Critical operation. |
| `set_multisig_admin(signers: Vec<Address>, threshold: u32)` | owner | none | Configures 1..10 unique signers. |
| `clear_multisig_admin()` | owner | none | Restores owner-only critical operations. |
| `get_multisig_admin()` | none | `(Vec<Address>, u32)` | Returns signers and threshold. |
| `get_interest_rate(project_id: u32)` | none | `u32` | Annualized bps from project scores. |
| `set_creator_reputation(caller: Address, creator: Address, score: u32)` | `caller` | none | Caller must be whitelister or owner; score 0..100. |
| `get_creator_reputation(creator: Address)` | none | `u32` | Defaults to 0. |
| `get_creator_funding_limit_bps(creator: Address)` | none | `u32` | Reputation-derived suggested limit. |
| `set_whitelister(new_whitelister: Address)` | owner | none | Replaces whitelister. |
| `get_whitelister()` | none | `Address` | Current whitelister. |

## InvestmentVault

### Data Types

`PortfolioInfo { shares: i128, usdc_value: i128, claimable_yield: i128, share_of_pool_bps: i128, total_deposited: i128 }`

`HBSTokenInfo { name: String, symbol: String, decimals: u32, trading_enabled: bool }`

`QueuedClaim { from: Address, usdc_owed: i128 }`

Compliance/reporting types: `ComplianceEventData`, `ReportingSnapshotData`,
`RegulatoryReport`. Bridge type: `BridgeTransferPayload`.

### Functions

| Function | Auth | Returns | Errors / Notes |
| --- | --- | --- | --- |
| `__constructor(admin: Address, usdc_sac: Address, registry: Address)` | none | none | Validates registry via `total_projects()`, sets HBS metadata. |
| `deposit(from: Address, usdc_amount: i128)` | `from` | `i128` | Transfers USDC, deducts insurance premium and optional fee, mints shares. |
| `batch_deposit(deposits: Vec<(Address, i128)>)` | each depositor | `Vec<i128>` | Runs multiple deposits in order; keep batches small enough for Soroban resource limits. |
| `withdraw(from: Address, shares_amount: i128)` | `from` via burn | `i128` | Burns shares; may enqueue if liquid USDC is insufficient. |
| `claim()` | none | `i128` | Settles queued withdrawals FIFO. |
| `fund_project(project_id: u32, amount: i128)` | owner, or disabled when multi-sig is enabled | none | Critical operation; checks score thresholds and insurance reserve. |
| `fund_project_with_approvals(project_id: u32, amount: i128, approvals: Vec<Address>)` | multi-sig signers | none | Critical operation. |
| `batch_fund_projects(fundings: Vec<(u32, i128)>, approvals: Vec<Address>)` | owner when multi-sig disabled, otherwise multi-sig signers | none | Common batch funding path. |
| `receive_yield(from: Address, amount: i128)` | owner, or disabled when multi-sig is enabled | none | Transfers repayment USDC and updates yield accumulator. |
| `receive_yield_with_approvals(from: Address, amount: i128, approvals: Vec<Address>)` | multi-sig signers | none | Critical operation. |
| `claim_yield(from: Address)` | `from` | `i128` | Pays accrued yield when liquid. |
| `claim_insurance(project_id: u32, recipient: Address, amount: i128)` | owner, or disabled when multi-sig is enabled | none | Critical operation; one claim per project. |
| `claim_insurance_with_approvals(project_id: u32, recipient: Address, amount: i128, approvals: Vec<Address>)` | multi-sig signers | none | Critical operation. |
| `set_multisig_admin(signers: Vec<Address>, threshold: u32)` | owner | none | Configures 1..10 unique signers. |
| `clear_multisig_admin()` | owner | none | Restores owner-only critical operations. |
| `get_multisig_admin()` | none | `(Vec<Address>, u32)` | Returns signers and threshold. |
| `get_expected_returns()` | none | `i128` | O(n) over registry projects. |
| `total_assets()` | none | `i128` | Liquid USDC + investments + expected returns. |
| `convert_to_shares(usdc_amount: i128)` | none | `i128` | ERC-4626-style conversion. |
| `convert_to_assets(shares_amount: i128)` | none | `i128` | ERC-4626-style conversion. |
| `get_utilization_bps()` | none | `u32` | Investments over liquid plus investments. |
| `claimable_yield(account: Address)` | none | `i128` | View-only accrued yield. |
| `get_portfolio(account: Address)` | none | `PortfolioInfo` | Investor analytics snapshot. |
| `insurance_fund_balance()` | none | `i128` | Stored insurance reserve. |
| `accepted_asset()` | none | `Address` | USDC SAC address. |
| `set_management_fee(fee_bps: u32, recipient: Address)` | owner | none | Fee capped at 500 bps. |
| `get_management_fee_bps()` | none | `u32` | Defaults to 0. |
| `enable_secondary_trading()` | owner | none | Sets HBS trading flag. |
| `is_trading_enabled()` | none | `bool` | Trading flag. |
| `set_funding_thresholds(min_credit_quality: u32, min_green_impact: u32)` | owner | none | Scores must be 0..100. |
| `get_min_credit_quality()` | none | `u32` | Defaults to 0. |
| `get_min_green_impact()` | none | `u32` | Defaults to 0. |
| `set_registry(new_registry: Address)` | owner | none | Validates registry and replaces dependency. |
| `get_registry()` | none | `Address` | Registry address. |
| `get_hbs_token_info()` | none | `HBSTokenInfo` | HBS metadata and trading flag. |
| `set_bridge(bridge: Address)` | owner | none | Configures bridge minter. |
| `bridge_mint(to: Address, amount: i128)` | bridge | none | Amount positive. |
| `bridge_burn(from: Address, amount: i128)` | `from` | none | Amount positive. |
| `set_wormhole_core(core: Address)` | owner | none | Configures Wormhole core. |
| `set_trusted_emitter(chain_id: u32, emitter_address: BytesN<32>, trusted: bool)` | owner | none | Updates trusted emitter map. |
| `initiate_bridge_transfer(from: Address, amount: i128, target_chain: u32, recipient: BytesN<32>, nonce: u64)` | `from` | `u64` | Burns HBS and publishes Wormhole message. |
| `complete_bridge_transfer(vaa: Bytes)` | none | none | Verifies VAA, trusted emitter, replay guard, mints HBS. |
| `set_flash_loan_fee(fee_bps: i128)` | owner | none | 0..1000 bps. |
| `flash_loan_fee()` | none | `i128` | Defaults to 30 bps. |
| `execute_flash_loan(initiator: Address, borrower: Address, amount: i128, data: Bytes)` | `initiator` | none | Calls borrower callback and collects amount plus fee. |
| `set_carbon_oracle(oracle: Address)` | owner | none | Configures oracle. |
| `set_carbon_credit_price(price: i128)` | oracle | none | Price positive. |
| `carbon_credit_price()` | none | `i128` | Defaults to 0. |
| `calculate_carbon_credits(project_id: u32, amount: i128)` | none | `CarbonCreditCalculation` | Uses project green impact. |
| `issue_carbon_credits(to: Address, project_id: u32, amount: i128)` | none | `i128` | Issues calculated credits when positive. |
| `transfer_carbon_credits(from: Address, to: Address, amount: i128)` | `from` | none | Balance must cover amount. |
| `carbon_credit_balance(address: Address)` | none | `i128` | Defaults to 0. |
| `set_max_transaction_amount(amount: i128)` | owner | none | Compliance cap; 0 disables. |
| `max_transaction_amount()` | none | `i128` | Defaults to 0. |
| `record_compliance_event(event_type: String, data: String)` | owner | none | Appends event. |
| `get_compliance_event(seq: u64)` | none | `ComplianceEventData` | Panics if missing. |
| `get_compliance_events(from: u64, to: u64)` | none | `Vec<ComplianceEventData>` | Inclusive range; skips missing entries. |
| `take_reporting_snapshot()` | owner | none | Captures latest reporting metrics. |
| `get_latest_snapshot()` | none | `ReportingSnapshotData` | Panics if no snapshot exists. |
| `export_regulatory_data()` | none | `RegulatoryReport` | Includes latest snapshot and up to 50 recent events. |

## Batch Operation Limits

Batch calls execute each item sequentially and consume the sum of their storage,
auth, and token-transfer costs. Clients should start with batches of 2-10 items,
simulate before submission, and split work when simulation approaches network
resource limits.

## Example Usage

Configure 2-of-3 multi-sig and fund two projects:

```text
vault.set_multisig_admin([signer_a, signer_b, signer_c], 2)
vault.batch_fund_projects([(1, 1000000000), (2, 2500000000)], [signer_a, signer_b])
```

Batch deposits:

```text
vault.batch_deposit([(alice, 1000000000), (bob, 500000000)])
```

Update registry scores with multi-sig:

```text
registry.update_impact_score_approved(1, 80, 90, [signer_a, signer_b])
```
