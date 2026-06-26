use soroban_sdk::{contractevent, Address, BytesN, Env};

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
