// Fixture: same field name with different types within one contract.
#[contractevent]
pub struct ManagementFeeSet {
    #[topic]
    pub recipient: Address,
    pub fee_bps: u32,
}

#[contractevent]
pub struct FlashLoanFeeSet {
    pub fee_bps: i128,
}
