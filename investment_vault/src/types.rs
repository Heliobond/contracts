use soroban_sdk::{contracttype, Address, String, Vec};

#[contracttype]
pub enum VaultKey {
    UsdcSac,
    Registry,
    TotalInvestments,
    ProjectInvestment(u32),
    Bridge,
    FlashLoanFee,
    CarbonOracle,
    CarbonCreditPrice,
    CarbonCreditBalance(Address),
    ComplianceEventCounter,
    ComplianceEvent(u64),
    ReportingSnapshot,
    MaxTransactionAmount,
}

#[contracttype]
pub struct CarbonCreditCalculation {
    pub project_id: u32,
    pub amount_invested: i128,
    pub credits: i128,
}

/// A recorded compliance event for audit trail purposes.
#[contracttype]
pub struct ComplianceEventData {
    pub seq: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub data: String,
}

/// A periodic snapshot of the vault's key metrics for regulatory reporting.
#[contracttype]
pub struct ReportingSnapshotData {
    pub timestamp: u64,
    pub total_assets: i128,
    pub total_supply: i128,
    pub total_investments: i128,
}

/// A comprehensive regulatory data export combining the latest snapshot
/// with recent compliance events.
#[contracttype]
pub struct RegulatoryReport {
    pub snapshot: ReportingSnapshotData,
    pub recent_events: Vec<ComplianceEventData>,
    pub max_transaction_amount: i128,
    pub carbon_credit_price: i128,
}
