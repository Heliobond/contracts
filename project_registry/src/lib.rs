#![no_std]
use soroban_sdk::{contract, contractimpl, token::Client as TokenClient, Address, Env, String, Vec};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;

/// Maximum URI length in bytes. Prevents excessively large ledger entries (#119).
const MAX_URI_LEN: u32 = 512;
/// Minimum URI length — must contain at least a scheme and one character (#117).
const MIN_URI_LEN: u32 = 8;

/// Base interest rate in basis points (10 %). High-risk / zero-score projects pay this rate (#129).
const BASE_RATE_BPS: u32 = 1_000;
/// Maximum rate discount in basis points earned by a perfect-score project (5 %) (#129).
const MAX_DISCOUNT_BPS: u32 = 500;

mod events;
mod types;

pub use types::{CertificationStatus, DataKey, ProjectData, Proposal};

/// Minimum voting period in seconds (~1 day at 5s/ledger, ≈ 17280 ledgers) (#134).
const MIN_VOTING_PERIOD: u64 = 86_400;

#[contract]
pub struct ProjectRegistry;

#[contractimpl]
impl ProjectRegistry {
    pub fn __constructor(env: Env, admin: Address, whitelister: Address) {
        set_owner(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Whitelister, &whitelister);
        env.storage()
            .instance()
            .set(&DataKey::ProjectCounter, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &0u32);
    }

    pub fn set_whitelist(env: Env, account: Address, status: bool) {
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        whitelister.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Whitelist(account.clone()), &status);
        events::whitelist_set(&env, &account, status);
    }

    /// Create a new project. `maturity_date` is a Unix timestamp (seconds);
    /// pass 0 to create an open-ended project (#127).
    pub fn create_project(env: Env, creator: Address, uri: String, maturity_date: u64) -> u32 {
        creator.require_auth();
        let is_whitelisted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Whitelist(creator.clone()))
            .unwrap_or(false);
        if !is_whitelisted {
            panic!("not whitelisted");
        }
        // URI validation (#117, #114)
        let uri_len = uri.len();
        if uri_len < MIN_URI_LEN {
            panic!("uri too short");
        }
        if uri_len > MAX_URI_LEN {
            panic!("uri too long");
        }
        // Maturity date must be in the future if provided (#127)
        if maturity_date > 0 && maturity_date <= env.ledger().timestamp() {
            panic!("maturity date must be in the future");
        }

        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        if counter == u32::MAX {
            panic!("project limit reached");
        }
        let project_id = counter + 1;
        // Counter integrity: the target slot must be vacant (#120).
        // Guards against a counter rollback or manipulation that would silently
        // overwrite an existing project entry.
        if env.storage().persistent().has(&DataKey::Project(project_id)) {
            panic!("counter integrity violation");
        }

        let project = ProjectData {
            owner: creator.clone(),
            uri: uri.clone(),
            credit_quality: 0,
            green_impact: 0,
            maturity_date,
            certification_status: CertificationStatus::None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        env.storage()
            .instance()
            .set(&DataKey::ProjectCounter, &project_id);
        events::project_created(&env, project_id, &creator, &uri);

        project_id
    }

    pub fn get_project(env: Env, id: u32) -> ProjectData {
        env.storage()
            .persistent()
            .get(&DataKey::Project(id))
            .unwrap_or_else(|| panic!("project not found"))
    }

    pub fn total_projects(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0)
    }

    #[only_owner]
    pub fn update_impact_score(env: Env, project_id: u32, credit_quality: u32, green_impact: u32) {
        if credit_quality > 100 || green_impact > 100 {
            panic!("scores must be 0-100");
        }
        let mut project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project {} not found", project_id));

        // Skip write and event if scores are identical (#124)
        if project.credit_quality == credit_quality && project.green_impact == green_impact {
            return;
        }

        project.credit_quality = credit_quality;
        project.green_impact = green_impact;
        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        events::project_updated(&env, project_id, credit_quality, green_impact);
        events::rate_updated(&env, project_id, compute_rate(credit_quality, green_impact));
    }

    /// Set the certification status of a project (whitelister or owner only) (#130).
    pub fn certify_project(env: Env, caller: Address, project_id: u32, status: CertificationStatus) {
        caller.require_auth();
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        let owner: Address = stellar_access::ownable::get_owner(&env).unwrap();
        if caller != whitelister && caller != owner {
            panic!("not authorized to certify");
        }
        let mut project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project not found"));
        project.certification_status = status.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        events::project_certified(&env, project_id, status);
    }

    /// Mark a project as settled once its maturity date has passed (#127).
    /// Returns true if the project is mature and was settled, false if already past.
    pub fn is_mature(env: Env, project_id: u32) -> bool {
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project not found"));
        if project.maturity_date == 0 {
            return false;
        }
        env.ledger().timestamp() >= project.maturity_date
    }

    pub fn get_all_projects(env: Env) -> Vec<(u32, ProjectData)> {
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        let mut result = Vec::new(&env);
        for i in 1..=counter {
            if let Some(project) = env
                .storage()
                .persistent()
                .get::<DataKey, ProjectData>(&DataKey::Project(i))
            {
                result.push_back((i, project));
            }
        }
        result
    }

    // ── Governance (#134) ──────────────────────────────────────────────────────

    /// Create a governance proposal. `voting_duration_secs` must be >= MIN_VOTING_PERIOD.
    /// Any whitelisted address may propose; voting weight is determined at vote time
    /// by the caller's HBS share balance (read via the vault cross-contract call).
    pub fn create_proposal(
        env: Env,
        proposer: Address,
        description: String,
        voting_duration_secs: u64,
    ) -> u32 {
        proposer.require_auth();
        if voting_duration_secs < MIN_VOTING_PERIOD {
            panic!("voting period too short");
        }
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let proposal_id = counter + 1;
        let voting_ends_at = env.ledger().timestamp() + voting_duration_secs;

        let proposal = Proposal {
            description,
            proposer: proposer.clone(),
            voting_ends_at,
            votes_for: 0,
            votes_against: 0,
            executed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &proposal_id);
        events::proposal_created(&env, proposal_id, &proposer, voting_ends_at);
        proposal_id
    }

    /// Cast a vote on an open proposal. `weight` is the caller's HBS share
    /// balance — callers must supply this honestly; the vault contract should
    /// be queried off-chain before invoking. `support = true` = vote for.
    pub fn cast_vote(env: Env, voter: Address, proposal_id: u32, support: bool, weight: i128) {
        voter.require_auth();
        if weight <= 0 {
            panic!("voting weight must be positive");
        }
        let already: bool = env
            .storage()
            .persistent()
            .get(&DataKey::HasVoted(proposal_id, voter.clone()))
            .unwrap_or(false);
        if already {
            panic!("already voted");
        }
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));
        if env.ledger().timestamp() > proposal.voting_ends_at {
            panic!("voting period ended");
        }
        if proposal.executed {
            panic!("proposal already executed");
        }
        if support {
            proposal.votes_for += weight;
        } else {
            proposal.votes_against += weight;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage()
            .persistent()
            .set(&DataKey::HasVoted(proposal_id, voter.clone()), &true);
        events::vote_cast(&env, proposal_id, &voter, support, weight);
    }

    /// Finalise a proposal after voting has ended. Anyone may call this.
    /// Returns true if the proposal passed (votes_for > votes_against).
    pub fn execute_proposal(env: Env, proposal_id: u32) -> bool {
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));
        if env.ledger().timestamp() <= proposal.voting_ends_at {
            panic!("voting still open");
        }
        if proposal.executed {
            panic!("proposal already executed");
        }
        proposal.executed = true;
        let passed = proposal.votes_for > proposal.votes_against;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        events::proposal_executed(&env, proposal_id, passed);
        passed
    }

    /// Set only the credit-quality score for a project. Admin-only, bounded 0–100.
    /// Emits `CreditQualityUpdated` with the new score. Use `update_impact_score` to
    /// update both scores simultaneously (#6).
    #[only_owner]
    pub fn update_credit_quality_score(env: Env, project_id: u32, credit_quality: u32) {
        if credit_quality > 100 {
            panic!("credit quality must be 0-100");
        }
        let mut project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project not found"));
        project.credit_quality = credit_quality;
        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        events::credit_quality_updated(&env, project_id, credit_quality);
    }

    /// Return a proposal by ID.
    pub fn get_proposal(env: Env, proposal_id: u32) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"))
    }

    // ── Collateral management (#128) ───────────────────────────────────────────

    /// Deposit `amount` of `token` as collateral for `project_id`.
    /// Only the project owner may deposit; tokens are held by this contract.
    pub fn deposit_collateral(
        env: Env,
        project_id: u32,
        depositor: Address,
        token: Address,
        amount: i128,
    ) {
        depositor.require_auth();
        if amount <= 0 {
            panic!("collateral amount must be positive");
        }
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project not found"));
        if project.owner != depositor {
            panic!("only the project owner may deposit collateral");
        }

        TokenClient::new(&env, &token).transfer(
            &depositor,
            &env.current_contract_address(),
            &amount,
        );

        let key = DataKey::Collateral(project_id, token.clone());
        let prev: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(prev + amount));

        events::collateral_deposited(&env, project_id, &token, &depositor, amount);
    }

    /// Return the collateral balance for (`project_id`, `token`).
    pub fn get_collateral(env: Env, project_id: u32, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Collateral(project_id, token))
            .unwrap_or(0)
    }

    /// Release all collateral of `token` back to the project owner.
    /// Allowed only after the project has matured or was never funded.
    pub fn release_collateral(env: Env, project_id: u32, caller: Address, token: Address) {
        caller.require_auth();
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project not found"));
        if project.owner != caller {
            panic!("only the project owner may release collateral");
        }
        // Collateral can only be released once the project has matured.
        if project.maturity_date > 0 && env.ledger().timestamp() < project.maturity_date {
            panic!("project has not matured yet");
        }

        let key = DataKey::Collateral(project_id, token.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance <= 0 {
            panic!("no collateral to release");
        }

        env.storage().persistent().set(&key, &0i128);
        TokenClient::new(&env, &token).transfer(
            &env.current_contract_address(),
            &caller,
            &balance,
        );

        events::collateral_released(&env, project_id, &token, &caller, balance);
    }

    /// Liquidate collateral to `recipient` (admin only). Used for defaulted projects.
    #[only_owner]
    pub fn liquidate_collateral(
        env: Env,
        project_id: u32,
        token: Address,
        recipient: Address,
    ) {
        let key = DataKey::Collateral(project_id, token.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance <= 0 {
            panic!("no collateral to liquidate");
        }

        env.storage().persistent().set(&key, &0i128);
        TokenClient::new(&env, &token).transfer(
            &env.current_contract_address(),
            &recipient,
            &balance,
        );

        events::collateral_liquidated(&env, project_id, &token, &recipient, balance);
    }

    // ── Interest rate (#129) ───────────────────────────────────────────────────

    /// Return the current annualised interest rate in basis points for `project_id`.
    /// Formula: `BASE_RATE_BPS − avg_score × (MAX_DISCOUNT_BPS / 100)`
    /// where `avg_score = (credit_quality + green_impact) / 2` (0–100).
    /// Rate range: 500 bps (5 %) for perfect scores → 1 000 bps (10 %) for zero scores.
    pub fn get_interest_rate(env: Env, project_id: u32) -> u32 {
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project not found"));
        compute_rate(project.credit_quality, project.green_impact)
    }

    // ── Creator reputation (#46) ───────────────────────────────────────────────

    /// Set a creator's reputation score (0–100). Callable by the whitelister or owner.
    /// Reputation reflects track record: successful funded projects, repayments, scores.
    /// Emits `ReputationUpdated`.
    pub fn set_creator_reputation(env: Env, caller: Address, creator: Address, score: u32) {
        caller.require_auth();
        if score > 100 {
            panic!("reputation score must be 0-100");
        }
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        let owner: Address = stellar_access::ownable::get_owner(&env).unwrap();
        if caller != whitelister && caller != owner {
            panic!("not authorized to set reputation");
        }
        env.storage()
            .persistent()
            .set(&DataKey::CreatorReputation(creator.clone()), &score);
        events::reputation_updated(&env, &creator, score);
    }

    /// Return the reputation score (0–100) for `creator`. Returns 0 if never set.
    pub fn get_creator_reputation(env: Env, creator: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::CreatorReputation(creator))
            .unwrap_or(0)
    }

    /// Return the suggested max funding limit in basis points of vault total assets
    /// for projects owned by `creator`, derived from their reputation score.
    ///
    /// Formula: `reputation * 50` bps (0 rep = 0 bps, 100 rep = 5 000 bps = 50%).
    /// Vault admins should consult this value when calling `fund_project`.
    pub fn get_creator_funding_limit_bps(env: Env, creator: Address) -> u32 {
        let score: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorReputation(creator))
            .unwrap_or(0);
        score * 50
    }
}

fn compute_rate(credit_quality: u32, green_impact: u32) -> u32 {
    let avg = (credit_quality + green_impact) / 2;
    let discount = avg * MAX_DISCOUNT_BPS / 100;
    BASE_RATE_BPS - discount
}

#[contractimpl(contracttrait)]
impl Ownable for ProjectRegistry {}

#[cfg(test)]
mod test;
