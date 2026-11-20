// Fixture: same field name with different types across contracts.
#[contractevent]
pub struct ScoreChanged {
    pub project_id: u64,
    pub old_credit_quality: u32,
    pub new_credit_quality: u32,
}
