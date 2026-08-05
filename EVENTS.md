# Heliobond Events Catalog

This document provides a catalog of all events emitted by the Heliobond smart contracts for off-chain indexers and developers.

Each event lists the public [`INTERFACE.md`](INTERFACE.md) function(s) that emit it, so indexer authors can trace an event back to the call that produced it.

## Project Registry Events

### `project_created`

- **Topics**: `["project", "created"]`
- **Data**: `(project_id: u32, creator: Address)`
- **Description**: Emitted when a new project is created in the registry.
- **Emitted by**: [`create_project`](INTERFACE.md#projectregistry)

### `score_changed`

- **Topics**: `["score_changed", project_id: u32]`
- **Data** (Map, keyed by field name): `{old_credit_quality: u32, new_credit_quality: u32, old_green_impact: u32, new_green_impact: u32, old_rate_bps: u32, new_rate_bps: u32}`
- **Description**: Emitted when a project's impact scores and corresponding interest rate are updated.
- **Emitted by**: [`update_impact_score`](INTERFACE.md#projectregistry) / [`update_impact_score_approved`](INTERFACE.md#projectregistry) (both scores), [`update_credit_quality_score`](INTERFACE.md#projectregistry) (credit quality only)

### `project_updated`

- **Topics**: `["project", "updated"]`
- **Data**: `(project_id: u32, credit_quality: u32, green_impact: u32)`
- **Description**: Emitted when the oracle updates a project's credit-quality / green-impact scores (#6).
- **Emitted by**: [`update_impact_score`](INTERFACE.md#projectregistry)

### `rate_updated`

- **Topics**: `["project", "rate_updated"]`
- **Data**: `(project_id: u32, rate_bps: u32)`
- **Description**: Emitted when a project's interest rate is recalculated (#129).
- **Emitted by**: [`recalculate_rate`](INTERFACE.md#projectregistry)

### `project_archived`

- **Topics**: `["project", "archived"]`
- **Data**: `(project_id: u32)`
- **Description**: Emitted when a project is archived.
- **Emitted by**: [`archive_project`](INTERFACE.md#projectregistry)

### `project_deleted`

- **Topics**: `["project", "deleted"]`
- **Data**: `(project_id: u32)`
- **Description**: Emitted when a project is completely deleted.
- **Emitted by**: [`delete_project`](INTERFACE.md#projectregistry)

### `project_compacted`

- **Topics**: `["project", "compacted"]`
- **Data**: `(project_id: u32)`
- **Description**: Emitted when a project's storage footprint is reduced.
- **Emitted by**: [`compact_archive`](INTERFACE.md#projectregistry)

### `collateral_deposited`

- **Topics**: `["project", "collateral_deposited"]`
- **Data**: `(project_id: u32, token: Address, depositor: Address, amount: i128)`
- **Description**: Emitted when collateral is added for a project.
- **Emitted by**: [`deposit_collateral`](INTERFACE.md#projectregistry)

### `collateral_released`

- **Topics**: `["project", "collateral_released"]`
- **Data**: `(project_id: u32, token: Address, receiver: Address, amount: i128)`
- **Description**: Emitted when collateral is returned to the project owner.
- **Emitted by**: [`release_collateral`](INTERFACE.md#projectregistry)

### `collateral_liquidated`

- **Topics**: `["project", "collateral_liquidated"]`
- **Data**: `(project_id: u32, token: Address, recipient: Address, amount: i128)`
- **Description**: Emitted when collateral is liquidated by the admin (#128).
- **Emitted by**: [`liquidate_collateral`](INTERFACE.md#projectregistry)

### `whitelist_set`

- **Topics**: `["project", "whitelist_set"]`
- **Data**: `(account: Address, status: bool)`
- **Description**: Emitted when an account's whitelist status is changed.
- **Emitted by**: [`set_whitelist`](INTERFACE.md#projectregistry)

### `project_certified`

- **Topics**: `["project", "certified"]`
- **Data**: `(project_id: u32, status: CertificationStatus)`
- **Description**: Emitted when a project's certification status is updated (#130).
- **Emitted by**: [`certify_project`](INTERFACE.md#projectregistry)

### `proposal_created`

- **Topics**: `["governance", "proposal_created"]`
- **Data**: `(proposal_id: u32, proposer: Address, voting_ends_at: u64)`
- **Description**: Emitted when a governance proposal is created (#134).
- **Emitted by**: [`create_proposal`](INTERFACE.md#projectregistry)

### `vote_cast`

- **Topics**: `["governance", "vote_cast"]`
- **Data**: `(proposal_id: u32, voter: Address, support: bool, weight: i128)`
- **Description**: Emitted when a vote is cast on a proposal (#134).
- **Emitted by**: [`cast_vote`](INTERFACE.md#projectregistry)

### `proposal_executed`

- **Topics**: `["governance", "proposal_executed"]`
- **Data**: `(proposal_id: u32, passed: bool)`
- **Description**: Emitted when a proposal is finalised (#134).
- **Emitted by**: [`execute_proposal`](INTERFACE.md#projectregistry)

## Investment Vault Events

### `deposit`

- **Topics**: `["vault", "deposit"]`
- **Data**: `(from: Address, usdc_amount: i128, shares_minted: i128)`
- **Description**: Emitted when an investor deposits USDC and receives vault shares.
- **Emitted by**: [`deposit`](INTERFACE.md#investmentvault)

### `withdraw`

- **Topics**: `["vault", "withdraw"]`
- **Data**: `(from: Address, shares_burned: i128, usdc_returned: i128)`
- **Description**: Emitted when an investor burns shares and withdraws USDC.
- **Emitted by**: [`withdraw`](INTERFACE.md#investmentvault)

### `withdraw_queued`

- **Topics**: `["vault", "withdraw_queued"]`
- **Data**: `(from: Address, shares_burned: i128, usdc_owed: i128)`
- **Description**: Emitted when a withdrawal is queued because liquid USDC is insufficient (#3). Shares are burned immediately; USDC will be paid when claim() is called.
- **Emitted by**: [`queue_withdrawal`](INTERFACE.md#investmentvault)

### `withdraw_claimed`

- **Topics**: `["vault", "withdraw_claimed"]`
- **Data**: `(to: Address, usdc_paid: i128, claim_index: u64)`
- **Description**: Emitted when a queued redemption claim is settled by claim() (#3).
- **Emitted by**: [`claim`](INTERFACE.md#investmentvault)

### `project_funded`

- **Topics**: `["vault", "project_funded"]`
- **Data**: `(project_id: u32, amount: i128, recipient: Address)`
- **Description**: Emitted when the vault transfers USDC from the vault to a project's owner.
- **Emitted by**: [`fund_project`](INTERFACE.md#investmentvault), [`fund_project_with_approvals`](INTERFACE.md#investmentvault), [`batch_fund_projects`](INTERFACE.md#investmentvault)

### `yield_received`

- **Topics**: `["vault", "yield_received"]`
- **Data**: `(from: Address, amount: i128)`
- **Description**: Emitted when yield repayment USDC is received from a project and folded into the yield-per-share accumulator for later claims.
- **Emitted by**: [`receive_yield`](INTERFACE.md#investmentvault)

### `yield_claimed`

- **Topics**: `["vault", "yield_claimed"]`
- **Data**: `(to: Address, amount: i128)`
- **Description**: Emitted when a shareholder claims accumulated yield (#125).
- **Emitted by**: [`claim_yield`](INTERFACE.md#investmentvault)

### `insurance_claimed`

- **Topics**: `["vault", "insurance_claimed"]`
- **Data**: `(project_id: u32, recipient: Address, amount: i128)`
- **Description**: Emitted when an insurance payout is made for a defaulted project (#135).
- **Emitted by**: [`claim_insurance`](INTERFACE.md#investmentvault)

### `paused`

- **Topics**: `["vault", "paused"]`
- **Data**: `()` (no data)
- **Description**: Emitted when the vault is paused (emergency stop).
- **Emitted by**: [`pause`](INTERFACE.md#investmentvault)

### `unpaused`

- **Topics**: `["vault", "unpaused"]`
- **Data**: `()` (no data)
- **Description**: Emitted when the vault is unpaused.
- **Emitted by**: [`unpause`](INTERFACE.md#investmentvault)

### `emergency_admin_changed`

- **Topics**: `["vault", "emergency_admin_changed"]`
- **Data**: `(new_emergency_admin: Option<Address>)`
- **Description**: Emitted when the admin sets or clears the emergency-admin address (#43).
- **Emitted by**: [`set_emergency_admin`](INTERFACE.md#investmentvault)
