/// Interest-rate calculation for green-bond projects.
///
/// Heliobond uses a two-dimensional scoring model to determine each project's
/// borrowing cost:
///
/// # Credit Quality vs Green Impact
///
/// | Dimension        | Range  | Purpose |
/// |----------------- |--------|---------|
/// | `credit_quality` | 0–100  | Reflects the borrower's ability to repay — backed by on-chain collateral, creator reputation, and certification status. |
/// | `green_impact`   | 0–100  | Reflects the environmental benefit of the project — verified via oracle data and third-party certification. |
///
/// Both scores are 0–100 unsigned integers and are always set and updated
/// together via `update_impact_score`, or independently via
/// `update_credit_quality_score`.
///
/// # How each score affects the interest rate
///
/// The two scores are averaged into a single 0–100 combined value:
///
/// ```text
/// combined_score = (credit_quality + green_impact) / 2   // integer division
/// ```
///
/// This combined score drives the annualised interest rate via the formula:
///
/// ```text
/// discount = combined_score × MAX_DISCOUNT_BPS / 100
/// rate     = max(BASE_RATE_BPS − discount, 0)
/// ```
///
/// - A project with **both scores at 100** earns the maximum discount
///   (MAX_DISCOUNT_BPS = 500 bps) and pays the minimum rate (500 bps = 5 %).
/// - A project with **both scores at 0** pays the base rate (BASE_RATE_BPS =
///   1 000 bps = 10 %).
/// - A project with credit_quality = 100 and green_impact = 0 (or vice versa)
///   earns half the maximum discount (250 bps) and pays 7.5 %.
///
/// The averaging means neither score alone can drive the rate to its floor —
/// a project must excel at both creditworthiness *and* environmental impact to
/// unlock the lowest borrowing cost.
///
/// # How each score affects the funding limit
///
/// The `get_creator_funding_limit_bps` function uses `creator reputation`
/// (a separate 0–100 score set by the admin/whitelister) to cap the fraction
/// of vault assets a creator's projects may receive. The *credit_quality*
/// score is **not** used to compute the funding limit — that depends only on
/// reputation. However, `credit_quality` indirectly affects funding eligibility
/// through the interest rate: a higher credit quality lowers the rate, making
/// the project more attractive to vault LPs and therefore more likely to
/// reach its funding target.
///
/// The `green_impact` score has no direct effect on funding limits, but
/// projects with higher green impact earn lower interest rates, which in turn
/// makes them more attractive to vault LPs who may choose projects based on
/// both yield and environmental impact.
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
