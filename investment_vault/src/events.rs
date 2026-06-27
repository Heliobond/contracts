use soroban_sdk::{contractevent, Address, Bytes, BytesN, Env};

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

/// Emitted when the bridge contract address is set or updated.
#[contractevent]
pub struct BridgeSet {
    #[topic]
    pub bridge: Address,
}

/// Emitted when HBS tokens are minted through an inbound bridge transfer.
#[contractevent]
pub struct BridgeMint {
    #[topic]
    pub to: Address,
    pub amount: i128,
}

/// Emitted when HBS tokens are burned for an outbound bridge transfer.
#[contractevent]
pub struct BridgeBurn {
    #[topic]
    pub from: Address,
    pub amount: i128,
}

pub fn project_funded(env: &Env, project_id: u32, amount: i128, recipient: &Address) {
    ProjectFunded {
        project_id,
        amount,
        recipient: recipient.clone(),
    }
    .publish(env);
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

/// Emitted when an outbound bridge transfer is initiated.
#[contractevent]
pub struct BridgeTransferInitiated {
    #[topic]
    pub from: Address,
    pub amount: i128,
    pub target_chain: u32,
    pub recipient: BytesN<32>,
    pub sequence: u64,
}

/// Emitted when an inbound bridge transfer is completed.
#[contractevent]
pub struct BridgeTransferCompleted {
    pub source_chain: u32,
    #[topic]
    pub emitter: BytesN<32>,
    #[topic]
    pub to: Address,
    pub amount: i128,
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

/// Emitted when a flash loan is executed.
#[contractevent]
pub struct FlashLoan {
    #[topic]
    pub initiator: Address,
    #[topic]
    pub borrower: Address,
    pub amount: i128,
    pub fee: i128,
}

/// Emitted when the flash loan fee is updated.
#[contractevent]
pub struct FlashLoanFeeSet {
    pub fee_bps: i128,
}

pub fn flash_loan(
    env: &Env,
    initiator: &Address,
    borrower: &Address,
    amount: i128,
    fee: i128,
) {
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

/// Emitted when the carbon credit oracle address is set or updated.
#[contractevent]
pub struct CarbonOracleSet {
    #[topic]
    pub oracle: Address,
}

/// Emitted when the carbon credit oracle updates the per-credit price.
#[contractevent]
pub struct CarbonCreditPriceSet {
    pub price: i128,
}

/// Emitted when carbon credits are calculated for a project investment.
#[contractevent]
pub struct CarbonCreditsCalculated {
    #[topic]
    pub project_id: u32,
    pub amount_invested: i128,
    pub credits: i128,
}

/// Emitted when carbon credits are transferred between accounts.
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

pub fn carbon_credits_calculated(
    env: &Env,
    project_id: u32,
    amount_invested: i128,
    credits: i128,
) {
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
