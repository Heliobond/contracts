use crate::types::CertificationStatus;
use soroban_sdk::{contractevent, Address, Env, Symbol};

/// Emitted when collateral is deposited for a project (#128).
#[contractevent]
pub struct CollateralDeposited {
    #[topic]
    pub project_id: u32,
    pub token: Address,
    pub depositor: Address,
    pub amount: i128,
}

/// Emitted when collateral is released back to the project owner (#128).
#[contractevent]
pub struct CollateralReleased {
    #[topic]
    pub project_id: u32,
    pub token: Address,
    pub recipient: Address,
    pub amount: i128,
}

/// Emitted when collateral is liquidated by the admin (#128).
#[contractevent]
pub struct CollateralLiquidated {
    #[topic]
    pub project_id: u32,
    pub token: Address,
    pub recipient: Address,
    pub amount: i128,
}

/// Emitted when a project's interest rate is recalculated (#129).
#[contractevent]
pub struct RateUpdated {
    #[topic]
    pub project_id: u32,
    pub rate_bps: u32,
}

/// Emitted when a whitelisted creator registers a new project.
#[contractevent]
pub struct ProjectCreated {
    #[topic]
    pub project_id: u32,
    #[topic]
    pub owner: Address,
}

/// Emitted when the oracle updates a project's credit-quality / green-impact scores.
#[contractevent]
pub struct ProjectUpdated {
    #[topic]
    pub project_id: u32,
    pub credit_quality: u32,
    pub green_impact: u32,
}

/// Emitted when impact scores change, with old and new values for off-chain
/// tracking of score history and rate recalculations (#6).
#[contractevent]
pub struct ScoreChanged {
    #[topic]
    pub project_id: u32,
    pub old_credit_quality: u32,
    pub new_credit_quality: u32,
    pub old_green_impact: u32,
    pub new_green_impact: u32,
    pub old_rate_bps: u32,
    pub new_rate_bps: u32,
}

/// Emitted when an account's whitelist status is changed.
#[contractevent]
pub struct WhitelistSet {
    #[topic]
    pub account: Address,
    pub status: bool,
}

/// Emitted when a project's certification status is updated (#130).
#[contractevent]
pub struct ProjectCertified {
    #[topic]
    pub project_id: u32,
    pub status: CertificationStatus,
}

/// Emitted when a governance proposal is created (#134).
#[contractevent]
pub struct ProposalCreated {
    #[topic]
    pub proposal_id: u32,
    pub proposer: Address,
    pub voting_ends_at: u64,
}

/// Emitted when a vote is cast on a proposal (#134).
#[contractevent]
pub struct VoteCast {
    #[topic]
    pub proposal_id: u32,
    pub voter: Address,
    pub support: bool,
    pub weight: i128,
}

/// Emitted when a proposal is finalised (#134).
#[contractevent]
pub struct ProposalExecuted {
    #[topic]
    pub proposal_id: u32,
    pub passed: bool,
}

pub fn project_created(env: &Env, project_id: u32, owner: &Address) {
    ProjectCreated {
        project_id,
        owner: owner.clone(),
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

#[allow(clippy::too_many_arguments)]
pub fn score_changed(
    env: &Env,
    project_id: u32,
    old_credit_quality: u32,
    new_credit_quality: u32,
    old_green_impact: u32,
    new_green_impact: u32,
    old_rate_bps: u32,
    new_rate_bps: u32,
) {
    ScoreChanged {
        project_id,
        old_credit_quality,
        new_credit_quality,
        old_green_impact,
        new_green_impact,
        old_rate_bps,
        new_rate_bps,
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

#[allow(deprecated)]
pub fn project_archived(env: &Env, project_id: u32) {
    env.events()
        .publish((Symbol::new(env, "project_archived"),), project_id);
}

#[allow(deprecated)]
pub fn project_deleted(env: &Env, project_id: u32) {
    env.events()
        .publish((Symbol::new(env, "project_deleted"),), project_id);
}

pub fn project_certified(env: &Env, project_id: u32, status: CertificationStatus) {
    ProjectCertified { project_id, status }.publish(env);
}

pub fn proposal_created(env: &Env, proposal_id: u32, proposer: &Address, voting_ends_at: u64) {
    ProposalCreated {
        proposal_id,
        proposer: proposer.clone(),
        voting_ends_at,
    }
    .publish(env);
}

pub fn vote_cast(env: &Env, proposal_id: u32, voter: &Address, support: bool, weight: i128) {
    VoteCast {
        proposal_id,
        voter: voter.clone(),
        support,
        weight,
    }
    .publish(env);
}

pub fn proposal_executed(env: &Env, proposal_id: u32, passed: bool) {
    ProposalExecuted {
        proposal_id,
        passed,
    }
    .publish(env);
}

/// Emitted when the admin updates a project's credit-quality score independently (#6).
#[contractevent]
pub struct CreditQualityUpdated {
    #[topic]
    pub project_id: u32,
    pub credit_quality: u32,
}

pub fn credit_quality_updated(env: &Env, project_id: u32, credit_quality: u32) {
    CreditQualityUpdated {
        project_id,
        credit_quality,
    }
    .publish(env);
}

pub fn collateral_deposited(
    env: &Env,
    project_id: u32,
    token: &Address,
    depositor: &Address,
    amount: i128,
) {
    CollateralDeposited {
        project_id,
        token: token.clone(),
        depositor: depositor.clone(),
        amount,
    }
    .publish(env);
}

pub fn collateral_released(
    env: &Env,
    project_id: u32,
    token: &Address,
    recipient: &Address,
    amount: i128,
) {
    CollateralReleased {
        project_id,
        token: token.clone(),
        recipient: recipient.clone(),
        amount,
    }
    .publish(env);
}

pub fn collateral_liquidated(
    env: &Env,
    project_id: u32,
    token: &Address,
    recipient: &Address,
    amount: i128,
) {
    CollateralLiquidated {
        project_id,
        token: token.clone(),
        recipient: recipient.clone(),
        amount,
    }
    .publish(env);
}

pub fn rate_updated(env: &Env, project_id: u32, rate_bps: u32) {
    RateUpdated {
        project_id,
        rate_bps,
    }
    .publish(env);
}

/// Emitted when a creator's reputation score is updated (#46).
#[contractevent]
pub struct ReputationUpdated {
    #[topic]
    pub creator: Address,
    pub score: u32,
}

pub fn reputation_updated(env: &Env, creator: &Address, score: u32) {
    ReputationUpdated {
        creator: creator.clone(),
        score,
    }
    .publish(env);
}

/// Emitted when the registry is paused (#72).
#[contractevent]
pub struct RegistryPaused {}

/// Emitted when the registry is unpaused (#72).
#[contractevent]
pub struct RegistryUnpaused {}

pub fn registry_paused(env: &Env) {
    RegistryPaused {}.publish(env);
}

pub fn registry_unpaused(env: &Env) {
    RegistryUnpaused {}.publish(env);
}

/// Emitted when the admin replaces the whitelister address (#76).
#[contractevent]
pub struct WhitelisterChanged {
    pub old_whitelister: Address,
    pub new_whitelister: Address,
}

pub fn whitelister_changed(env: &Env, old: &Address, new: &Address) {
    WhitelisterChanged {
        old_whitelister: old.clone(),
        new_whitelister: new.clone(),
    }
    .publish(env);
}

#[allow(deprecated)]
pub fn project_compacted(env: &Env, project_id: u32) {
    env.events()
        .publish((Symbol::new(env, "project_compacted"),), project_id);
}
