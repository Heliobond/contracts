use soroban_sdk::{contractevent, Address, Env};

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
    YieldReceived { from: from.clone(), amount }.publish(env);
}

pub fn yield_claimed(env: &Env, to: &Address, amount: i128) {
    YieldClaimed { to: to.clone(), amount }.publish(env);
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
    ManagementFeeSet { recipient: recipient.clone(), fee_bps }.publish(env);
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
    FundingThresholdsSet { min_credit_quality, min_green_impact }.publish(env);
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
