use soroban_sdk::{Env, Address, String};
use crate::types::{ProjectData, CertificationStatus};

pub fn calculate_interest_rate(
    base_rate_bps: u32,
    max_discount_bps: u32,
    credit_quality: u32,
    green_impact: u32,
) -> u32 {
    let combined_score = (credit_quality + green_impact) / 2;
    let discount = (combined_score * max_discount_bps) / 100;
    
    if discount > base_rate_bps {
        0
    } else {
        base_rate_bps - discount
    }
}
