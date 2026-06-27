use soroban_sdk::{contracttype, Address, String};

/// Certification state for a green project (#130).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum CertificationStatus {
    None      = 0,
    Pending   = 1,
    Certified = 2,
    Revoked   = 3,
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
}
