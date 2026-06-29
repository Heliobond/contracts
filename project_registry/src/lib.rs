#![no_std]
use soroban_sdk::{
    contract, contractimpl, panic_with_error, token::Client as TokenClient, Address, Env, String,
    Vec,
};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;

/// Maximum URI length in bytes. Prevents excessively large ledger entries (#119).
const MAX_URI_LEN: u32 = 512;
/// Minimum URI length — must contain at least a scheme and one character (#117).
const MIN_URI_LEN: u32 = 8;
/// Current schema version for instance and persistent contract state (#66).
const STATE_VERSION: u32 = 1;

/// Base interest rate in basis points (10 %). High-risk / zero-score projects pay this rate (#129).
const BASE_RATE_BPS: u32 = 1_000;
/// Maximum rate discount in basis points earned by a perfect-score project (5 %) (#129).
const MAX_DISCOUNT_BPS: u32 = 500;
const MAX_MULTISIG_SIGNERS: u32 = 10;

mod events;
mod types;

pub use types::{CertificationStatus, DataKey, ProjectData, Proposal, RegistryError};

/// Minimum voting period in seconds (~1 day at 5s/ledger, ≈ 17280 ledgers) (#134).
const MIN_VOTING_PERIOD: u64 = 86_400;

/// Minimum oracle update interval in seconds (1 hour).
const MIN_UPDATE_INTERVAL: u64 = 3600;

pub const CONTRACT_NAME: &str = "Project Registry";
pub const CONTRACT_DESCRIPTION: &str = "Heliobond Project Registry";
pub const CONTRACT_VERSION: &str = "1.0.0";

#[contract]
pub struct ProjectRegistry;

#[contractimpl]
impl ProjectRegistry {
    /// Initialise the registry.
    ///
    /// - `admin` — contract owner; may update scores, certify, and liquidate collateral.
    /// - `whitelister` — address authorised to approve creator accounts via `set_whitelist`.
    pub fn __constructor(env: Env, admin: Address, whitelister: Address) {
        set_owner(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::StateVersion, &STATE_VERSION);
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

    /// Return the state schema version supported by this contract build.
    pub fn state_version(_env: Env) -> u32 {
        STATE_VERSION
    }

    /// Return the version recorded in instance storage. Unversioned deployments report 0.
    pub fn stored_state_version(env: Env) -> u32 {
        read_state_version(&env)
    }

    /// Migrate older state to the current schema version.
    ///
    /// Version 0 means a deployment that predates explicit state versioning. The v1
    /// migration only records the version because existing storage layouts are unchanged.
    #[only_owner]
    pub fn migrate_state(env: Env, from_version: u32) -> u32 {
        let current = read_state_version(&env);
        if current != from_version || current > STATE_VERSION {
            panic_with_error!(&env, RegistryError::UnsupportedStateVersion);
        }
        if current < STATE_VERSION {
            env.storage()
                .instance()
                .set(&DataKey::StateVersion, &STATE_VERSION);
        }
        STATE_VERSION
    }

    /// Grant or revoke project-creation rights for `account`.
    /// Only the whitelister address (set at construction) may call this.
    pub fn set_whitelist(env: Env, account: Address, status: bool) {
        require_current_state(&env);
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
        require_current_state(&env);
        creator.require_auth();
        let is_whitelisted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Whitelist(creator.clone()))
            .unwrap_or(false);
        if !is_whitelisted {
            panic_with_error!(&env, RegistryError::NotWhitelisted);
        }
        // URI validation (#117, #114, #29)
        let uri_len = uri.len();
        if uri_len < MIN_URI_LEN {
            panic_with_error!(&env, RegistryError::UriTooShort);
        }
        if uri_len > MAX_URI_LEN {
            panic_with_error!(&env, RegistryError::UriTooLong);
        }
        // Validate URI scheme: must start with ipfs://, https://, or ar:// (#29)
        let uri_bytes = uri.to_bytes();
        let mut has_valid_scheme = false;
        if uri_len >= 7 {
            let prefix7 = String::from(uri_bytes.slice(0..7));
            if prefix7 == String::from_str(&env, "ipfs://") {
                has_valid_scheme = true;
            }
        }
        if !has_valid_scheme && uri_len >= 8 {
            let prefix8 = String::from(uri_bytes.slice(0..8));
            if prefix8 == String::from_str(&env, "https://") {
                has_valid_scheme = true;
            }
        }
        if !has_valid_scheme && uri_len >= 5 {
            let prefix5 = String::from(uri_bytes.slice(0..5));
            if prefix5 == String::from_str(&env, "ar://") {
                has_valid_scheme = true;
            }
        }
        if !has_valid_scheme {
            panic_with_error!(&env, RegistryError::InvalidUriScheme);
        }
        // Maturity date must be in the future if provided (#127)
        if maturity_date > 0 && maturity_date <= env.ledger().timestamp() {
            panic_with_error!(&env, RegistryError::MaturityDateInPast);
        }

        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        if counter == u32::MAX {
            panic_with_error!(&env, RegistryError::ProjectLimitReached);
        }
        let project_id = counter + 1;
        // Counter integrity: the target slot must be vacant (#120).
        // Guards against a counter rollback or manipulation that would silently
        // overwrite an existing project entry.
        if env
            .storage()
            .persistent()
            .has(&DataKey::Project(project_id))
        {
            panic_with_error!(&env, RegistryError::CounterIntegrityViolation);
        }

        let project = ProjectData {
            owner: creator.clone(),
            uri: uri.clone(),
            credit_quality: 0,
            green_impact: 0,
            maturity_date,
            certification_status: CertificationStatus::None,
            last_update_timestamp: 0,
            archived: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        env.storage()
            .instance()
            .set(&DataKey::ProjectCounter, &project_id);
        events::project_created(&env, project_id, &creator);

        project_id
    }

    /// Archive a project. Admin-only. Archived projects are excluded from get_all_projects by default (#26).
    #[only_owner]
    pub fn archive_project(env: Env, project_id: u32) {
        require_current_state(&env);
        let mut project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        
        project.archived = true;
        env.storage().persistent().set(&DataKey::Project(project_id), &project);
        events::project_archived(&env, project_id);
    }

    /// Delete a project. Admin-only. Can only delete if no investments exist (#26).
    /// This is a placeholder - actual implementation requires cross-contract call to vault.
    #[only_owner]
    pub fn delete_project(env: Env, project_id: u32) {
        require_current_state(&env);
        // Verify project exists
        let _project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        
        // NOTE: In production, should verify no investments via vault.get_project_investment(project_id)
        // For now, we allow deletion assuming caller has verified no active investments
        
        env.storage().persistent().remove(&DataKey::Project(project_id));
        events::project_deleted(&env, project_id);
    }

    /// Return the `ProjectData` for `id`. Panics with `ProjectNotFound` if the ID is unknown.
    pub fn get_project(env: Env, id: u32) -> ProjectData {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Project(id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound))
    }

    /// Return the current project counter (equals the highest assigned project ID).
    /// Returns 0 when no projects have been created yet.
    pub fn total_projects(env: Env) -> u32 {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0)
    }

    /// Update both `credit_quality` and `green_impact` for `project_id`. Admin-only.
    /// Both values must be in the range 0–100. Emits `ProjectUpdated` and `RateUpdated`.
    /// No-op if the new scores are identical to the existing ones.
    #[only_owner]
    pub fn update_impact_score(env: Env, project_id: u32, credit_quality: u32, green_impact: u32) {
        require_multisig_disabled(&env);
        update_impact_score_internal(env, project_id, credit_quality, green_impact);
    }

    pub fn update_impact_score_approved(
        env: Env,
        project_id: u32,
        credit_quality: u32,
        green_impact: u32,
        approvals: Vec<Address>,
    ) {
        require_admin_approval(&env, approvals);
        update_impact_score_internal(env, project_id, credit_quality, green_impact);
    }

    /// Set the certification status of a project (whitelister or owner only) (#130).
    pub fn certify_project(
        env: Env,
        caller: Address,
        project_id: u32,
        status: CertificationStatus,
    ) {
        caller.require_auth();
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        let owner: Address = stellar_access::ownable::get_owner(&env).unwrap();
        if caller != whitelister && caller != owner {
            panic_with_error!(&env, RegistryError::NotAuthorizedToCertify);
        }
        let mut project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        project.certification_status = status.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        events::project_certified(&env, project_id, status);
    }

    /// Mark a project as settled once its maturity date has passed (#127).
    /// Returns true if the project is mature and was settled, false if already past.
    pub fn is_mature(env: Env, project_id: u32) -> bool {
        require_current_state(&env);
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.maturity_date == 0 {
            return false;
        }
        env.ledger().timestamp() >= project.maturity_date
    }

    /// Return all registered projects as `(project_id, ProjectData)` pairs.
    /// Iterates IDs 1..=`total_projects()` — O(n) in the number of projects.

    pub fn get_all_projects(env: Env) -> Vec<(u32, ProjectData)> {
        require_current_state(&env);
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
                if !project.archived {
                    result.push_back((i, project));
                }
            }
        }
        result
    }

    /// Return all projects including archived ones (#26).
    pub fn get_all_projects_with_archived(env: Env) -> Vec<(u32, ProjectData)> {
        require_current_state(&env);
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
        require_current_state(&env);
        proposer.require_auth();
        if voting_duration_secs < MIN_VOTING_PERIOD {
            panic_with_error!(&env, RegistryError::VotingPeriodTooShort);
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

    /// Cast a vote on an open proposal.
    ///
    /// `weight` is the caller's HBS share balance, which the caller supplies.
    /// **There is no on-chain verification of `weight` against the actual balance.**
    /// Off-chain callers MUST query `vault.balance(voter)` and pass that value;
    /// supplying an inflated weight enables vote stuffing.
    /// `support = true` = vote for; `support = false` = vote against.
    pub fn cast_vote(env: Env, voter: Address, proposal_id: u32, support: bool, weight: i128) {
        require_current_state(&env);
        voter.require_auth();
        if weight <= 0 {
            panic_with_error!(&env, RegistryError::VotingWeightNotPositive);
        }
        let already: bool = env
            .storage()
            .persistent()
            .get(&DataKey::HasVoted(proposal_id, voter.clone()))
            .unwrap_or(false);
        if already {
            panic_with_error!(&env, RegistryError::AlreadyVoted);
        }
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProposalNotFound));
        if env.ledger().timestamp() > proposal.voting_ends_at {
            panic_with_error!(&env, RegistryError::VotingPeriodEnded);
        }
        if proposal.executed {
            panic_with_error!(&env, RegistryError::ProposalAlreadyExecuted);
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
        require_current_state(&env);
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProposalNotFound));
        if env.ledger().timestamp() <= proposal.voting_ends_at {
            panic_with_error!(&env, RegistryError::VotingStillOpen);
        }
        if proposal.executed {
            panic_with_error!(&env, RegistryError::ProposalAlreadyExecuted);
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
    /// Emits `ScoreChanged` with the old and new values. Use `update_impact_score` to
    /// update both scores simultaneously (#6).
    #[only_owner]
    pub fn update_credit_quality_score(env: Env, project_id: u32, credit_quality: u32) {
        if credit_quality > 100 {
            panic_with_error!(&env, RegistryError::CreditQualityOutOfRange);
        }
        let mut project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));

        let old_cq = project.credit_quality;
        let old_rate = compute_rate(project.credit_quality, project.green_impact);
        project.credit_quality = credit_quality;
        project.last_update_timestamp = env.ledger().timestamp();
        let new_rate = compute_rate(credit_quality, project.green_impact);
        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        events::credit_quality_updated(&env, project_id, credit_quality);
        events::score_changed(
            &env,
            project_id,
            old_cq,
            credit_quality,
            project.green_impact,
            project.green_impact,
            old_rate,
            new_rate,
        );
    }

    /// Return a proposal by ID. Panics with `ProposalNotFound` if unknown.
    pub fn get_proposal(env: Env, proposal_id: u32) -> Proposal {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProposalNotFound))
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
        require_current_state(&env);
        depositor.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, RegistryError::CollateralNotPositive);
        }
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.owner != depositor {
            panic_with_error!(&env, RegistryError::NotProjectOwner);
        }

        TokenClient::new(&env, &token).transfer(
            &depositor,
            env.current_contract_address(),
            &amount,
        );

        let key = DataKey::Collateral(project_id, token.clone());
        let prev: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(prev + amount));

        events::collateral_deposited(&env, project_id, &token, &depositor, amount);
    }

    /// Return the collateral balance for (`project_id`, `token`).
    pub fn get_collateral(env: Env, project_id: u32, token: Address) -> i128 {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Collateral(project_id, token))
            .unwrap_or(0)
    }

    /// Release all collateral of `token` back to the project owner.
    /// Allowed only after the project has matured or was never funded.
    pub fn release_collateral(env: Env, project_id: u32, caller: Address, token: Address) {
        require_current_state(&env);
        caller.require_auth();
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.owner != caller {
            panic_with_error!(&env, RegistryError::NotProjectOwner);
        }
        // Collateral can only be released once the project has matured.
        if project.maturity_date > 0 && env.ledger().timestamp() < project.maturity_date {
            panic_with_error!(&env, RegistryError::ProjectNotMature);
        }

        let key = DataKey::Collateral(project_id, token.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance <= 0 {
            panic_with_error!(&env, RegistryError::NoCollateral);
        }

        env.storage().persistent().set(&key, &0i128);
        TokenClient::new(&env, &token).transfer(&env.current_contract_address(), &caller, &balance);

        events::collateral_released(&env, project_id, &token, &caller, balance);
    }

    /// Liquidate collateral to `recipient` (admin only). Used for defaulted projects.
    #[only_owner]
    pub fn liquidate_collateral(env: Env, project_id: u32, token: Address, recipient: Address) {
        require_multisig_disabled(&env);
        liquidate_collateral_internal(env, project_id, token, recipient);
    }

    pub fn liquidate_collateral_approved(
        env: Env,
        project_id: u32,
        token: Address,
        recipient: Address,
        approvals: Vec<Address>,
    ) {
        require_admin_approval(&env, approvals);
        liquidate_collateral_internal(env, project_id, token, recipient);
    }

    #[only_owner]
    pub fn set_multisig_admin(env: Env, signers: Vec<Address>, threshold: u32) {
        validate_multisig_config(&env, &signers, threshold);
        env.storage()
            .instance()
            .set(&DataKey::MultiSigSigners, &signers);
        env.storage()
            .instance()
            .set(&DataKey::MultiSigThreshold, &threshold);
    }

    #[only_owner]
    pub fn clear_multisig_admin(env: Env) {
        env.storage()
            .instance()
            .set(&DataKey::MultiSigThreshold, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::MultiSigSigners, &Vec::<Address>::new(&env));
    }

    pub fn get_multisig_admin(env: Env) -> (Vec<Address>, u32) {
        let signers = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigSigners)
            .unwrap_or_else(|| Vec::new(&env));
        let threshold = env
            .storage()
            .instance()
            .get(&DataKey::MultiSigThreshold)
            .unwrap_or(0);
        (signers, threshold)
    }

    // ── Interest rate (#129) ───────────────────────────────────────────────────

    /// Return the current annualised interest rate in basis points for `project_id`.
    /// Formula: `BASE_RATE_BPS − avg_score × (MAX_DISCOUNT_BPS / 100)`
    /// where `avg_score = (credit_quality + green_impact) / 2` (0–100).
    /// Rate range: 500 bps (5 %) for perfect scores → 1 000 bps (10 %) for zero scores.
    pub fn get_interest_rate(env: Env, project_id: u32) -> u32 {
        require_current_state(&env);
        let project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        compute_rate(project.credit_quality, project.green_impact)
    }

    // ── Creator reputation (#46) ───────────────────────────────────────────────

    /// Set a creator's reputation score (0–100). Callable by the whitelister or owner.
    /// Reputation reflects track record: successful funded projects, repayments, scores.
    /// Emits `ReputationUpdated`.
    pub fn set_creator_reputation(env: Env, caller: Address, creator: Address, score: u32) {
        require_current_state(&env);
        caller.require_auth();
        if score > 100 {
            panic_with_error!(&env, RegistryError::ReputationOutOfRange);
        }
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        let owner: Address = stellar_access::ownable::get_owner(&env).unwrap();
        if caller != whitelister && caller != owner {
            panic_with_error!(&env, RegistryError::NotAuthorizedReputation);
        }
        env.storage()
            .persistent()
            .set(&DataKey::CreatorReputation(creator.clone()), &score);
        events::reputation_updated(&env, &creator, score);
    }

    /// Return the reputation score (0–100) for `creator`. Returns 0 if never set.
    pub fn get_creator_reputation(env: Env, creator: Address) -> u32 {
        require_current_state(&env);
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
        require_current_state(&env);
        let score: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorReputation(creator))
            .unwrap_or(0);
        score * 50
    }

    // ── Dependency injection (#76) ─────────────────────────────────────────────

    /// Replace the whitelister address. Admin-only (#76).
    ///
    /// The new whitelister takes effect immediately for all subsequent `set_whitelist` calls.
    /// Emits `WhitelisterChanged`. The admin key is the security boundary — treat it
    /// with the same care as a multisig threshold key.
    #[only_owner]
    pub fn set_whitelister(env: Env, new_whitelister: Address) {
        require_current_state(&env);
        let old: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        env.storage()
            .instance()
            .set(&DataKey::Whitelister, &new_whitelister);
        events::whitelister_changed(&env, &old, &new_whitelister);
    }

    /// Return the current whitelister address.
    pub fn get_whitelister(env: Env) -> Address {
        require_current_state(&env);
        env.storage().instance().get(&DataKey::Whitelister).unwrap()
    }
}

fn update_impact_score_internal(env: Env, project_id: u32, credit_quality: u32, green_impact: u32) {
    if credit_quality > 100 || green_impact > 100 {
        panic_with_error!(&env, RegistryError::ScoresOutOfRange);
    }
    let mut project: ProjectData = env
        .storage()
        .persistent()
        .get(&DataKey::Project(project_id))
        .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));

    if project.credit_quality == credit_quality && project.green_impact == green_impact {
        return;
    }

    let old_cq = project.credit_quality;
    let old_gi = project.green_impact;
    let old_rate = compute_rate(old_cq, old_gi);

    project.credit_quality = credit_quality;
    project.green_impact = green_impact;
    let new_rate = compute_rate(credit_quality, green_impact);

    env.storage()
        .persistent()
        .set(&DataKey::Project(project_id), &project);
    events::project_updated(&env, project_id, credit_quality, green_impact);
    events::rate_updated(&env, project_id, new_rate);
    events::score_changed(
        &env,
        project_id,
        old_cq,
        credit_quality,
        old_gi,
        green_impact,
        old_rate,
        new_rate,
    );
}

fn update_credit_quality_score_internal(env: Env, project_id: u32, credit_quality: u32) {
    if credit_quality > 100 {
        panic_with_error!(&env, RegistryError::CreditQualityOutOfRange);
    }
    let mut project: ProjectData = env
        .storage()
        .persistent()
        .get(&DataKey::Project(project_id))
        .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
    let old_cq = project.credit_quality;
    let old_rate = compute_rate(project.credit_quality, project.green_impact);
    project.credit_quality = credit_quality;
    let new_rate = compute_rate(credit_quality, project.green_impact);
    env.storage()
        .persistent()
        .set(&DataKey::Project(project_id), &project);
    events::credit_quality_updated(&env, project_id, credit_quality);
    events::score_changed(
        &env,
        project_id,
        old_cq,
        credit_quality,
        project.green_impact,
        project.green_impact,
        old_rate,
        new_rate,
    );
}

fn liquidate_collateral_internal(env: Env, project_id: u32, token: Address, recipient: Address) {
    let key = DataKey::Collateral(project_id, token.clone());
    let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    if balance <= 0 {
        panic_with_error!(&env, RegistryError::NoCollateral);
    }

    env.storage().persistent().set(&key, &0i128);
    TokenClient::new(&env, &token).transfer(&env.current_contract_address(), &recipient, &balance);

    events::collateral_liquidated(&env, project_id, &token, &recipient, balance);
}

fn validate_multisig_config(env: &Env, signers: &Vec<Address>, threshold: u32) {
    if signers.len() > MAX_MULTISIG_SIGNERS {
        panic_with_error!(env, RegistryError::TooManyMultiSigSigners);
    }
    if threshold == 0 || threshold > signers.len() {
        panic_with_error!(env, RegistryError::InvalidMultiSigThreshold);
    }
    for i in 0..signers.len() {
        let signer = signers.get(i).unwrap();
        for j in (i + 1)..signers.len() {
            if signer == signers.get(j).unwrap() {
                panic_with_error!(env, RegistryError::DuplicateApproval);
            }
        }
    }
}

fn require_admin_approval(env: &Env, approvals: Vec<Address>) {
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::MultiSigThreshold)
        .unwrap_or(0);
    if threshold == 0 {
        stellar_access::ownable::get_owner(env)
            .unwrap()
            .require_auth();
        return;
    }

    let signers: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::MultiSigSigners)
        .unwrap_or_else(|| Vec::new(env));
    if threshold > signers.len() {
        panic_with_error!(env, RegistryError::InvalidMultiSigThreshold);
    }

    let mut approved = 0u32;
    for i in 0..approvals.len() {
        let approver = approvals.get(i).unwrap();
        for j in 0..i {
            if approver == approvals.get(j).unwrap() {
                panic_with_error!(env, RegistryError::DuplicateApproval);
            }
        }

        let mut is_signer = false;
        for signer in signers.iter() {
            if approver == signer {
                is_signer = true;
                break;
            }
        }
        if !is_signer {
            panic_with_error!(env, RegistryError::NotMultiSigSigner);
        }
        approver.require_auth();
        approved += 1;
    }

    if approved < threshold {
        panic_with_error!(env, RegistryError::InsufficientApprovals);
    }
}

fn require_multisig_disabled(env: &Env) {
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::MultiSigThreshold)
        .unwrap_or(0);
    if threshold > 0 {
        panic_with_error!(env, RegistryError::InsufficientApprovals);
    }
}

fn compute_rate(credit_quality: u32, green_impact: u32) -> u32 {
    let avg = (credit_quality + green_impact) / 2;
    let discount = avg * MAX_DISCOUNT_BPS / 100;
    BASE_RATE_BPS - discount
}

fn read_state_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::StateVersion)
        .unwrap_or(0)
}

fn require_current_state(env: &Env) {
    let current = read_state_version(env);
    if current != 0 && current != STATE_VERSION {
        panic_with_error!(env, RegistryError::UnsupportedStateVersion);
    }
}

#[contractimpl(contracttrait)]
impl Ownable for ProjectRegistry {}

#[cfg(test)]
mod test;
