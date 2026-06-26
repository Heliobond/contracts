use soroban_sdk::contracttype;

#[contracttype]
pub enum VaultKey {
    UsdcSac,
    Registry,
    TotalInvestments,
    ProjectInvestment(u32),
    Bridge,
}
