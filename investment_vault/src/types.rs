use soroban_sdk::{contracttype, Address};

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
}

#[contracttype]
pub struct CarbonCreditCalculation {
    pub project_id: u32,
    pub amount_invested: i128,
    pub credits: i128,
}
