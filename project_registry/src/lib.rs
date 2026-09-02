#![no_std]
use soroban_sdk::{
    contract, contractimpl, panic_with_error, token::Client as TokenClient, Address, BytesN, Env,
    String, Vec,
};
use stellar_access::ownable::{
    get_owner, set_owner, transfer_ownership as ownable_transfer_ownership, Ownable,
};
use stellar_macros::only_owner;

/// Maximum URI length in bytes. Prevents excessively large ledger entries (#119).
const MAX_URI_LEN: u32 = 512;
/// Minimum URI length — must contain at least a scheme and one character (#117).
const MIN_URI_LEN: u32 = 8;
/// Maximum governance proposal description length in bytes, mirroring the
/// URI-length bound above — prevents excessively large ledger entries (#455).
const MAX_PROPOSAL_DESCRIPTION_LEN: u32 = 2048;
/// Current schema version for instance and persistent contract state (#66).
const STATE_VERSION: u32 = 1;

/// Base interest rate in basis points (10 %). High-risk / zero-score projects pay this rate (#129).
const BASE_RATE_BPS: u32 = 1_000;
/// Maximum rate discount in basis points earned by a perfect-score project (5 %) (#129).
const MAX_DISCOUNT_BPS: u32 = 500;
/// Upper bound for credit-quality and green-impact score inputs (#386).
const MAX_SCORE: u32 = 100;
const MAX_MULTISIG_SIGNERS: u32 = 10;
const MAX_SCORE_HISTORY: u32 = 50;

/// Maximum number of (project_id, score) pairs in a single batch update call (#31).
/// Prevents excessively large transactions that could exceed ledger resource limits.
const MAX_BATCH_SCORE_SIZE: u32 = 20;

/// Maximum number of entries in a single compact_storage call (#332).
/// Mirrors MAX_BATCH_SCORE_SIZE to prevent unbounded O(n*m) iteration.
const MAX_COMPACT_STORAGE_SIZE: u32 = 20;

/// Maximum voting duration in seconds — 30 days (#332).
/// Prevents overflow in voting_ends_at computation and rejects unreasonably long proposals.
const MAX_VOTING_PERIOD: u64 = 30 * 86_400;

mod events;
mod logic;
mod storage;
mod types;

pub use types::{
    ArchiveSummary, CertificationStatus, DataKey, HealthStatus, ProjectData, ProjectStatus,
    Proposal, RegistryError, ScoreHistoryEntry,
};

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
        require_not_paused(&env);
        require_current_state(&env);
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        whitelister.require_auth();
        // Validation: Soroban Address types inherently prevent null/zero addresses,
        // fulfilling explicit validation requirements for account.
        storage::write_whitelist(&env, account.clone(), status);
        events::whitelist_set(&env, &account, status);
    }

    /// Create a new project. `maturity_date` is a Unix timestamp (seconds);
    /// pass 0 to create an open-ended project (#127).
    pub fn create_project(
        env: Env,
        creator: Address,
        uri: String,
        maturity_date: u64,
        metadata_hash: BytesN<32>,
    ) -> u32 {
        require_not_paused(&env);
        require_current_state(&env);
        creator.require_auth();
        // Validation: Soroban Address types inherently prevent null/zero addresses,
        // fulfilling explicit validation requirements for creator.
        let is_whitelisted: bool = storage::read_whitelist(&env, creator.clone());
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
            status: types::ProjectStatus::Pending,
            created_at: env.ledger().timestamp(),
            metadata_hash: metadata_hash.clone(),
        };

        storage::write_project(&env, project_id, &project);
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
        let mut project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));

        if project.status == types::ProjectStatus::Archived {
            panic_with_error!(&env, RegistryError::ProjectArchived);
        }

        project.status = types::ProjectStatus::Archived;
        storage::write_project(&env, project_id, &project);
        events::project_archived(&env, project_id);
    }

    /// Transition a project's lifecycle status to `Active`, `Funded`, or `Completed`. Admin-only (#329).
    ///
    /// Nothing in this contract advances `ProjectData::status` past `Pending` on
    /// its own — this is the callable transition path until a real cross-contract
    /// call from the vault on funding events lands (tracked separately). Cannot be
    /// used to set or clear `Archived`; use `archive_project` for that, since
    /// archived projects are otherwise immutable. Emits `ProjectStatusChanged`.
    #[only_owner]
    pub fn set_project_status(env: Env, project_id: u32, status: types::ProjectStatus) {
        require_not_paused(&env);
        require_current_state(&env);
        let mut project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.status == types::ProjectStatus::Archived
            || status == types::ProjectStatus::Archived
        {
            panic_with_error!(&env, RegistryError::InvalidStatusTransition);
        }
        if project.status == status {
            panic_with_error!(&env, RegistryError::ProjectStatusUnchanged);
        }
        let old_status = project.status.clone();
        project.status = status.clone();
        storage::write_project(&env, project_id, &project);
        events::project_status_changed(&env, project_id, old_status, status);
    }

    /// Delete a project. Admin-only. Can only delete if no investments exist (#26).
    /// This is a placeholder - actual implementation requires cross-contract call to vault.
    #[only_owner]
    pub fn delete_project(env: Env, project_id: u32) {
        require_current_state(&env);
        // Verify project exists
        let _project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));

        // NOTE: In production, should verify no investments via vault.get_project_investment(project_id)
        // For now, we allow deletion assuming caller has verified no active investments

        env.storage()
            .persistent()
            .remove(&DataKey::Project(project_id));
        events::project_deleted(&env, project_id);
    }

    /// Compact an archived project's storage to a minimal `ArchiveSummary` (#73).
    ///
    /// The full `ProjectData` (up to ~580 bytes) is removed and replaced with an
    /// `ArchiveSummary` (~52 bytes), significantly reducing ongoing rent costs.
    /// The project must already be archived via `archive_project`.
    /// After compaction, `get_project(id)` returns `ProjectNotFound`; use
    /// `get_archive_summary(id)` to retrieve the compact historical record.
    #[only_owner]
    pub fn compact_archive(env: Env, project_id: u32) {
        require_current_state(&env);
        let project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.status != types::ProjectStatus::Archived {
            panic_with_error!(&env, RegistryError::ProjectNotArchived);
        }
        let summary = ArchiveSummary {
            owner: project.owner,
            final_credit_quality: project.credit_quality,
            final_green_impact: project.green_impact,
            maturity_date: project.maturity_date,
            certification_status: project.certification_status,
            metadata_hash: project.metadata_hash,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Arch(project_id), &summary);
        env.storage()
            .persistent()
            .remove(&DataKey::Project(project_id));
        events::project_compacted(&env, project_id);
    }

    /// Return the compact archive summary for a compacted project (#73).
    /// Panics with `ProjectNotFound` if no compact record exists for `id`.
    pub fn get_archive_summary(env: Env, project_id: u32) -> ArchiveSummary {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Arch(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound))
    }

    /// Return the `ProjectData` for `id`. Panics with `ProjectNotFound` if the ID is unknown.
    pub fn get_project(env: Env, id: u32) -> ProjectData {
        require_current_state(&env);
        storage::read_project(&env, id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound))
    }

    /// Return true if `candidate_hash` matches the metadata hash recorded for
    /// `project_id` at creation time (#44). Callers hash the metadata content
    /// they fetched from the project's `uri` off-chain and pass it here to get
    /// trustless proof the content matches what the creator committed to.
    pub fn verify_metadata_hash(env: Env, project_id: u32, candidate_hash: BytesN<32>) -> bool {
        require_current_state(&env);
        if let Some(project) = storage::read_project(&env, project_id) {
            return project.metadata_hash == candidate_hash;
        }
        // Project may have been compacted (#73) — fall back to the archive summary,
        // which preserves metadata_hash precisely so this check keeps working (#448).
        let summary: ArchiveSummary = env
            .storage()
            .persistent()
            .get(&DataKey::Arch(project_id))
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        summary.metadata_hash == candidate_hash
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

    /// Return a consolidated operational-status snapshot for monitoring tools (#77).
    ///
    /// Bundles state_version, is_paused, total_projects, and whether an
    /// emergency admin is configured into a single call, so monitoring/alerting
    /// integrations don't need to poll each getter separately.
    pub fn health_check(env: Env) -> HealthStatus {
        let is_paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        let has_emergency_admin: bool = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::EmergencyAdmin)
            .is_some();
        let total_projects: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        HealthStatus {
            state_version: read_state_version(&env),
            is_paused,
            total_projects,
            has_emergency_admin,
        }
    }

    /// Update both `credit_quality` and `green_impact` for `project_id`. Admin-only.
    /// Both values must be in the range 0–100. Emits `ProjectUpdated` and `RateUpdated`.
    /// No-op if the new scores are identical to the existing ones.
    #[only_owner]
    pub fn update_impact_score(env: Env, project_id: u32, credit_quality: u32, green_impact: u32) {
        require_not_paused(&env);
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
        require_not_paused(&env);
        require_admin_approval(&env, approvals);
        update_impact_score_internal(env, project_id, credit_quality, green_impact);
    }

    /// Update impact scores for multiple projects in a single transaction. Admin-only (#31).
    ///
    /// Each entry in `updates` is `(project_id, credit_quality, green_impact)`.
    /// Both score values must be in the range 0–100. Panics with `EmptyBatchUpdate`
    /// if `updates` is empty (#445) — a no-op batch is almost certainly a caller bug.
    /// The batch is rejected as a whole if it exceeds `MAX_BATCH_SCORE_SIZE` (20
    /// entries), preventing transactions that would exceed Soroban ledger resource
    /// limits.
    ///
    /// Individual project entries that are no-ops (scores identical to current values)
    /// are silently skipped, consistent with `update_impact_score`. Emits `ProjectUpdated`,
    /// `RateUpdated`, and `ScoreChanged` for each project whose scores actually change.
    #[only_owner]
    pub fn update_impact_scores_batch(env: Env, updates: Vec<(u32, u32, u32)>) {
        require_not_paused(&env);
        require_multisig_disabled(&env);
        update_impact_scores_batch_internal(env, updates);
    }

    /// Update impact scores for multiple projects using multi-sig admin approvals (#184, #437).
    ///
    /// Mirrors `update_impact_score_approved`/`liquidate_collateral_approved`: this is
    /// the usable batch-update path once multisig is enabled, since
    /// `update_impact_scores_batch` itself is blocked by `require_multisig_disabled`
    /// after `set_multisig_admin` sets a threshold > 0. Same validation as
    /// `update_impact_scores_batch` (empty-batch/size-cap checks, no-op skipping).
    ///
    /// Named `update_scores_batch_approved` rather than
    /// `update_impact_scores_batch_approved` because Soroban caps exported
    /// contract function names at 32 bytes and the longer name exceeds it.
    pub fn update_scores_batch_approved(
        env: Env,
        updates: Vec<(u32, u32, u32)>,
        approvals: Vec<Address>,
    ) {
        require_not_paused(&env);
        require_admin_approval(&env, approvals);
        update_impact_scores_batch_internal(env, updates);
    }

    /// Set the certification status of a project (whitelister or owner only) (#130).
    pub fn certify_project(
        env: Env,
        caller: Address,
        project_id: u32,
        status: CertificationStatus,
    ) {
        require_not_paused(&env);
        require_current_state(&env);
        caller.require_auth();
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        let owner: Address = stellar_access::ownable::get_owner(&env).unwrap();
        if caller != whitelister && caller != owner {
            panic_with_error!(&env, RegistryError::NotAuthorizedToCertify);
        }
        let mut project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.status == types::ProjectStatus::Archived {
            panic_with_error!(&env, RegistryError::ProjectArchived);
        }
        if project.certification_status == status {
            panic_with_error!(&env, RegistryError::AlreadyCertified);
        }
        project.certification_status = status.clone();
        storage::write_project(&env, project_id, &project);
        events::project_certified(&env, project_id, status);
    }

    /// Check whether a project's maturity date has been reached (#127).
    /// Returns true if the current ledger timestamp is on or after the maturity
    /// date, false otherwise. Projects with no maturity date (0) are never mature.
    pub fn is_mature(env: Env, project_id: u32) -> bool {
        require_current_state(&env);
        let project: ProjectData = storage::read_project(&env, project_id)
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
            if let Some(project) = storage::read_project(&env, i) {
                if project.status != types::ProjectStatus::Archived {
                    result.push_back((i, project));
                }
            }
        }
        result
    }

    /// Return up to `limit` non-archived projects starting after the first
    /// `offset` project IDs, ordered by ascending project ID (#269).
    ///
    /// Ordering is stable across calls as long as no projects are created or
    /// archived in between: paging through `offset = 0, limit, 2*limit, ...`
    /// visits every non-archived project exactly once, in the same order
    /// `get_all_projects` would return them.
    pub fn get_projects_page(env: Env, offset: u32, limit: u32) -> Vec<(u32, ProjectData)> {
        require_current_state(&env);
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        let mut result = Vec::new(&env);
        if limit == 0 {
            return result;
        }
        let start = offset.saturating_add(1);
        let end = start.saturating_add(limit).min(counter.saturating_add(1));
        for i in start..end {
            if let Some(project) = storage::read_project(&env, i) {
                if project.status != types::ProjectStatus::Archived {
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
            if let Some(project) = storage::read_project(&env, i) {
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
        require_not_paused(&env);
        require_current_state(&env);
        proposer.require_auth();
        if voting_duration_secs < MIN_VOTING_PERIOD {
            panic_with_error!(&env, RegistryError::VotingPeriodTooShort);
        }
        if voting_duration_secs > MAX_VOTING_PERIOD {
            panic_with_error!(&env, RegistryError::VotingPeriodTooLong);
        }
        if description.len() > MAX_PROPOSAL_DESCRIPTION_LEN {
            panic_with_error!(&env, RegistryError::ProposalDescriptionTooLong);
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
        storage::write_proposal(&env, proposal_id, &proposal);
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
        require_not_paused(&env);
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
        let mut proposal: Proposal = storage::read_proposal(&env, proposal_id)
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
        storage::write_proposal(&env, proposal_id, &proposal);
        env.storage()
            .persistent()
            .set(&DataKey::HasVoted(proposal_id, voter.clone()), &true);
        events::vote_cast(&env, proposal_id, &voter, support, weight);
    }

    /// Finalise a proposal after voting has ended. Anyone may call this.
    /// Returns true if the proposal passed (votes_for > votes_against).
    pub fn execute_proposal(env: Env, proposal_id: u32) -> bool {
        require_current_state(&env);
        let mut proposal: Proposal = storage::read_proposal(&env, proposal_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProposalNotFound));
        if env.ledger().timestamp() <= proposal.voting_ends_at {
            panic_with_error!(&env, RegistryError::VotingStillOpen);
        }
        if proposal.executed {
            panic_with_error!(&env, RegistryError::ProposalAlreadyExecuted);
        }
        proposal.executed = true;
        let passed = proposal.votes_for > proposal.votes_against;
        storage::write_proposal(&env, proposal_id, &proposal);
        events::proposal_executed(&env, proposal_id, passed);
        passed
    }

    /// Set only the credit-quality score for a project. Admin-only, bounded 0–100.
    /// Emits `ScoreChanged` with the old and new values. Use `update_impact_score` to
    /// update both scores simultaneously (#6).
    #[only_owner]
    pub fn update_credit_quality_score(env: Env, project_id: u32, credit_quality: u32) {
        require_not_paused(&env);
        require_multisig_disabled(&env);
        if credit_quality > MAX_SCORE {
            panic_with_error!(&env, RegistryError::CreditQualityOutOfRange);
        }
        let mut project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));

        if project.status == types::ProjectStatus::Archived {
            panic_with_error!(&env, RegistryError::ProjectArchived);
        }

        // Rate-limit oracle updates: reject if too soon since the last update.
        if project.last_update_timestamp > 0
            && env.ledger().timestamp() < project.last_update_timestamp + MIN_UPDATE_INTERVAL
        {
            panic_with_error!(&env, RegistryError::UpdateTooFrequent);
        }

        let old_cq = project.credit_quality;
        if old_cq == credit_quality {
            return;
        }
        let old_rate = compute_rate(project.credit_quality, project.green_impact);
        project.credit_quality = credit_quality;
        project.last_update_timestamp = env.ledger().timestamp();
        let new_rate = compute_rate(credit_quality, project.green_impact);
        storage::write_project(&env, project_id, &project);
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
        append_score_history(&env, project_id, credit_quality, project.green_impact);
    }

    /// Return a proposal by ID. Panics with `ProposalNotFound` if unknown.
    pub fn get_proposal(env: Env, proposal_id: u32) -> Proposal {
        require_current_state(&env);
        storage::read_proposal(&env, proposal_id)
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
        require_not_paused(&env);
        require_current_state(&env);
        depositor.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, RegistryError::CollateralNotPositive);
        }
        let project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.status == types::ProjectStatus::Archived {
            panic_with_error!(&env, RegistryError::ProjectArchived);
        }
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
        require_not_paused(&env);
        require_current_state(&env);
        caller.require_auth();
        let project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        if project.status == types::ProjectStatus::Archived {
            panic_with_error!(&env, RegistryError::ProjectArchived);
        }
        if project.owner != caller {
            panic_with_error!(&env, RegistryError::NotProjectOwner);
        }
        // Collateral can only be released once the project has matured.
        if project.maturity_date > 0 {
            if env.ledger().timestamp() < project.maturity_date {
                panic_with_error!(&env, RegistryError::ProjectNotMature);
            }
        } else {
            if project.status != types::ProjectStatus::Completed
                && project.status != types::ProjectStatus::Pending
            {
                panic_with_error!(&env, RegistryError::ProjectNotMature);
            }
        }

        let key = DataKey::Collateral(project_id, token.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance <= 0 {
            panic_with_error!(&env, RegistryError::NoCollateral);
        }

        env.storage().persistent().remove(&key);
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
        let project: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
        compute_rate(project.credit_quality, project.green_impact)
    }

    // ── Creator reputation (#46) ───────────────────────────────────────────────

    /// Set a creator's reputation score (0–100). Callable by the whitelister or owner.
    /// Reputation reflects track record: successful funded projects, repayments, scores.
    /// Emits `ReputationUpdated`.
    pub fn set_creator_reputation(env: Env, caller: Address, creator: Address, score: u32) {
        require_not_paused(&env);
        require_current_state(&env);
        caller.require_auth();
        if score > MAX_SCORE {
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

    #[only_owner]
    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // ── Circuit breaker (#72) ──────────────────────────────────────────────────

    /// Pause the registry. Admin-only. Blocks all state-mutating user operations.
    /// Getters and admin management functions (upgrade, migrate_state) remain available.
    #[only_owner]
    pub fn pause(env: Env) {
        env.storage().instance().set(&DataKey::Paused, &true);
        events::registry_paused(&env);
    }

    /// Unpause the registry. Admin-only.
    #[only_owner]
    pub fn unpause(env: Env) {
        env.storage().instance().set(&DataKey::Paused, &false);
        events::registry_unpaused(&env);
    }

    /// Return whether the registry is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Set (or clear, via `None`) the emergency-admin address. Owner-only (#43).
    ///
    /// The emergency admin may call `emergency_pause`/`emergency_unpause` without
    /// holding full owner privileges — intended for a fast-response operational
    /// role, separate from the owner who manages whitelisting, scoring, etc.
    #[only_owner]
    pub fn set_emergency_admin(env: Env, emergency_admin: Option<Address>) {
        match &emergency_admin {
            Some(addr) => env.storage().instance().set(&DataKey::EmergencyAdmin, addr),
            None => env.storage().instance().remove(&DataKey::EmergencyAdmin),
        }
        events::emergency_admin_changed(&env, emergency_admin);
    }

    /// Return the configured emergency-admin address, if any.
    pub fn get_emergency_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::EmergencyAdmin)
    }

    /// Pause the registry as the emergency admin, without requiring owner auth (#43).
    pub fn emergency_pause(env: Env, caller: Address) {
        caller.require_auth();
        require_emergency_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Paused, &true);
        events::registry_paused(&env);
    }

    /// Unpause the registry as the emergency admin, without requiring owner auth (#43).
    pub fn emergency_unpause(env: Env, caller: Address) {
        caller.require_auth();
        require_emergency_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Paused, &false);
        events::registry_unpaused(&env);
    }

    // ── Score history (#123) ───────────────────────────────────────────────────

    /// Return the score history for `project_id` in chronological order (oldest first).
    /// Retains at most MAX_SCORE_HISTORY (50) entries via a ring buffer; older entries are
    /// overwritten. Returns an empty vec if no scores have been recorded yet.
    pub fn get_score_history(env: Env, project_id: u32) -> Vec<ScoreHistoryEntry> {
        require_current_state(&env);
        let _: ProjectData = storage::read_project(&env, project_id)
            .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));

        let total: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ScoreHistoryTotal(project_id))
            .unwrap_or(0);

        let count = if total < MAX_SCORE_HISTORY {
            total
        } else {
            MAX_SCORE_HISTORY
        };
        // Oldest slot: when buffer is full it's total%MAX, otherwise it's 0.
        let start_slot = if total >= MAX_SCORE_HISTORY {
            total % MAX_SCORE_HISTORY
        } else {
            0
        };

        let mut result = Vec::new(&env);
        for i in 0..count {
            let slot = (start_slot + i) % MAX_SCORE_HISTORY;
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, ScoreHistoryEntry>(&DataKey::ScoreHistorySlot(project_id, slot))
            {
                result.push_back(entry);
            }
        }
        result
    }

    // ── Storage compaction (#88) ───────────────────────────────────────────────

    /// Remove accumulated zero-value persistent storage entries. Admin-only.
    /// Currently removes collateral keys that were set to zero before the lazy-cleanup
    /// fix (release_collateral and liquidate_collateral now call remove instead of
    /// set-to-zero). Pass the `project_ids` to inspect and a `tokens` list for each.
    /// Returns the number of entries removed.
    #[only_owner]
    pub fn compact_storage(env: Env, project_ids: Vec<u32>, tokens: Vec<Address>) -> u32 {
        require_current_state(&env);
        if project_ids.len() > MAX_COMPACT_STORAGE_SIZE || tokens.len() > MAX_COMPACT_STORAGE_SIZE {
            panic_with_error!(&env, RegistryError::CompactStorageTooLarge);
        }
        let mut removed: u32 = 0;
        for pid in project_ids.iter() {
            for token in tokens.iter() {
                let key = DataKey::Collateral(pid, token.clone());
                if env.storage().persistent().has(&key) {
                    let val: i128 = env.storage().persistent().get(&key).unwrap_or(0);
                    if val == 0 {
                        env.storage().persistent().remove(&key);
                        removed += 1;
                    }
                }
            }
        }
        removed
    }
}

fn update_impact_scores_batch_internal(env: Env, updates: Vec<(u32, u32, u32)>) {
    if updates.is_empty() {
        panic_with_error!(&env, RegistryError::EmptyBatchUpdate);
    }
    if updates.len() > MAX_BATCH_SCORE_SIZE {
        panic_with_error!(&env, RegistryError::BatchTooLarge);
    }
    for entry in updates.iter() {
        let (project_id, credit_quality, green_impact) = entry;
        update_impact_score_internal(env.clone(), project_id, credit_quality, green_impact);
    }
}

fn update_impact_score_internal(env: Env, project_id: u32, credit_quality: u32, green_impact: u32) {
    if credit_quality > MAX_SCORE || green_impact > MAX_SCORE {
        panic_with_error!(&env, RegistryError::ScoresOutOfRange);
    }
    let mut project: ProjectData = storage::read_project(&env, project_id)
        .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));

    if project.status == types::ProjectStatus::Archived {
        panic_with_error!(&env, RegistryError::ProjectArchived);
    }

    // Rate-limit oracle updates: reject if too soon since the last update.
    if project.last_update_timestamp > 0
        && env.ledger().timestamp() < project.last_update_timestamp + MIN_UPDATE_INTERVAL
    {
        panic_with_error!(&env, RegistryError::UpdateTooFrequent);
    }

    if project.credit_quality == credit_quality && project.green_impact == green_impact {
        return;
    }

    let old_cq = project.credit_quality;
    let old_gi = project.green_impact;
    let old_rate = compute_rate(old_cq, old_gi);

    project.credit_quality = credit_quality;
    project.green_impact = green_impact;
    project.last_update_timestamp = env.ledger().timestamp();
    let new_rate = compute_rate(credit_quality, green_impact);

    storage::write_project(&env, project_id, &project);
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
    append_score_history(&env, project_id, credit_quality, green_impact);
}

fn liquidate_collateral_internal(env: Env, project_id: u32, token: Address, recipient: Address) {
    require_not_paused(&env);
    require_current_state(&env);
    let project: ProjectData = storage::read_project(&env, project_id)
        .unwrap_or_else(|| panic_with_error!(&env, RegistryError::ProjectNotFound));
    if project.maturity_date > 0 && env.ledger().timestamp() < project.maturity_date {
        panic_with_error!(&env, RegistryError::ProjectNotMature);
    }
    let key = DataKey::Collateral(project_id, token.clone());
    let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    if balance <= 0 {
        panic_with_error!(&env, RegistryError::NoCollateral);
    }

    env.storage().persistent().remove(&key);
    TokenClient::new(&env, &token).transfer(&env.current_contract_address(), &recipient, &balance);

    events::collateral_liquidated(&env, project_id, &token, &recipient, balance);
}

/// Thin wrapper around the shared `multisig` crate (#459) mapping its
/// generic errors onto this contract's own `RegistryError` codes.
fn validate_multisig_config(env: &Env, signers: &Vec<Address>, threshold: u32) {
    if let Err(e) = multisig::validate_multisig_config(signers, threshold, MAX_MULTISIG_SIGNERS) {
        match e {
            multisig::ConfigError::TooManySigners => {
                panic_with_error!(env, RegistryError::TooManyMultiSigSigners)
            }
            multisig::ConfigError::InvalidThreshold => {
                panic_with_error!(env, RegistryError::InvalidMultiSigThreshold)
            }
            multisig::ConfigError::DuplicateSigner => {
                panic_with_error!(env, RegistryError::DuplicateApproval)
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
    let signers: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::MultiSigSigners)
        .unwrap_or_else(|| Vec::new(env));
    let owner = stellar_access::ownable::get_owner(env).unwrap();
    if let Err(e) = multisig::require_admin_approval(&owner, threshold, &signers, approvals) {
        match e {
            multisig::ApprovalError::InvalidThreshold => {
                panic_with_error!(env, RegistryError::InvalidMultiSigThreshold)
            }
            multisig::ApprovalError::DuplicateApproval => {
                panic_with_error!(env, RegistryError::DuplicateApproval)
            }
            multisig::ApprovalError::NotSigner => {
                panic_with_error!(env, RegistryError::NotMultiSigSigner)
            }
            multisig::ApprovalError::InsufficientApprovals => {
                panic_with_error!(env, RegistryError::InsufficientApprovals)
            }
        }
    }
}

fn require_multisig_disabled(env: &Env) {
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&DataKey::MultiSigThreshold)
        .unwrap_or(0);
    if !multisig::is_multisig_disabled(threshold) {
        panic_with_error!(env, RegistryError::InsufficientApprovals);
    }
}

fn compute_rate(credit_quality: u32, green_impact: u32) -> u32 {
    logic::calculate_interest_rate(
        BASE_RATE_BPS,
        MAX_DISCOUNT_BPS,
        credit_quality,
        green_impact,
    )
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

fn require_not_paused(env: &Env) {
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        panic_with_error!(env, RegistryError::Paused);
    }
}

/// Panics unless `caller` is the configured emergency admin (#43).
fn require_emergency_admin(env: &Env, caller: &Address) {
    let emergency_admin: Option<Address> = env.storage().instance().get(&DataKey::EmergencyAdmin);
    if emergency_admin.as_ref() != Some(caller) {
        panic_with_error!(env, RegistryError::NotEmergencyAdmin);
    }
}

/// Append a score snapshot to the per-project ring buffer (#123).
fn append_score_history(env: &Env, project_id: u32, credit_quality: u32, green_impact: u32) {
    let total: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::ScoreHistoryTotal(project_id))
        .unwrap_or(0);
    let slot = total % MAX_SCORE_HISTORY;
    env.storage().persistent().set(
        &DataKey::ScoreHistorySlot(project_id, slot),
        &ScoreHistoryEntry {
            timestamp: env.ledger().timestamp(),
            credit_quality,
            green_impact,
        },
    );
    env.storage()
        .persistent()
        .set(&DataKey::ScoreHistoryTotal(project_id), &(total + 1));
}

#[contractimpl(contracttrait)]
impl Ownable for ProjectRegistry {
    /// Initiates a 2-step ownership transfer and emits a project-specific
    /// `OwnershipTransferred` event for auditing (#30).
    fn transfer_ownership(e: &Env, new_owner: Address, live_until_ledger: u32) {
        let old_owner = get_owner(e).unwrap_or_else(|| panic!("owner not set"));
        ownable_transfer_ownership(e, &new_owner, live_until_ledger);
        events::ownership_transferred(e, &old_owner, &new_owner);
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod wasm_test;
