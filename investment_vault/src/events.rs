use soroban_sdk::{contractevent, Address, BytesN, Env, String};

/// Emitted when an investor deposits USDC and receives vault shares.
#[contractevent]
pub struct Deposit {
    #[topic]
    pub from: Address,
    pub usdc_amount: i128,
    pub shares_minted: i128,
}

/// Emitted when an investor burns shares and withdraws USDC.
#[contractevent]
pub struct Withdraw {
    #[topic]
    pub from: Address,
    pub shares_burned: i128,
    pub usdc_returned: i128,
}

/// Emitted when the vault funds a registered project.
#[contractevent]
pub struct ProjectFunded {
    #[topic]
    pub project_id: u32,
    pub amount: i128,
    pub recipient: Address,
}

/// Emitted when yield USDC is received from a project repayment (#125).
#[contractevent]
pub struct YieldReceived {
    #[topic]
    pub from: Address,
    pub amount: i128,
}

/// Emitted when a shareholder claims accumulated yield (#125).
#[contractevent]
pub struct YieldClaimed {
    #[topic]
    pub to: Address,
    pub amount: i128,
}

/// Emitted when an insurance payout is made for a defaulted project (#135).
#[contractevent]
pub struct InsuranceClaimed {
    #[topic]
    pub project_id: u32,
    pub recipient: Address,
    pub amount: i128,
}

/// Emitted when a withdrawal is queued because liquid USDC is insufficient (#3).
/// Shares are burned immediately; USDC will be paid when claim() is called.
#[contractevent]
pub struct WithdrawQueued {
    #[topic]
    pub from: Address,
    pub shares_burned: i128,
    pub usdc_owed: i128,
}

/// Emitted when a queued redemption claim is settled by claim() (#3).
#[contractevent]
pub struct WithdrawClaimed {
    #[topic]
    pub to: Address,
    pub usdc_paid: i128,
    pub claim_index: u64,
}

pub fn deposit(env: &Env, from: &Address, usdc_amount: i128, shares_minted: i128) {
    Deposit {
        from: from.clone(),
        usdc_amount,
        shares_minted,
    }
    .publish(env);
}

pub fn withdraw(env: &Env, from: &Address, shares_burned: i128, usdc_returned: i128) {
    Withdraw {
        from: from.clone(),
        shares_burned,
        usdc_returned,
    }
    .publish(env);
}

pub fn project_funded(env: &Env, project_id: u32, amount: i128, recipient: &Address) {
    ProjectFunded {
        project_id,
        amount,
        recipient: recipient.clone(),
    }
    .publish(env);
}

pub fn yield_received(env: &Env, from: &Address, amount: i128) {
    YieldReceived {
        from: from.clone(),
        amount,
    }
    .publish(env);
}

pub fn yield_claimed(env: &Env, to: &Address, amount: i128) {
    YieldClaimed {
        to: to.clone(),
        amount,
    }
    .publish(env);
}

pub fn insurance_claimed(env: &Env, project_id: u32, recipient: &Address, amount: i128) {
    InsuranceClaimed {
        project_id,
        recipient: recipient.clone(),
        amount,
    }
    .publish(env);
}

pub fn withdraw_queued(env: &Env, from: &Address, shares_burned: i128, usdc_owed: i128) {
    WithdrawQueued {
        from: from.clone(),
        shares_burned,
        usdc_owed,
    }
    .publish(env);
}

pub fn withdraw_claimed(env: &Env, to: &Address, usdc_paid: i128, claim_index: u64) {
    WithdrawClaimed {
        to: to.clone(),
        usdc_paid,
        claim_index,
    }
    .publish(env);
}

/// Emitted when the admin updates the management fee configuration (#7).
#[contractevent]
pub struct ManagementFeeSet {
    #[topic]
    pub recipient: Address,
    pub fee_bps: u32,
}

pub fn management_fee_set(env: &Env, recipient: &Address, fee_bps: u32) {
    ManagementFeeSet {
        recipient: recipient.clone(),
        fee_bps,
    }
    .publish(env);
}

/// Emitted when the admin enables secondary market trading for HBS (#126).
#[contractevent]
pub struct TradingEnabled {
    pub enabled: bool,
}

pub fn trading_enabled(env: &Env, enabled: bool) {
    TradingEnabled { enabled }.publish(env);
}

/// Emitted when vault utilization crosses a high threshold during a withdrawal (#45).
/// Off-chain monitors should alert operators to consider replenishing liquidity.
#[contractevent]
pub struct UtilizationWarning {
    pub utilization_bps: u32,
}

pub fn utilization_warning(env: &Env, utilization_bps: u32) {
    UtilizationWarning { utilization_bps }.publish(env);
}

/// Emitted when the admin updates the minimum funding score thresholds (#47).
#[contractevent]
pub struct FundingThresholdsSet {
    pub min_credit_quality: u32,
    pub min_green_impact: u32,
}

pub fn funding_thresholds_set(env: &Env, min_credit_quality: u32, min_green_impact: u32) {
    FundingThresholdsSet {
        min_credit_quality,
        min_green_impact,
    }
    .publish(env);
}

/// Emitted when the admin replaces the ProjectRegistry dependency (#76).
#[contractevent]
pub struct RegistryChanged {
    pub old_registry: Address,
    pub new_registry: Address,
}

pub fn registry_changed(env: &Env, old: &Address, new: &Address) {
    RegistryChanged {
        old_registry: old.clone(),
        new_registry: new.clone(),
    }
    .publish(env);
}

// ── Bridge events ────────────────────────────────────────────────────────

#[contractevent]
pub struct BridgeSet {
    #[topic]
    pub bridge: Address,
}

#[contractevent]
pub struct BridgeMint {
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contractevent]
pub struct BridgeBurn {
    #[topic]
    pub from: Address,
    pub amount: i128,
}

#[contractevent]
pub struct BridgeTransferInitiated {
    #[topic]
    pub from: Address,
    pub amount: i128,
    pub target_chain: u32,
    pub recipient: BytesN<32>,
    pub sequence: u64,
}

#[contractevent]
pub struct BridgeTransferCompleted {
    pub source_chain: u32,
    #[topic]
    pub emitter: BytesN<32>,
    #[topic]
    pub to: Address,
    pub amount: i128,
}

/// Emitted when a cross-chain emitter is registered or unregistered (#48).
#[contractevent]
pub struct TrustedEmitterSet {
    pub chain_id: u32,
    #[topic]
    pub emitter: BytesN<32>,
    pub trusted: bool,
}

pub fn bridge_set(env: &Env, bridge: &Address) {
    BridgeSet {
        bridge: bridge.clone(),
    }
    .publish(env);
}

pub fn bridge_mint(env: &Env, to: &Address, amount: i128) {
    BridgeMint {
        to: to.clone(),
        amount,
    }
    .publish(env);
}

pub fn bridge_burn(env: &Env, from: &Address, amount: i128) {
    BridgeBurn {
        from: from.clone(),
        amount,
    }
    .publish(env);
}

pub fn bridge_transfer_initiated(
    env: &Env,
    from: &Address,
    amount: i128,
    target_chain: u32,
    recipient: &BytesN<32>,
    sequence: u64,
) {
    BridgeTransferInitiated {
        from: from.clone(),
        amount,
        target_chain,
        recipient: recipient.clone(),
        sequence,
    }
    .publish(env);
}

pub fn trusted_emitter_set(env: &Env, chain_id: u32, emitter: &BytesN<32>, trusted: bool) {
    TrustedEmitterSet {
        chain_id,
        emitter: emitter.clone(),
        trusted,
    }
    .publish(env);
}

pub fn bridge_transfer_completed(
    env: &Env,
    source_chain: u32,
    emitter: &BytesN<32>,
    to: &Address,
    amount: i128,
) {
    BridgeTransferCompleted {
        source_chain,
        emitter: emitter.clone(),
        to: to.clone(),
        amount,
    }
    .publish(env);
}

// ── Flash loan events ────────────────────────────────────────────────────

#[contractevent]
pub struct FlashLoan {
    #[topic]
    pub initiator: Address,
    #[topic]
    pub borrower: Address,
    pub amount: i128,
    pub fee: i128,
}

#[contractevent]
pub struct FlashLoanFeeSet {
    pub fee_bps: i128,
}

pub fn flash_loan(env: &Env, initiator: &Address, borrower: &Address, amount: i128, fee: i128) {
    FlashLoan {
        initiator: initiator.clone(),
        borrower: borrower.clone(),
        amount,
        fee,
    }
    .publish(env);
}

pub fn flash_loan_fee_set(env: &Env, fee_bps: i128) {
    FlashLoanFeeSet { fee_bps }.publish(env);
}

// ── Carbon credit events ─────────────────────────────────────────────────

#[contractevent]
pub struct CarbonOracleSet {
    #[topic]
    pub oracle: Address,
}

#[contractevent]
pub struct CarbonCreditPriceSet {
    pub price: i128,
}

#[contractevent]
pub struct CarbonCreditsCalculated {
    #[topic]
    pub project_id: u32,
    pub amount_invested: i128,
    pub credits: i128,
}

#[contractevent]
pub struct CarbonCreditsTransferred {
    #[topic]
    pub from: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
}

pub fn carbon_oracle_set(env: &Env, oracle: &Address) {
    CarbonOracleSet {
        oracle: oracle.clone(),
    }
    .publish(env);
}

pub fn carbon_credit_price_set(env: &Env, price: i128) {
    CarbonCreditPriceSet { price }.publish(env);
}

pub fn carbon_credits_calculated(env: &Env, project_id: u32, amount_invested: i128, credits: i128) {
    CarbonCreditsCalculated {
        project_id,
        amount_invested,
        credits,
    }
    .publish(env);
}

pub fn carbon_credits_transferred(env: &Env, from: &Address, to: &Address, amount: i128) {
    CarbonCreditsTransferred {
        from: from.clone(),
        to: to.clone(),
        amount,
    }
    .publish(env);
}

// ── Compliance / regulatory events ───────────────────────────────────────

#[contractevent]
pub struct ComplianceEventRecorded {
    pub seq: u64,
    #[topic]
    pub event_type: String,
}

#[contractevent]
pub struct ReportingSnapshotTaken {
    pub timestamp: u64,
}

#[contractevent]
pub struct MaxTransactionAmountSet {
    pub amount: i128,
}

pub fn compliance_event_recorded(env: &Env, seq: u64, event_type: &String) {
    ComplianceEventRecorded {
        seq,
        event_type: event_type.clone(),
    }
    .publish(env);
}

pub fn reporting_snapshot_taken(env: &Env, timestamp: u64) {
    ReportingSnapshotTaken { timestamp }.publish(env);
}

pub fn max_transaction_amount_set(env: &Env, amount: i128) {
    MaxTransactionAmountSet { amount }.publish(env);
}
