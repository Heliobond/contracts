use soroban_sdk::{contractevent, Address, Env, String};

/// Emitted when a whitelisted creator registers a new project.
#[contractevent]
pub struct ProjectCreated {
    #[topic]
    pub project_id: u32,
    pub owner: Address,
    pub uri: String,
}

/// Emitted when the oracle updates a project's credit-quality / green-impact scores.
#[contractevent]
pub struct ProjectUpdated {
    #[topic]
    pub project_id: u32,
    pub credit_quality: u32,
    pub green_impact: u32,
}

/// Emitted when an account's whitelist status is changed.
#[contractevent]
pub struct WhitelistSet {
    #[topic]
    pub account: Address,
    pub status: bool,
}

pub fn project_created(env: &Env, project_id: u32, owner: &Address, uri: &String) {
    ProjectCreated {
        project_id,
        owner: owner.clone(),
        uri: uri.clone(),
    }
    .publish(env);
}

pub fn project_updated(env: &Env, project_id: u32, credit_quality: u32, green_impact: u32) {
    ProjectUpdated {
        project_id,
        credit_quality,
        green_impact,
    }
    .publish(env);
}

pub fn whitelist_set(env: &Env, account: &Address, status: bool) {
    WhitelistSet {
        account: account.clone(),
        status,
    }
    .publish(env);
}
