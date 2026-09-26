# Heliobond Events Catalog

This document provides a catalog of all events emitted by the Heliobond smart contracts for off-chain indexers and developers.

Each event lists the public [`INTERFACE.md`](INTERFACE.md) function(s) that emit it, so indexer authors can trace an event back to the call that produced it.

## Project Registry Events

### `project_created`
- **Topics**: `["project", "created"]`
- **Data**: `(project_id: u32, owner: Address)`
- **Description**: Emitted when a new project is created in the registry.
- **Emitted by**: [`create_project`](INTERFACE.md#projectregistry)

### `project_updated`
- **Topics**: `["project", "updated"]`
- **Data**: `(project_id: u32, credit_quality: u32, green_impact: u32)`
- **Description**: Emitted when both impact scores are updated together.
- **Emitted by**: [`update_impact_score`](INTERFACE.md#projectregistry) / [`update_impact_score_approved`](INTERFACE.md#projectregistry)

### `credit_quality_updated`
- **Topics**: `["project", "credit_quality_updated"]`
- **Data**: `(project_id: u32, credit_quality: u32)`
- **Description**: Emitted when only the credit-quality score is updated.
- **Emitted by**: [`update_credit_quality_score`](INTERFACE.md#projectregistry)

### `rate_updated`
- **Topics**: `["project", "rate_updated"]`
- **Data**: `(project_id: u32, rate_bps: u32)`
- **Description**: Emitted alongside `project_updated`/`score_changed` whenever a project's interest rate is recalculated.
- **Emitted by**: [`update_impact_score`](INTERFACE.md#projectregistry) / [`update_impact_score_approved`](INTERFACE.md#projectregistry) / [`update_impact_scores_batch`](INTERFACE.md#projectregistry) / [`update_scores_batch_approved`](INTERFACE.md#projectregistry)

### `score_changed`
- **Topics**: `["score_changed", project_id: u32]`
- **Data** (Map, keyed by field name): `{old_credit_quality: u32, new_credit_quality: u32, old_green_impact: u32, new_green_impact: u32, old_rate_bps: u32, new_rate_bps: u32}`
- **Description**: Emitted when a project's impact scores and corresponding interest rate are updated.
- **Emitted by**: [`update_impact_score`](INTERFACE.md#projectregistry) / [`update_impact_score_approved`](INTERFACE.md#projectregistry) (both scores), [`update_credit_quality_score`](INTERFACE.md#projectregistry) (credit quality only)

### `project_archived`
- **Topics**: `["project", "archived"]`
- **Data**: `(project_id: u32)`
- **Description**: Emitted when a project is archived.
- **Emitted by**: [`archive_project`](INTERFACE.md#projectregistry)

### `project_status_changed`
- **Topics**: `["project_status_changed", project_id: u32]`
- **Data**: `(old_status: ProjectStatus, new_status: ProjectStatus)`
- **Description**: Emitted when a project's lifecycle status transitions between `Pending`, `Active`, `Funded`, or `Completed` (#329). Not emitted for `Archived` — see `project_archived`.
- **Emitted by**: [`set_project_status`](INTERFACE.md#projectregistry)

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
- **Description**: Emitted when collateral is seized to `recipient` for a defaulted project.
- **Emitted by**: [`liquidate_collateral`](INTERFACE.md#projectregistry) / [`liquidate_collateral_approved`](INTERFACE.md#projectregistry)

### `whitelist_set`
- **Topics**: `["whitelist_set"]`
- **Data**: `(account: Address, status: bool)`
- **Description**: Emitted when an account's project-creation whitelist status is granted or revoked.
- **Emitted by**: [`set_whitelist`](INTERFACE.md#projectregistry)

### `project_certified`
- **Topics**: `["project", "certified"]`
- **Data**: `(project_id: u32, status: CertificationStatus)`
- **Description**: Emitted when a project's certification status changes.
- **Emitted by**: [`certify_project`](INTERFACE.md#projectregistry)

### `proposal_created`
- **Topics**: `["proposal_created"]`
- **Data**: `(proposal_id: u32, proposer: Address, voting_ends_at: u64)`
- **Description**: Emitted when a governance proposal is created.
- **Emitted by**: [`create_proposal`](INTERFACE.md#projectregistry)

### `vote_cast`
- **Topics**: `["vote_cast"]`
- **Data**: `(proposal_id: u32, voter: Address, support: bool, weight: i128)`
- **Description**: Emitted when a vote is cast on an open proposal.
- **Emitted by**: [`cast_vote`](INTERFACE.md#projectregistry)

### `proposal_executed`
- **Topics**: `["proposal_executed"]`
- **Data**: `(proposal_id: u32, passed: bool)`
- **Description**: Emitted when a proposal is finalised after its voting period ends.
- **Emitted by**: [`execute_proposal`](INTERFACE.md#projectregistry)

### `reputation_updated`
- **Topics**: `["reputation_updated"]`
- **Data**: `(creator: Address, score: u32)`
- **Description**: Emitted when a creator's reputation score is set.
- **Emitted by**: [`set_creator_reputation`](INTERFACE.md#projectregistry)

### `registry_paused` / `registry_unpaused`
- **Topics**: `["registry_paused"]` / `["registry_unpaused"]`
- **Data**: none
- **Description**: Emitted when the circuit breaker is engaged or released.
- **Emitted by**: [`pause`](INTERFACE.md#projectregistry) / [`unpause`](INTERFACE.md#projectregistry) / [`emergency_pause`](INTERFACE.md#projectregistry) / [`emergency_unpause`](INTERFACE.md#projectregistry)

### `emergency_admin_changed`
- **Topics**: `["emergency_admin_changed"]`
- **Data**: `(new_emergency_admin: Option<Address>)`
- **Description**: Emitted when the owner sets or clears the emergency-admin address.
- **Emitted by**: [`set_emergency_admin`](INTERFACE.md#projectregistry)

### `whitelister_changed`
- **Topics**: `["whitelister_changed"]`
- **Data**: `(old_whitelister: Address, new_whitelister: Address)`
- **Description**: Emitted when the owner replaces the whitelister address.
- **Emitted by**: [`set_whitelister`](INTERFACE.md#projectregistry)

### `ownership_transferred`
- **Topics**: `["ownership_transferred", old_owner: Address, new_owner: Address]`
- **Data**: none
- **Description**: Emitted alongside the underlying `stellar-access` ownership-transfer event when a two-step ownership transfer is initiated.
- **Emitted by**: `transfer_ownership` (from the `Ownable` trait)

## Investment Vault Events

### `deposit`
- **Topics**: `["vault", "deposit"]`
- **Data**: `(from: Address, usdc_amount: i128, shares_minted: i128)`
- **Description**: Emitted when an investor deposits USDC and receives newly minted vault shares.
- **Emitted by**: [`deposit`](INTERFACE.md#investmentvault)

### `withdraw`
- **Topics**: `["vault", "withdraw"]`
- **Data**: `(from: Address, shares_burned: i128, usdc_returned: i128)`
- **Description**: Emitted when an investor burns shares and receives USDC immediately (sufficient liquid USDC was available). See `withdraw_queued` for the insufficient-liquidity path.
- **Emitted by**: [`withdraw`](INTERFACE.md#investmentvault)

### `withdraw_queued`
- **Topics**: `["vault", "withdraw_queued"]`
- **Data**: `(from: Address, shares_burned: i128, usdc_owed: i128, queued_liabilities: i128)`
- **Description**: Emitted when `withdraw` can't pay out immediately because liquid USDC is insufficient. Shares are burned right away and the USDC payout is enqueued in FIFO order; call `claim()` once liquidity is restored. `queued_liabilities` is the total unpaid queue after this entry; it is deducted from `total_assets`, so the share price of remaining holders is unchanged by the burn (#613).
- **Emitted by**: [`withdraw`](INTERFACE.md#investmentvault)

### `withdraw_claimed`
- **Topics**: `["vault", "withdraw_claimed"]`
- **Data**: `(to: Address, usdc_paid: i128, claim_index: u64, queued_liabilities: i128)`
- **Description**: Emitted when a previously queued redemption (see `withdraw_queued`) is settled. `queued_liabilities` is the total unpaid queue remaining after this payout (#613).
- **Emitted by**: [`claim`](INTERFACE.md#investmentvault)

### `paused`
- **Topics**: `["vault", "paused"]`
- **Data**: — (no fields)
- **Description**: Emitted when the vault's circuit breaker is engaged, blocking all state-mutating user operations.
- **Emitted by**: [`pause`](INTERFACE.md#investmentvault), [`emergency_pause`](INTERFACE.md#investmentvault)

### `unpaused`
- **Topics**: `["vault", "unpaused"]`
- **Data**: — (no fields)
- **Description**: Emitted when the vault's circuit breaker is disengaged, resuming normal operation.
- **Emitted by**: [`unpause`](INTERFACE.md#investmentvault), [`emergency_unpause`](INTERFACE.md#investmentvault)

### `emergency_admin_changed`
- **Topics**: `["vault", "emergency_admin_changed"]`
- **Data**: `(new_emergency_admin: Option<Address>)`
- **Description**: Emitted when the admin sets or clears the address authorised to pause/unpause the vault in an emergency without going through the owner.
- **Emitted by**: [`set_emergency_admin`](INTERFACE.md#investmentvault)

### `project_funded`
- **Topics**: `["vault", "project_funded"]`
- **Data**: `(project_id: u32, amount: i128, recipient: Address)`
- **Description**: Emitted when the vault transfers USDC from the vault to a project's owner.
- **Emitted by**: [`fund_project`](INTERFACE.md#investmentvault), [`fund_project_with_approvals`](INTERFACE.md#investmentvault), [`batch_fund_projects`](INTERFACE.md#investmentvault)

### `principal_repaid`
- **Topics**: `["principal_repaid", project_id: u32]`
- **Data**: `(from: Address, amount: i128, outstanding: i128)`
- **Description**: Emitted when a project returns principal to the vault. `outstanding` is the project's remaining deployed principal after this repayment (#631).
- **Emitted by**: [`repay_principal`](INTERFACE.md#investmentvault)

### `project_settled`
- **Topics**: `["project_settled", project_id: u32]`
- **Data**: `(funded: i128, repaid: i128, impairment: i128)`
- **Description**: Emitted when a matured project's books are closed. `impairment` is the principal that was never repaid and has been written off from `total_assets`; the project can no longer be funded or repaid. Frontends can use this (or `get_project_position`) to show the project as matured/settled (#631).
- **Emitted by**: [`settle_project`](INTERFACE.md#investmentvault)

### `yield_received`
- **Topics**: `["vault", "yield_received"]`
- **Data**: `(from: Address, amount: i128)`
- **Description**: Emitted when yield repayment USDC is received from a project and folded into the yield-per-share accumulator for later claims.
- **Emitted by**: [`receive_yield`](INTERFACE.md#investmentvault)

### `yield_claimed`
- **Topics**: `["vault", "yield_claimed"]`
- **Data**: `(to: Address, amount: i128)`
- **Description**: Emitted when a shareholder claims accumulated yield, transferring USDC out of the vault to `to`.
- **Emitted by**: [`claim_yield`](INTERFACE.md#investmentvault)

### `insurance_claimed`
- **Topics**: `["vault", "insurance_claimed"]`
- **Data**: `(project_id: u32, recipient: Address, amount: i128)`
- **Description**: Emitted when an insurance payout is made for a defaulted project, transferring USDC out of the vault to `recipient`.
- **Emitted by**: [`claim_insurance`](INTERFACE.md#investmentvault), [`claim_insurance_with_approvals`](INTERFACE.md#investmentvault)

### `bridge_transfer_initiated`
- **Topics**: `["vault", "bridge_transfer_initiated"]`
- **Data**: `(from: Address, amount: i128, target_chain: u32, recipient: BytesN<32>, sequence: u64)`
- **Description**: Emitted when an outbound Wormhole cross-chain bridge transfer is initiated: shares are burned from `from` and a message is queued for the target chain.
- **Emitted by**: [`initiate_bridge_transfer`](INTERFACE.md#investmentvault)

### `bridge_transfer_completed`
- **Topics**: `["vault", "bridge_transfer_completed"]`
- **Data**: `(source_chain: u32, emitter: BytesN<32>, to: Address, amount: i128)`
- **Description**: Emitted when an inbound Wormhole VAA is verified and its transfer completed, minting shares to `to`.
- **Emitted by**: [`complete_bridge_transfer`](INTERFACE.md#investmentvault)

### `trusted_emitter_set`
- **Topics**: `["vault", "trusted_emitter_set"]`
- **Data**: `(chain_id: u32, emitter: BytesN<32>, trusted: bool)`
- **Description**: Emitted when a cross-chain emitter address is registered or unregistered as trusted for a given source chain, gating which inbound VAAs `complete_bridge_transfer` will accept.
- **Emitted by**: [`set_trusted_emitter`](INTERFACE.md#investmentvault)

### `flash_loan`
- **Topics**: `["vault", "flash_loan"]`
- **Data**: `(initiator: Address, borrower: Address, amount: i128, fee: i128)`
- **Description**: Emitted when a flash loan is drawn and successfully repaid (principal plus fee) within the same transaction.
- **Emitted by**: [`execute_flash_loan`](INTERFACE.md#investmentvault)

### `flash_loan_fee_set`
- **Topics**: `["vault", "flash_loan_fee_set"]`
- **Data**: `(fee_bps: i128)`
- **Description**: Emitted when the flash loan fee, in basis points, is updated.
- **Emitted by**: [`set_flash_loan_fee`](INTERFACE.md#investmentvault)

### `carbon_oracle_set`
- **Topics**: `["vault", "carbon_oracle_set"]`
- **Data**: `(oracle: Address)`
- **Description**: Emitted when the carbon credit price oracle address is configured.
- **Emitted by**: [`set_carbon_oracle`](INTERFACE.md#investmentvault)

### `carbon_credit_price_set`
- **Topics**: `["vault", "carbon_credit_price_set"]`
- **Data**: `(price: i128)`
- **Description**: Emitted when the carbon credit price is updated.
- **Emitted by**: [`set_carbon_credit_price`](INTERFACE.md#investmentvault)

### `carbon_credits_calculated`
- **Topics**: `["vault", "carbon_credits_calculated"]`
- **Data**: `(project_id: u32, amount_invested: i128, credits: i128)`
- **Description**: Emitted when carbon credits are calculated and issued for an investment in a project.
- **Emitted by**: [`calculate_carbon_credits`](INTERFACE.md#investmentvault)

### `carbon_credits_transferred`
- **Topics**: `["vault", "carbon_credits_transferred"]`
- **Data**: `(from: Address, to: Address, amount: i128)`
- **Description**: Emitted when carbon credits are transferred between accounts.
- **Emitted by**: [`transfer_carbon_credits`](INTERFACE.md#investmentvault)

### `compliance_event_recorded`
- **Topics**: `["vault", "compliance_event_recorded"]`
- **Data**: `(seq: u64, event_type: String)`
- **Description**: Emitted when a compliance event is recorded to the on-chain audit trail for regulatory reporting.
- **Emitted by**: [`record_compliance_event`](INTERFACE.md#investmentvault)

### `reporting_snapshot_taken`
- **Topics**: `["vault", "reporting_snapshot_taken"]`
- **Data**: `(timestamp: u64)`
- **Description**: Emitted when a periodic snapshot of the vault's key metrics is taken for regulatory reporting.
- **Emitted by**: [`take_reporting_snapshot`](INTERFACE.md#investmentvault)

### `max_transaction_amount_set`
- **Topics**: `["vault", "max_transaction_amount_set"]`
- **Data**: `(amount: i128)`
- **Description**: Emitted when the maximum single-transaction amount compliance limit is updated.
- **Emitted by**: [`set_max_transaction_amount`](INTERFACE.md#investmentvault)

### `management_fee_set`
- **Topics**: `["vault", "management_fee_set"]`
- **Data**: `(recipient: Address, fee_bps: u32)`
- **Description**: Emitted when the admin updates the management fee configuration.
- **Emitted by**: [`set_management_fee`](INTERFACE.md#investmentvault)

### `trading_enabled`
- **Topics**: `["vault", "trading_enabled"]`
- **Data**: `(enabled: bool)`
- **Description**: Emitted when the admin enables secondary market trading for HBS shares.
- **Emitted by**: [`enable_secondary_trading`](INTERFACE.md#investmentvault)

### `funding_thresholds_set`
- **Topics**: `["vault", "funding_thresholds_set"]`
- **Data**: `(min_credit_quality: u32, min_green_impact: u32)`
- **Description**: Emitted when the admin updates the minimum funding score thresholds.
- **Emitted by**: [`set_funding_thresholds`](INTERFACE.md#investmentvault)

### `registry_changed`
- **Topics**: `["vault", "registry_changed"]`
- **Data**: `(old_registry: Address, new_registry: Address)`
- **Description**: Emitted when the admin replaces the linked ProjectRegistry contract.
- **Emitted by**: [`set_registry`](INTERFACE.md#investmentvault)

### `utilization_warning`
- **Topics**: `["vault", "utilization_warning"]`
- **Data**: `(utilization_bps: u32)`
- **Description**: Emitted during `withdraw()` whenever vault utilization crosses the high-utilization threshold (`UTIL_WARN_BPS`, 70%). Off-chain monitors should alert operators to consider replenishing liquidity.
- **Emitted by**: [`withdraw`](INTERFACE.md#investmentvault)

### `bridge_set`
- **Topics**: `["vault", "bridge_set"]`
- **Data**: `(bridge: Address)`
- **Description**: Emitted when the admin configures the direct (non-Wormhole) bridge address authorised to mint/burn vault shares.
- **Emitted by**: [`set_bridge`](INTERFACE.md#investmentvault)

### `bridge_mint`
- **Topics**: `["vault", "bridge_mint"]`
- **Data**: `(to: Address, amount: i128)`
- **Description**: Emitted when the configured direct bridge mints shares to `to` for an inbound cross-chain transfer.
- **Emitted by**: [`bridge_mint`](INTERFACE.md#investmentvault)

### `bridge_burn`
- **Topics**: `["vault", "bridge_burn"]`
- **Data**: `(from: Address, amount: i128)`
- **Description**: Emitted when the configured direct bridge burns shares from `from` for an outbound cross-chain transfer.
- **Emitted by**: [`bridge_burn`](INTERFACE.md#investmentvault)

### `ownership_transferred`
- **Topics**: `["vault", "ownership_transferred"]`
- **Data**: `(old_owner: Address, new_owner: Address)`
- **Description**: Emitted when the admin transfers contract ownership.
- **Emitted by**: [`transfer_ownership`](INTERFACE.md#investmentvault)

### `withdrawal_window_set`
- **Topics**: `["vault", "withdrawal_window_set"]`
- **Data**: `(ledgers: u32)`
- **Description**: Emitted when the admin changes the withdrawal sliding-window length. `withdraw` rejects with `DepositLocked` until `ledgers` ledgers have passed since the caller's last deposit, in addition to the time-based `MIN_LOCK_PERIOD` (1 day). `0` disables the ledger check (#530).
- **Emitted by**: [`set_withdrawal_window`](INTERFACE.md#investmentvault)

### `funding_round_started`
- **Topics**: `["vault", "funding_round_started"]`
- **Data**: — (no fields)
- **Description**: Emitted when the admin opens a funding round.
- **Emitted by**: [`start_funding_round`](INTERFACE.md#investmentvault)

### `funding_round_ended`
- **Topics**: `["vault", "funding_round_ended"]`
- **Data**: — (no fields)
- **Description**: Emitted when the admin closes a funding round.
- **Emitted by**: [`end_funding_round`](INTERFACE.md#investmentvault)

### `investment_cap_set`
- **Topics**: `["vault", "investment_cap_set"]`
- **Data**: `(cap: i128)`
- **Description**: Emitted when the admin changes the per-project investment cap.
- **Emitted by**: [`set_max_investment_per_project`](INTERFACE.md#investmentvault)
