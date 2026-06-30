use soroban_sdk::{contracterror, contracttype, Address, String};

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
    /// Contract is paused; state-mutating operations are not allowed (#72).
    Paused = 34,
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
    /// Whether the project has been archived (#26).
    pub archived: bool,
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
}
