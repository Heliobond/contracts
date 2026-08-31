use soroban_sdk::{contracterror, contracttype, Address, BytesN, String};

/// A timestamped snapshot of a project's scores, for on-chain history tracking (#123).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreHistoryEntry {
    pub timestamp: u64,
    pub credit_quality: u32,
    pub green_impact: u32,
}

/// Structured error codes for the ProjectRegistry contract (#75).
/// Variant values are stable — never reorder or renumber after deployment,
/// as on-chain callers may inspect the numeric code.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RegistryError {
    /// Creator address is not in the whitelist.
    NotWhitelisted = 1,
    /// Project URI is shorter than MIN_URI_LEN bytes.
    UriTooShort = 2,
    /// Project URI is longer than MAX_URI_LEN bytes.
    UriTooLong = 3,
    /// Maturity date is not in the future.
    MaturityDateInPast = 4,
    /// Project ID counter reached u32::MAX.
    ProjectLimitReached = 5,
    /// Counter integrity check failed (slot already occupied).
    CounterIntegrityViolation = 6,
    /// Project with the given ID does not exist.
    ProjectNotFound = 7,
    /// Credit quality or green impact score is out of the 0–100 range.
    ScoresOutOfRange = 8,
    /// Caller is not authorised to certify projects.
    NotAuthorizedToCertify = 9,
    /// Voting weight must be positive.
    VotingWeightNotPositive = 10,
    /// Caller has already voted on this proposal.
    AlreadyVoted = 11,
    /// Proposal with the given ID does not exist.
    ProposalNotFound = 12,
    /// Voting period for this proposal has ended.
    VotingPeriodEnded = 13,
    /// Proposal has already been executed.
    ProposalAlreadyExecuted = 14,
    /// Requested voting duration is below MIN_VOTING_PERIOD.
    VotingPeriodTooShort = 15,
    /// Voting is still in progress; cannot execute yet.
    VotingStillOpen = 16,
    /// Collateral amount must be positive.
    CollateralNotPositive = 17,
    /// Only the project owner may perform this operation.
    NotProjectOwner = 18,
    /// No collateral balance to release or liquidate.
    NoCollateral = 19,
    /// Project has not yet reached its maturity date.
    ProjectNotMature = 20,
    /// Reputation score is out of the 0–100 range.
    ReputationOutOfRange = 21,
    /// Caller is not authorised to set creator reputation.
    NotAuthorizedReputation = 22,
    /// Credit quality score is out of the 0–100 range.
    CreditQualityOutOfRange = 23,
    /// Multi-sig threshold must be greater than 0 and no larger than signer count.
    InvalidMultiSigThreshold = 24,
    /// Multi-sig signer set is larger than the contract limit.
    TooManyMultiSigSigners = 25,
    /// Approval address is not configured as a multi-sig signer.
    NotMultiSigSigner = 26,
    /// Approval set contains the same signer more than once.
    DuplicateApproval = 27,
    /// The operation did not include enough multi-sig approvals.
    InsufficientApprovals = 28,
    /// Project URI must start with a valid scheme (ipfs://, https://, ar://).
    InvalidUriScheme = 29,
    /// Project cannot be deleted because it has active investments.
    ProjectHasInvestments = 30,
    /// Project is already archived.
    ProjectArchived = 31,
    /// State version mismatch during migration.
    UnsupportedStateVersion = 32,
    /// Score update requested too soon after previous update.
    UpdateTooFrequent = 33,
    /// Project must be archived before it can be compacted.
    ProjectNotArchived = 34,
    /// Circuit breaker is active (paused).
    Paused = 35,
    /// Caller is neither the owner nor the configured emergency admin.
    NotEmergencyAdmin = 36,
    /// Batch score update request exceeds the configured maximum batch size (#31).
    BatchTooLarge = 37,
    /// Project is already certified with the target status.
    AlreadyCertified = 38,
    /// create_proposal's description exceeds MAX_PROPOSAL_DESCRIPTION_LEN (#455).
    ProposalDescriptionTooLong = 39,
    /// update_impact_scores_batch received an empty updates list (#445).
    EmptyBatchUpdate = 40,
    /// set_project_status was asked to set or clear `Archived` — use
    /// `archive_project` instead, since archived projects are immutable (#329).
    InvalidStatusTransition = 41,
    /// set_project_status was called with the project's current status (#329).
    ProjectStatusUnchanged = 42,
    /// compact_storage input exceeds the maximum allowed batch size (#332).
    CompactStorageTooLarge = 43,
    /// create_proposal voting_duration_secs exceeds the maximum allowed period (#332).
    VotingPeriodTooLong = 44,
}

/// Certification state for a green project (#130).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum CertificationStatus {
    None = 0,
    Pending = 1,
    Certified = 2,
    Revoked = 3,
}

/// Lifecycle status of a project (#27).
///
/// `Pending` is set at creation and `Archived` via `archive_project`.
/// `Active`, `Funded`, and `Completed` are set via `set_project_status`
/// (admin/vault-callable) — this contract does not transition into them on
/// its own; a cross-contract call from the vault on real funding events is
/// tracked separately (#329).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum ProjectStatus {
    Pending = 0,
    Active = 1,
    Funded = 2,
    Completed = 3,
    Archived = 4,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectData {
    pub owner: Address,
    pub uri: String,
    pub credit_quality: u32,
    pub green_impact: u32,
    /// Unix timestamp (seconds) after which the project is considered mature (#127).
    /// 0 means no maturity date set.
    pub maturity_date: u64,
    /// Third-party certification state (#130).
    pub certification_status: CertificationStatus,
    /// Timestamp of the last score update (#70).
    pub last_update_timestamp: u64,
    /// The lifecycle status of the project (#27).
    pub status: ProjectStatus,
    /// Ledger timestamp at project creation (#40).
    ///
    /// NOTE: this field was added after initial deployment. Projects created by
    /// contract builds predating this change will not have a value for it;
    /// any state-migration for existing persisted `ProjectData` records (see
    /// `migrate_state`) must backfill this field, e.g. from `last_update_timestamp`
    /// or 0, since it cannot be reconstructed from on-chain data otherwise.
    pub created_at: u64,
    /// Content hash (e.g. sha256) of the off-chain metadata pointed to by `uri` (#44).
    ///
    /// Lets callers verify metadata hasn't been tampered with by hashing the
    /// fetched content and comparing it against this value (see
    /// `verify_metadata_hash`), without trusting the off-chain host.
    pub metadata_hash: BytesN<32>,
}

/// Compact archive record stored when a project's full data is compacted (#73).
///
/// Replaces `ProjectData` (~580 bytes) with a minimal summary (~52 bytes),
/// reducing persistent storage rent for old projects that no longer need full data.
/// Use `compact_archive` to transition an archived project to this form.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArchiveSummary {
    pub owner: Address,
    pub final_credit_quality: u32,
    pub final_green_impact: u32,
    pub maturity_date: u64,
    pub certification_status: CertificationStatus,
    /// Preserved so `verify_metadata_hash` keeps working after compaction (#448).
    pub metadata_hash: BytesN<32>,
}

/// A governance proposal that HBS holders vote on (#134).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    /// Short human-readable description of the proposal.
    pub description: String,
    /// Address that created the proposal.
    pub proposer: Address,
    /// Ledger timestamp after which no more votes are accepted.
    pub voting_ends_at: u64,
    /// Weighted votes in favour (1 HBS share = 1 vote).
    pub votes_for: i128,
    /// Weighted votes against.
    pub votes_against: i128,
    /// True once the proposal outcome has been finalised.
    pub executed: bool,
}

#[contracttype]
pub enum DataKey {
    StateVersion,
    Whitelister,
    ProjectCounter,
    Project(u32),
    Whitelist(Address),
    /// Auto-incrementing proposal ID counter (#134).
    ProposalCounter,
    /// Proposal storage (#134).
    Proposal(u32),
    /// Whether `address` has voted on proposal `id` (#134).
    HasVoted(u32, Address),
    /// Compact archive summary for a fully compacted project (#73).
    /// Short key name reduces per-entry storage cost.
    Arch(u32),
    /// Collateral balance for (project_id, token) held by this contract (#128).
    Collateral(u32, Address),
    /// Reputation score (0-100) for a project creator (#46).
    CreatorReputation(Address),
    /// Configured multi-sig signer set for critical admin operations (#69).
    MultiSigSigners,
    /// Number of approvals required from MultiSigSigners. 0 disables multi-sig.
    MultiSigThreshold,
    /// Score history ring-buffer slot for (project_id, slot_index) (#123).
    ScoreHistorySlot(u32, u32),
    /// Total score updates ever written for a project (ring-buffer counter) (#123).
    ScoreHistoryTotal(u32),
    /// Circuit breaker pause state (#72).
    Paused,
    /// Optional emergency-admin address that may pause/unpause without
    /// holding full owner privileges (#43). Unset means no emergency admin.
    EmergencyAdmin,
}

/// Consolidated operational status for monitoring/health-check integrations (#77).
///
/// Bundles the getters an off-chain monitor would otherwise have to poll
/// individually into a single call.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct HealthStatus {
    pub state_version: u32,
    pub is_paused: bool,
    pub total_projects: u32,
    pub has_emergency_admin: bool,
}
