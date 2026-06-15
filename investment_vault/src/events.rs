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
