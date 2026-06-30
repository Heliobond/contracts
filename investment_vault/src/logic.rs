pub mod logic {
    // Placeholder for investment_vault business logic.
    pub fn calculate_performance_fee(yield_amount: i128, fee_bps: u32) -> i128 {
        (yield_amount * (fee_bps as i128)) / 10000
    }
}
