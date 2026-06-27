use soroban_sdk::{contracttype, contracterror, Address, String};

/// Structured error codes for the InvestmentVault contract (#75).
/// Variant values are stable — never reorder or renumber after deployment,
/// as on-chain callers may inspect the numeric code.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    /// Deposit or transfer amount must be positive.
    AmountNotPositive       = 1,
    /// Deposit exceeds the per-deposit maximum (MAX_DEPOSIT).
    DepositExceedsMaximum   = 2,
    /// Requested funding exceeds available USDC (liquid minus insurance reserve).
    InsufficientDeployable  = 3,
    /// Shares to burn must be positive.
    SharesNotPositive       = 4,
    /// Requested withdrawal exceeds the utilization-based limit.
    WithdrawalExceedsLimit  = 5,
    /// Insufficient liquid USDC to settle withdrawal immediately.
    InsufficientLiquid      = 6,
    /// Yield amount must be positive.
    YieldAmountNotPositive  = 7,
    /// Cannot distribute yield when no shares are outstanding.
    NoSharesOutstanding     = 8,
    /// Insufficient liquid USDC to pay out yield claim.
    InsufficientLiquidYield = 9,
    /// Insurance has already been claimed for this project.
    InsuranceAlreadyClaimed = 10,
    /// Insurance fund balance is insufficient for the requested claim.
    InsufficientInsurance   = 11,
    /// Management fee exceeds MAX_MANAGEMENT_FEE_BPS.
    FeeExceedsMaximum       = 12,
    /// Share transfers to the vault contract address are not allowed.
    TransferToVaultBlocked  = 13,
    /// Management fee recipient address has not been set.
    FeeRecipientNotSet      = 14,
    /// Expected queue entry is missing from storage.
    QueueEntryMissing       = 15,
    /// Insurance claim amount must be positive.
    ClaimAmountNotPositive  = 16,
    /// Project credit quality is below the configured minimum threshold.
    BelowMinCreditQuality   = 17,
    /// Project green impact is below the configured minimum threshold.
    BelowMinGreenImpact     = 18,
    /// Funding threshold value is out of the 0–100 range.
    ThresholdOutOfRange     = 19,
}

#[contracttype]
pub enum VaultKey {
    UsdcSac,
    Registry,
    TotalInvestments,
    ProjectInvestment(u32),
    /// Global yield-per-share accumulator, scaled by YIELD_SCALE (#125).
    YieldPerShareAccum,
    /// Per-shareholder checkpoint: yield-per-share value at last claim (#125).
    YieldDebt(Address),
    /// Insurance fund USDC balance (#135).
    InsuranceFund,
    /// Whether a project default claim has been paid out (#135).
    InsuranceClaimed(u32),
    /// Lifetime USDC deposited by an investor — used in portfolio analytics (#132).
    TotalDeposited(Address),
    /// Optional management fee in basis points, admin-set, hard-capped (#7).
    ManagementFeeBps,
    /// Recipient address for management fee transfers (#7).
    ManagementFeeRecipient,
    /// Whether secondary market trading of HBS is active (#126).
    TradingEnabled,
    /// Index of the oldest unprocessed redemption queue entry (#3).
    QueueHead,
    /// Next free index in the redemption queue (#3).
    QueueTail,
    /// A queued redemption claim by index (#3).
    QueueEntry(u64),
    /// Admin-set minimum credit quality a project must have before funding (#47).
    MinCreditQuality,
    /// Admin-set minimum green impact a project must have before funding (#47).
    MinGreenImpact,
}

/// Metadata returned for DEX listing and secondary market integration (#126).
#[contracttype]
#[derive(Clone, Debug)]
pub struct HBSTokenInfo {
    /// Human-readable token name.
    pub name: String,
    /// Ticker symbol.
    pub symbol: String,
    /// Number of decimal places (7 for USDC-parity denominations).
    pub decimals: u32,
    /// Whether the admin has enabled secondary trading.
    pub trading_enabled: bool,
}

/// A pending withdrawal claim created when vault liquidity is insufficient (#3).
/// Shares are burned immediately at enqueue; this records the fixed USDC owed.
#[contracttype]
pub struct QueuedClaim {
    /// Address that will receive the USDC when liquidity is available.
    pub from: Address,
    /// USDC amount owed, fixed at the share price when the withdrawal was requested.
    pub usdc_owed: i128,
}

/// On-chain portfolio snapshot for a single investor (#132).
#[contracttype]
pub struct PortfolioInfo {
    /// HBS shares currently held.
    pub shares: i128,
    /// Current USDC redemption value of those shares.
    pub usdc_value: i128,
    /// Unclaimed yield in USDC.
    pub claimable_yield: i128,
    /// Shares as a fraction of total supply, in basis points (0-10 000).
    pub share_of_pool_bps: i128,
    /// Lifetime USDC deposited by this investor.
    pub total_deposited: i128,
}
