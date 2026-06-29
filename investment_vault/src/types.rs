use soroban_sdk::{contracttype, contracterror, Address, BytesN, String, Vec};

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
    /// Bridge contract address is not set.
    BridgeNotSet            = 20,
    /// Wormhole core contract address is not set.
    WormholeCoreNotSet      = 21,
    /// Emitter is not trusted for cross-chain minting.
    EmitterNotTrusted       = 22,
    /// VAA has already been consumed.
    VaaAlreadyConsumed      = 23,
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
    /// Bridge contract address for cross-chain bridging.
    Bridge,
    /// Flash loan fee in basis points.
    FlashLoanFee,
    /// Carbon credit oracle contract address.
    CarbonOracle,
    /// Carbon credit price in USD micro-units.
    CarbonCreditPrice,
    /// Carbon credit balance per address.
    CarbonCreditBalance(Address),
    /// Compliance event sequence counter.
    ComplianceEventCounter,
    /// A compliance event by sequence number.
    ComplianceEvent(u64),
    /// Latest regulatory reporting snapshot.
    ReportingSnapshot,
    /// Maximum transaction amount for compliance (0 = no limit).
    MaxTransactionAmount,
    /// Cached expected returns value — avoids O(n) iteration on every read (#81).
    /// Updated incrementally on `fund_project`; call `refresh_expected_returns`
    /// to recompute from scratch after impact scores change.
    CachedExpectedReturns,
    /// Cached total_assets value — avoids recomputation on every read (#81).
    /// Updated incrementally on deposit/withdraw/fund/receive_yield/claim.
    /// Call `refresh_total_assets` to recompute from scratch if the cache
    /// may be stale (e.g., after a direct USDC transfer to the vault address).
    CachedTotalAssets,
}

/// Container for wormhole bridge data keys.
#[contracttype]
pub enum BridgeDataKey {
    WormholeCore,
    TrustedEmitter(u32, BytesN<32>),
    ConsumedVaa(BytesN<32>),
}

/// A carbon credit calculation result.
#[contracttype]
pub struct CarbonCreditCalculation {
    pub project_id: u32,
    pub amount_invested: i128,
    pub credits: i128,
}

/// A recorded compliance event for audit trail purposes.
#[contracttype]
pub struct ComplianceEventData {
    pub seq: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub data: String,
}

/// A periodic snapshot of the vault's key metrics for regulatory reporting.
#[contracttype]
pub struct ReportingSnapshotData {
    pub timestamp: u64,
    pub total_assets: i128,
    pub total_supply: i128,
    pub total_investments: i128,
}

/// A comprehensive regulatory data export combining the latest snapshot
/// with recent compliance events.
#[contracttype]
pub struct RegulatoryReport {
    pub snapshot: ReportingSnapshotData,
    pub recent_events: Vec<ComplianceEventData>,
    pub max_transaction_amount: i128,
    pub carbon_credit_price: i128,
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
