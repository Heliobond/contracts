// Fixture: both contracts use the same type for shared fields.
#[contractevent]
pub struct ScoreChanged {
    pub project_id: u32,
    pub old_credit_quality: u32,
    pub new_credit_quality: u32,
}

#[contractevent]
pub struct ManagementFeeSet {
    #[topic]
    pub recipient: Address,
    pub fee_bps: u32,
}
