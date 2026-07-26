pub mod logic {
    /// Calculate the performance/management fee accrued on a given yield or deposit amount.
    ///
    /// # Formula
    /// `fee_amount = (yield_amount * fee_bps) / 10_000`
    ///
    /// - `yield_amount`: The base yield or deposit amount in USDC (7 decimals).
    /// - `fee_bps`: Basis points representing the fee percentage (where 10,000 bps = 100%, 500 bps = 5%).
    ///
    /// Returns the computed fee amount in USDC units.
    pub fn calculate_performance_fee(yield_amount: i128, fee_bps: u32) -> i128 {
        (yield_amount * (fee_bps as i128)) / 10000
    }
}
