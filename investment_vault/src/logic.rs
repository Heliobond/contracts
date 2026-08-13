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
    #[allow(dead_code)]
    pub fn calculate_performance_fee(yield_amount: i128, fee_bps: u32) -> i128 {
        (yield_amount * (fee_bps as i128)) / 10000
    }

    /// Determine the effective management fee rate (in basis points) for a deposit.
    ///
    /// When a volume-discount tier is configured (`volume_threshold` and `discounted_bps`
    /// are both `Some`), large deposits at or above the threshold pay the lower
    /// `discounted_bps` rate instead of the flat `base_bps` rate. This implements a
    /// simple two-tier dynamic fee schedule (#39).
    ///
    /// Returns `base_bps` when no tier is active or the deposit is below the threshold.
    pub fn calculate_dynamic_fee_bps(
        deposit_amount: i128,
        base_bps: u32,
        volume_threshold: Option<i128>,
        discounted_bps: Option<u32>,
    ) -> u32 {
        match (volume_threshold, discounted_bps) {
            (Some(threshold), Some(discounted)) if deposit_amount >= threshold => discounted,
            _ => base_bps,
        }
    }
}
