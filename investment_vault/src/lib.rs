#![no_std]
//! # InvestmentVault Contract
//!
//! ## Cross-Contract Trust Boundaries (#22)
//!
//! This contract makes cross-contract calls to the ProjectRegistry via the imported WASM interface.
//!
//! ### Trust Assumptions:
//! - The vault trusts the registry to return valid ProjectData with legitimate owner addresses
//! - The vault trusts the registry's total_projects() return value for iteration
//! - A compromised or malicious registry could return manipulated data
//!
//! ### Mitigations:
//! - Registry address is validated at construction via total_projects() call
//! - Registry can only be changed by admin via set_registry() which re-validates
//! - Tests include scenarios for unexpected registry responses (e.g., zero address owner)
//! - Consider using a registry interface trait with known-good implementations
//!
//! ## i128 Arithmetic and Overflow Protection (#25)
//!
//! All financial calculations use i128. Soroban runtime includes overflow checks enabled
//! via `overflow-checks = true` in Cargo.toml profile.release.
//!
//! ### Overflow Behavior:
//! - Arithmetic overflow triggers a panic and transaction revert
//! - Maximum safe deposit: 1 billion USDC (MAX_DEPOSIT constant)
//! - Share calculations use proportional ratios: shares = usdc * total_shares / total_assets
//! - Yield accumulator scaled by 1e18 (YIELD_SCALE) for precision
//!
//! ### Maximum Safe Values:
//! - Single deposit: 1,000,000,000 USDC (1 billion, 7 decimals = 1e16)
//! - Total vault assets: Theoretically up to i128::MAX / 1e18 for yield calculations
//! - In practice, economic limits constrain values well below overflow thresholds
//!
//! ### Critical Path Arithmetic:
//! - deposit(): usdc_amount * total_shares / total_assets (checked)
//! - withdraw(): shares_amount * total_assets / total_shares (checked)
//! - receive_yield(): amount * YIELD_SCALE / total_shares (checked)
//!
use soroban_sdk::{
    contract, contractimpl, panic_with_error, Address, Bytes, BytesN, Env, MuxedAddress, String,
    Vec,
};
use stellar_access::ownable::{
    get_owner, set_owner, transfer_ownership as ownable_transfer_ownership, Ownable,
};
use stellar_macros::only_owner;
use stellar_tokens::fungible::burnable::FungibleBurnable;
use stellar_tokens::fungible::{Base, FungibleToken};

/// Maximum single deposit: 1 billion USDC (7 decimals) — prevents i128 overflow
/// in share calculations and caps single-user concentration risk (#112).
const MAX_DEPOSIT: i128 = 1_000_000_000 * 10_000_000;

/// Minimum deposit amount: 100 USDC (7 decimals) — prevents dust attacks that
/// could manipulate share price via rounding or inflate storage costs (#13).
const MIN_DEPOSIT: i128 = 100_0000000;

/// Minimum withdraw shares amount: 100 shares — prevents dust redemptions that
/// could be used for griefing or disproportionate gas costs (#13).
const MIN_WITHDRAW: i128 = 100_0000000;

/// Scaling factor for the yield-per-share accumulator (#125).
/// Large enough to preserve precision when total_shares >> yield amount.
const YIELD_SCALE: i128 = 1_000_000_000_000_000_000; // 1e18

/// Basis points deducted from each deposit as an insurance premium (#135).
/// 50 bps = 0.5 % of deposit amount.
const INSURANCE_PREMIUM_BPS: i128 = 50;

/// Basis-point denominator (100 bps = 1 whole unit). Used to convert bps
/// fractions into decimal multipliers throughout the vault (#387).
const BPS_SCALE: i128 = 10_000;

/// Upper bound for credit-quality and green-impact score inputs (#386).
const MAX_SCORE: u32 = 100;

/// Maximum total supply of HBS shares (7 decimals) (#20).
///
/// The vault's deposit mechanism already naturally limits supply based on USDC
/// liquidity, but an explicit cap provides a predictable upper bound for
/// integrators and rules out theoretical infinite minting. Set well above
/// MAX_DEPOSIT (a single deposit can mint at most ~MAX_DEPOSIT shares) so
/// ordinary single deposits are never affected by it in practice; it only
/// bites once accumulated supply across many deposits approaches the cap.
const MAX_HBS_SUPPLY: i128 = 10_000_000_000 * 10_000_000; // 10 billion shares
const MAX_MULTISIG_SIGNERS: u32 = 10;
const STATE_VERSION: u32 = 1;

/// Default per-project investment cap: 5 million USDC (7 decimals) (#32).
/// Prevents over-concentration of vault funds in a single project.
/// Overridable per-deployment via `set_max_investment_per_project`.
const MAX_INVESTMENT_PER_PROJECT: i128 = 5_000_000 * 10_000_000;

/// Minimum deposit lock duration in seconds (#33).
/// Depositors cannot withdraw within this window after their last deposit.
/// Set to 1 day (86 400 s) to prevent flash-deposit-withdraw manipulation.
const MIN_LOCK_PERIOD: u64 = 86_400;

/// Seconds in one year, used for time-weighted expected-returns (#34).
const ANNUAL_PERIOD_SECS: i128 = 31_536_000;

mod composability;
mod events;
mod logic;
mod storage;
mod types;
mod wormhole;

mod registry_interface {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/project_registry.wasm");
}

pub use types::{
    CarbonCreditCalculation, ComplianceEventData, HBSTokenInfo, HealthStatus, PortfolioInfo,
    QueuedClaim, RegulatoryReport, ReportingSnapshotData, VaultError, VaultKey,
};
pub use wormhole::{BridgeDataKey, BridgeTransferPayload};

/// Wormhole core contract client interface.
/// In production, replace with `contractimport!` pointing to the
/// deployed Wormhole core contract WASM.
#[soroban_sdk::contractclient(name = "WormholeCoreClient")]
pub trait WormholeCore {
    fn verify_vaa(env: Env, vaa: Bytes) -> wormhole::ParsedVaa;
    fn publish_message(env: Env, consistency_level: u32, payload: Bytes) -> u64;
}

/// Interface for flash loan receiver contracts.
#[soroban_sdk::contractclient(name = "FlashLoanReceiverClient")]
pub trait FlashLoanReceiver {
    fn flash_loan_callback(
        env: Env,
        initiator: Address,
        vault: Address,
        amount: i128,
        fee: i128,
        data: Bytes,
    ) -> bool;
}

/// Hard cap on the management fee to protect investors (#7).
/// 500 bps = 5% maximum.
const MAX_MANAGEMENT_FEE_BPS: u32 = 500;

// ── Graduated withdrawal limits (#45) ─────────────────────────────────────────
/// Utilization tier thresholds (investments / (liquid + investments), in bps).
const UTIL_HIGH_BPS: u32 = 9_000; // 90%
const UTIL_MED_BPS: u32 = 7_000; // 70%
const UTIL_LOW_BPS: u32 = 5_000; // 50%

/// Utilization threshold above which an on-chain warning event is emitted (#45).
const UTIL_WARN_BPS: u32 = UTIL_MED_BPS;

/// Max single-withdrawal as a fraction of liquid USDC at each utilization tier.
const HIGH_TIER_PCT: i128 = 10; // 10% of liquid at ≥ 90% utilization
const MED_TIER_PCT: i128 = 25; // 25% of liquid at ≥ 70% utilization
const LOW_TIER_PCT: i128 = 50; // 50% of liquid at ≥ 50% utilization

/// Max entries accepted by `batch_deposit` in a single call, to prevent excessively
/// large transactions that could exceed Soroban ledger resource limits (#447).
const MAX_BATCH_DEPOSIT_SIZE: u32 = 20;
/// Max entries accepted by `batch_fund_projects` in a single call, for the same
/// reason as `MAX_BATCH_DEPOSIT_SIZE` (#447).
const MAX_BATCH_FUND_SIZE: u32 = 20;

pub const CONTRACT_NAME: &str = "Investment Vault";
pub const CONTRACT_DESCRIPTION: &str = "Heliobond Investment Vault";
pub const CONTRACT_VERSION: &str = "1.0.0";

/// State schema version for this contract build. Increment when a migration is required.

#[contract]
pub struct InvestmentVault;

#[contractimpl]
impl InvestmentVault {
    /// Initialise the vault.
    ///
    /// - `admin` — contract owner; may fund projects, distribute yield, set fees.
    /// - `usdc_sac` — Stellar Asset Contract address for USDC (the vault's accepted asset).
    /// - `registry` — deployed `ProjectRegistry` contract; validated immediately by calling
    ///   `total_projects()`, which panics if the address is not a valid registry.
    pub fn __constructor(env: Env, admin: Address, usdc_sac: Address, registry: Address) {
        set_owner(&env, &admin);
        // Validate that registry is a deployed ProjectRegistry contract by calling it.
        // This panics at construction time if the address is invalid.
        registry_interface::Client::new(&env, &registry).total_projects();
        // Validate that usdc_sac is a valid SAC.
        soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());
        env.storage()
            .instance()
            .set(&VaultKey::StateVersion, &STATE_VERSION);
        env.storage().instance().set(&VaultKey::UsdcSac, &usdc_sac);
        env.storage().instance().set(&VaultKey::Registry, &registry);
        storage::set_persistent(&env, &VaultKey::TotalInvestments, &0i128);
        // CachedTotalAssets lives in instance storage: read on almost every call,
        // auto-bumped with the instance TTL, no separate rent needed (#85).
        env.storage()
            .instance()
            .set(&VaultKey::CachedTotalAssets, &0i128);
        Base::set_metadata(
            &env,
            7,
            String::from_str(&env, "Heliobond Shares"),
            String::from_str(&env, "HBS"),
        );
    }

    /// Return the state schema version supported by this contract build.
    pub fn state_version(_env: Env) -> u32 {
        STATE_VERSION
    }

    /// Return the version recorded in instance storage. Unversioned deployments report 0.
    pub fn stored_state_version(env: Env) -> u32 {
        read_state_version(&env)
    }

    /// Migrate older state to the current schema version.
    ///
    /// Version 0 means a deployment that predates explicit state versioning. The v1
    /// migration only records the version because existing storage layouts are unchanged.
    #[only_owner]
    pub fn migrate_state(env: Env, from_version: u32) -> u32 {
        let current = read_state_version(&env);
        if current != from_version || current > STATE_VERSION {
            panic_with_error!(&env, VaultError::UnsupportedStateVersion);
        }
        if current < STATE_VERSION {
            env.storage()
                .instance()
                .set(&VaultKey::StateVersion, &STATE_VERSION);
        }
        STATE_VERSION
    }

    /// Transfer USDC from the vault to a registered project's owner. Admin-only.
    ///
    /// Rejects funding if the project's `credit_quality` or `green_impact` is below
    /// the admin-configured minimum thresholds (see `set_funding_thresholds`; defaults
    /// are 0 so no restriction applies until explicitly configured).
    ///
    /// The insurance reserve is always protected — only `liquid_usdc - insurance_fund`
    /// is available for deployment.
    ///
    /// USDC is transferred directly to the project `owner` address registered in the
    /// `ProjectRegistry`, not to an arbitrary address.
    #[only_owner]
    pub fn fund_project(env: Env, project_id: u32, amount: i128) {
        require_not_paused(&env);
        require_multisig_disabled(&env);
        fund_project_internal(env, project_id, amount);
    }

    /// Transfer USDC from the vault to a registered project's owner with multi-sig admin approvals (#184).
    pub fn fund_project_with_approvals(
        env: Env,
        project_id: u32,
        amount: i128,
        approvals: Vec<Address>,
    ) {
        require_admin_approval(&env, approvals);
        fund_project_internal(env, project_id, amount);
    }

    /// Fund multiple projects in a single batch transaction with multi-sig approvals (#184, #188).
    ///
    /// Rejects batch requests containing duplicate project IDs to prevent double-funding.
    /// Panics with `EmptyBatchFunding` if `fundings` is empty (#445) — a no-op batch
    /// is almost certainly a caller bug and should not pay `require_admin_approval`'s
    /// cost for nothing.
    /// Panics with `BatchTooLarge` if `fundings` exceeds `MAX_BATCH_FUND_SIZE`
    /// (20 entries), preventing transactions that could exceed Soroban ledger
    /// resource limits (#447).
    pub fn batch_fund_projects(env: Env, fundings: Vec<(u32, i128)>, approvals: Vec<Address>) {
        if fundings.is_empty() {
            panic_with_error!(&env, VaultError::EmptyBatchFunding);
        }
        if fundings.len() > MAX_BATCH_FUND_SIZE {
            panic_with_error!(&env, VaultError::BatchTooLarge);
        }
        require_admin_approval(&env, approvals);
        let mut seen = Vec::new(&env);
        for funding in fundings.iter() {
            if seen.contains(&funding.0) {
                panic_with_error!(&env, VaultError::DuplicateProjectId);
            }
            seen.push_back(funding.0);
            fund_project_internal(env.clone(), funding.0, funding.1);
        }
    }

    /// Return expected returns across all funded projects using a time-weighted formula (#34).
    ///
    /// For each project with an `InvestmentTimestamp` (set when first funded), returns
    /// are accrued proportionally to time elapsed since funding:
    ///   `expected += investment * score_rate * elapsed / annual_period`
    ///
    /// Projects funded before this feature was deployed fall back to the static formula
    /// (`investment * score_rate`) so no previously-funded project loses its contribution.
    pub fn get_expected_returns(env: Env) -> i128 {
        let registry_addr: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
        let registry = registry_interface::Client::new(&env, &registry_addr);
        let total_projects = registry.total_projects();
        let now = env.ledger().timestamp();

        let mut expected: i128 = 0;
        for i in 1..=total_projects {
            let investment: i128 = env
                .storage()
                .persistent()
                .get(&VaultKey::ProjectInvestment(i))
                .unwrap_or(0);
            if investment > 0 {
                let project = registry.get_project(&i);
                let score_rate = project.credit_quality as i128 + project.green_impact as i128;

                let funded_at: u64 = env
                    .storage()
                    .persistent()
                    .get(&VaultKey::InvestmentTimestamp(i))
                    .unwrap_or(0);

                if funded_at > 0 && now > funded_at {
                    // Time-weighted: accrue interest over elapsed time (#34).
                    let elapsed = (now - funded_at) as i128;
                    expected += investment * score_rate * elapsed / (200 * ANNUAL_PERIOD_SECS);
                } else {
                    // Static fallback for pre-existing investments without a timestamp.
                    expected += investment * score_rate / 200;
                }
            }
        }

        expected
    }

    /// Return the vault's net asset value (NAV) by recomputing from on-chain
    /// state on every call (e.g., liquid USDC + investments + expected returns).
    pub fn total_assets(env: Env) -> i128 {
        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());
        let investments: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::TotalInvestments)
            .unwrap_or(0);
        let expected = Self::get_expected_returns(env.clone());
        let total = liquid + investments + expected;

        env.storage()
            .instance()
            .set(&VaultKey::CachedTotalAssets, &total);
        total
    }

    /// Convert a USDC amount to vault shares at the current NAV (ERC-4626 formula).
    /// Returns `usdc_amount` 1:1 when the vault is empty (first deposit).
    pub fn convert_to_shares(env: Env, usdc_amount: i128) -> i128 {
        require_current_state(&env);
        let total_assets = Self::total_assets(env.clone());
        let total_shares = Base::total_supply(&env);
        if total_shares == 0 || total_assets == 0 {
            // 1:1 mint when vault is empty (#111)
            usdc_amount
        } else {
            usdc_amount * total_shares / total_assets
        }
    }

    /// Convert vault shares to a USDC redemption value at the current NAV.
    /// Returns 0 when the vault is empty (no shares outstanding).
    pub fn convert_to_assets(env: Env, shares_amount: i128) -> i128 {
        require_current_state(&env);
        let total_assets = Self::total_assets(env.clone());
        let total_shares = Base::total_supply(&env);
        if total_shares == 0 || total_assets == 0 {
            // No assets to redeem when vault is empty (#111)
            0
        } else {
            shares_amount * total_assets / total_shares
        }
    }

    /// Deposit USDC and mint HBS vault shares. Returns the number of shares minted.
    ///
    /// Deductions applied before share calculation:
    /// 1. Insurance premium: `INSURANCE_PREMIUM_BPS` (50 bps = 0.5%) credited to the insurance fund.
    /// 2. Management fee: optional `ManagementFeeBps` (0–500 bps) sent to the fee recipient.
    ///
    /// The remaining investable amount is converted to shares at the current NAV.
    pub fn deposit(env: Env, from: Address, usdc_amount: i128) -> i128 {
        require_not_paused(&env);
        require_current_state(&env);
        from.require_auth();
        if usdc_amount <= 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }
        if usdc_amount < MIN_DEPOSIT {
            panic_with_error!(&env, VaultError::DepositBelowMinimum);
        }
        if usdc_amount > MAX_DEPOSIT {
            panic_with_error!(&env, VaultError::DepositExceedsMaximum);
        }
        check_max_transaction_amount(&env, usdc_amount);

        // Deduct insurance premium before share calculation (#135)
        let premium = usdc_amount * INSURANCE_PREMIUM_BPS / BPS_SCALE;

        // Deduct optional management fee (#7).
        // Applies a dynamic (volume-tiered) rate when one is configured (#39):
        // deposits >= VolumeTierThreshold use VolumeTierFeeBps; others use the
        // flat ManagementFeeBps rate.
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&VaultKey::ManagementFeeBps)
            .unwrap_or(0);
        let volume_threshold: Option<i128> =
            env.storage().instance().get(&VaultKey::VolumeTierThreshold);
        let volume_tier_bps: Option<u32> =
            env.storage().instance().get(&VaultKey::VolumeTierFeeBps);
        let effective_fee_bps = logic::logic::calculate_dynamic_fee_bps(
            usdc_amount,
            fee_bps,
            volume_threshold,
            volume_tier_bps,
        );
        let fee_amount = usdc_amount * (effective_fee_bps as i128) / BPS_SCALE;

        let investable = usdc_amount - premium - fee_amount;

        let shares = Self::convert_to_shares(env.clone(), investable);

        // Enforce the max HBS supply cap before any transfers (#20).
        let total_shares = Base::total_supply(&env);
        if total_shares + shares > MAX_HBS_SUPPLY {
            panic_with_error!(&env, VaultError::MaxSupplyExceeded);
        }

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let token = soroban_sdk::token::TokenClient::new(&env, &usdc_sac);
        token.transfer(&from, env.current_contract_address(), &usdc_amount);

        // Credit insurance fund with the premium (#135)
        let ins: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::InsuranceFund)
            .unwrap_or(0);
        storage::set_persistent(&env, &VaultKey::InsuranceFund, &(ins + premium));

        // Transfer management fee to recipient if non-zero (#7)
        if fee_amount > 0 {
            let recipient: Address = env
                .storage()
                .instance()
                .get(&VaultKey::ManagementFeeRecipient)
                .unwrap_or_else(|| panic_with_error!(&env, VaultError::FeeRecipientNotSet));
            token.transfer(&env.current_contract_address(), &recipient, &fee_amount);
        }

        // Track lifetime deposits for portfolio analytics (#132)
        let prev_dep: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::TotalDeposited(from.clone()))
            .unwrap_or(0);
        storage::set_persistent(
            &env,
            &VaultKey::TotalDeposited(from.clone()),
            &(prev_dep + usdc_amount),
        );

        // Update cached total assets: liquid increases by full usdc_amount (#81, #85)
        let cached_ta: i128 = env
            .storage()
            .instance()
            .get(&VaultKey::CachedTotalAssets)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&VaultKey::CachedTotalAssets, &(cached_ta + usdc_amount));

        Base::mint(&env, &from, shares);
        lock_deposit(&env, &from);
        events::deposit(&env, &from, usdc_amount, shares);

        shares
    }

    /// Perform multiple deposits from different accounts in a single batch transaction (#184).
    ///
    /// Panics with `EmptyBatchDeposit` if `deposits` is empty (#178) — a no-op
    /// batch is almost certainly a caller bug and should not silently succeed.
    /// Panics with `BatchTooLarge` if `deposits` exceeds `MAX_BATCH_DEPOSIT_SIZE`
    /// (20 entries), preventing transactions that could exceed Soroban ledger
    /// resource limits (#447).
    pub fn batch_deposit(env: Env, deposits: Vec<(Address, i128)>) -> Vec<i128> {
        if deposits.is_empty() {
            panic_with_error!(&env, VaultError::EmptyBatchDeposit);
        }
        if deposits.len() > MAX_BATCH_DEPOSIT_SIZE {
            panic_with_error!(&env, VaultError::BatchTooLarge);
        }
        let mut minted = Vec::new(&env);
        for deposit in deposits.iter() {
            minted.push_back(Self::deposit(env.clone(), deposit.0, deposit.1));
        }
        minted
    }

    /// Return the vault utilization in basis points:
    /// `total_investments * BPS_SCALE / (liquid_usdc + total_investments)`.
    /// Returns 0 when no capital is deployed. Does not call into the registry (#45).
    pub fn get_utilization_bps(env: Env) -> u32 {
        require_current_state(&env);
        let total_investments: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::TotalInvestments)
            .unwrap_or(0);
        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());
        let total_actual = liquid + total_investments;
        if total_actual == 0 {
            return 0;
        }
        (total_investments * BPS_SCALE / total_actual) as u32
    }

    /// Return a consolidated operational-status snapshot for monitoring tools (#77).
    ///
    /// Bundles state_version, is_paused, get_utilization_bps, and whether an
    /// emergency admin is configured into a single call, so monitoring/alerting
    /// integrations don't need to poll each getter separately.
    pub fn health_check(env: Env) -> HealthStatus {
        let is_paused: bool = env
            .storage()
            .instance()
            .get(&VaultKey::Paused)
            .unwrap_or(false);
        let has_emergency_admin: bool = env
            .storage()
            .instance()
            .get::<_, Address>(&VaultKey::EmergencyAdmin)
            .is_some();
        HealthStatus {
            state_version: read_state_version(&env),
            is_paused,
            utilization_bps: Self::get_utilization_bps(env.clone()),
            has_emergency_admin,
        }
    }

    /// Burn `shares_amount` HBS shares and return USDC to `from`.
    ///
    /// Withdrawal is subject to graduated liquidity limits based on vault utilization
    /// (see `get_utilization_bps`). If the vault has insufficient liquid USDC to pay
    /// the full redemption, shares are burned immediately and the claim is enqueued
    /// in FIFO order — call `claim()` once liquidity is restored.
    pub fn withdraw(env: Env, from: Address, shares_amount: i128, min_usdc_return: i128) -> i128 {
        require_not_paused(&env);
        require_current_state(&env);
        check_deposit_lock(&env, &from);
        // Note: from.require_auth() is called inside Base::burn
        if shares_amount <= 0 {
            panic_with_error!(&env, VaultError::SharesNotPositive);
        }
        if shares_amount < MIN_WITHDRAW {
            panic_with_error!(&env, VaultError::WithdrawBelowMinimum);
        }

        let usdc_returned = Self::convert_to_assets(env.clone(), shares_amount);
        check_max_transaction_amount(&env, usdc_returned);

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());

        // Graduated withdrawal limit based on vault utilization (#45).
        // Protects remaining investors from bank-run scenarios when most USDC is deployed.
        let utilization_bps = Self::get_utilization_bps(env.clone());
        let max_withdraw: i128 = if utilization_bps >= UTIL_HIGH_BPS {
            liquid * HIGH_TIER_PCT / 100
        } else if utilization_bps >= UTIL_MED_BPS {
            liquid * MED_TIER_PCT / 100
        } else if utilization_bps >= UTIL_LOW_BPS {
            liquid * LOW_TIER_PCT / 100
        } else {
            i128::MAX
        };
        if utilization_bps >= UTIL_WARN_BPS {
            events::utilization_warning(&env, utilization_bps);
        }
        if usdc_returned > max_withdraw {
            panic_with_error!(&env, VaultError::WithdrawalExceedsLimit);
        }
        if usdc_returned < min_usdc_return {
            panic_with_error!(&env, VaultError::SlippageLimitExceeded);
        }

        if usdc_returned > liquid {
            // Insufficient liquidity: burn shares immediately (locking in the current USDC
            // value) and enqueue a FIFO claim. call claim() once liquidity is restored.
            Base::burn(&env, &from, shares_amount);
            let tail: u64 = env
                .storage()
                .persistent()
                .get(&VaultKey::QueueTail)
                .unwrap_or(0);
            storage::set_persistent(
                &env,
                &VaultKey::QueueEntry(tail),
                &QueuedClaim {
                    from: from.clone(),
                    usdc_owed: usdc_returned,
                },
            );
            storage::set_persistent(&env, &VaultKey::QueueTail, &(tail + 1));
            events::withdraw_queued(&env, &from, shares_amount, usdc_returned);
            return 0;
        }

        Base::burn(&env, &from, shares_amount);

        // Update cached total assets: liquid decreases by usdc_returned (#81, #85)
        let cached_ta: i128 = env
            .storage()
            .instance()
            .get(&VaultKey::CachedTotalAssets)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&VaultKey::CachedTotalAssets, &(cached_ta - usdc_returned));

        soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
            &env.current_contract_address(),
            &from,
            &usdc_returned,
        );

        events::withdraw(&env, &from, shares_amount, usdc_returned);
        usdc_returned
    }

    /// Settle queued redemptions in FIFO order using available liquid USDC (#3).
    ///
    /// Stops at the head entry if it cannot be fully satisfied — available liquidity
    /// is NOT used to pay out later, smaller entries. This preserves strict ordering
    /// so no claimant can be skipped ahead of an earlier one.
    ///
    /// Anyone may call this function; no auth required.
    pub fn claim(env: Env) -> i128 {
        require_current_state(&env);
        let head: u64 = env
            .storage()
            .persistent()
            .get(&VaultKey::QueueHead)
            .unwrap_or(0);
        let tail: u64 = env
            .storage()
            .persistent()
            .get(&VaultKey::QueueTail)
            .unwrap_or(0);

        if head == tail {
            return 0; // queue is empty
        }

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let mut liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());
        let mut total_paid: i128 = 0;
        let mut idx = head;

        while idx < tail && liquid > 0 {
            let entry: QueuedClaim = env
                .storage()
                .persistent()
                .get(&VaultKey::QueueEntry(idx))
                .unwrap_or_else(|| panic_with_error!(&env, VaultError::QueueEntryMissing));

            if entry.usdc_owed > liquid {
                break; // can't fully satisfy this entry yet; preserve FIFO order
            }

            // CEI: remove from storage before the external transfer
            env.storage()
                .persistent()
                .remove(&VaultKey::QueueEntry(idx));
            liquid -= entry.usdc_owed;
            total_paid += entry.usdc_owed;
            idx += 1;

            soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
                &env.current_contract_address(),
                &entry.from,
                &entry.usdc_owed,
            );
            events::withdraw_claimed(&env, &entry.from, entry.usdc_owed, idx - 1);
        }

        if idx != head {
            storage::set_persistent(&env, &VaultKey::QueueHead, &idx);
        }

        // Update cached total assets: liquid decreased by total_paid (#81, #85)
        if total_paid > 0 {
            let cached_ta: i128 = env
                .storage()
                .instance()
                .get(&VaultKey::CachedTotalAssets)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&VaultKey::CachedTotalAssets, &(cached_ta - total_paid));
        }

        total_paid
    }

    // ── Yield distribution (#125) ──────────────────────────────────────────────

    /// Deposit USDC yield into the vault and update the per-share accumulator.
    /// Called by the owner when a project makes a repayment.
    #[only_owner]
    pub fn receive_yield(env: Env, from: Address, amount: i128) {
        require_multisig_disabled(&env);
        receive_yield_internal(env, from, amount);
    }

    /// Deposit USDC yield into the vault using multi-sig admin approvals (#184, #436).
    ///
    /// Mirrors `fund_project_with_approvals`/`claim_insurance_with_approvals`: this is
    /// the only usable path into `receive_yield_internal` once multisig is enabled,
    /// since `receive_yield` itself is permanently blocked by `require_multisig_disabled`
    /// after `set_multisig_admin` sets a threshold > 0.
    pub fn receive_yield_with_approvals(
        env: Env,
        from: Address,
        amount: i128,
        approvals: Vec<Address>,
    ) {
        require_admin_approval(&env, approvals);
        receive_yield_internal(env, from, amount);
    }

    /// Return the USDC yield claimable by `account` without modifying state.
    pub fn claimable_yield(env: Env, account: Address) -> i128 {
        require_current_state(&env);
        let accum: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::YieldPerShareAccum)
            .unwrap_or(0);
        let debt: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::YieldDebt(account.clone()))
            .unwrap_or(0);
        let shares = Base::balance(&env, &account);
        shares * (accum - debt) / YIELD_SCALE
    }

    /// Claim accumulated yield for `from`. Transfers claimable USDC to `from`.
    pub fn claim_yield(env: Env, from: Address) -> i128 {
        require_current_state(&env);
        from.require_auth();
        let accum: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::YieldPerShareAccum)
            .unwrap_or(0);
        let debt: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::YieldDebt(from.clone()))
            .unwrap_or(0);
        let shares = Base::balance(&env, &from);
        let claimable = shares * (accum - debt) / YIELD_SCALE;

        if claimable <= 0 {
            return 0;
        }

        // Update debt checkpoint before transfer (CEI)
        storage::set_persistent(&env, &VaultKey::YieldDebt(from.clone()), &accum);

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());
        if claimable > liquid {
            panic_with_error!(&env, VaultError::InsufficientLiquidYield);
        }

        soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
            &env.current_contract_address(),
            &from,
            &claimable,
        );

        // Update cached total assets: liquid decreases by claimable (#81, #85)
        let cached_ta: i128 = env
            .storage()
            .instance()
            .get(&VaultKey::CachedTotalAssets)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&VaultKey::CachedTotalAssets, &(cached_ta - claimable));

        events::yield_claimed(&env, &from, claimable);
        claimable
    }

    // ── Portfolio analytics (#132) ─────────────────────────────────────────────

    /// Return a full on-chain portfolio snapshot for `account`.
    pub fn get_portfolio(env: Env, account: Address) -> PortfolioInfo {
        require_current_state(&env);
        let shares = Base::balance(&env, &account);
        let total_shares = Base::total_supply(&env);
        let usdc_value = Self::convert_to_assets(env.clone(), shares);

        let accum: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::YieldPerShareAccum)
            .unwrap_or(0);
        let debt: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::YieldDebt(account.clone()))
            .unwrap_or(0);
        let claimable_yield = shares * (accum - debt) / YIELD_SCALE;

        let share_of_pool_bps = if total_shares == 0 {
            0
        } else {
            shares * BPS_SCALE / total_shares
        };

        let total_deposited: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::TotalDeposited(account))
            .unwrap_or(0);

        PortfolioInfo {
            shares,
            usdc_value,
            claimable_yield,
            share_of_pool_bps,
            total_deposited,
        }
    }

    // ── Insurance fund (#135) ──────────────────────────────────────────────────

    /// Return the current insurance fund USDC balance.
    pub fn insurance_fund_balance(env: Env) -> i128 {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&VaultKey::InsuranceFund)
            .unwrap_or(0)
    }

    /// Pay out an insurance claim for a defaulted project (owner only).
    /// Transfers `amount` from the insurance fund to `recipient`.
    #[only_owner]
    pub fn claim_insurance(env: Env, project_id: u32, recipient: Address, amount: i128) {
        require_multisig_disabled(&env);
        claim_insurance_internal(env, project_id, recipient, amount);
    }

    /// Pay out an insurance claim for a defaulted project using multi-sig approvals (#184).
    pub fn claim_insurance_with_approvals(
        env: Env,
        project_id: u32,
        recipient: Address,
        amount: i128,
        approvals: Vec<Address>,
    ) {
        require_admin_approval(&env, approvals);
        claim_insurance_internal(env, project_id, recipient, amount);
    }

    /// Configure multi-sig admin signers and approval threshold (owner-only) (#184).
    #[only_owner]
    pub fn set_multisig_admin(env: Env, signers: Vec<Address>, threshold: u32) {
        require_not_paused(&env);
        validate_multisig_config(&env, &signers, threshold);
        env.storage()
            .instance()
            .set(&VaultKey::MultiSigSigners, &signers);
        env.storage()
            .instance()
            .set(&VaultKey::MultiSigThreshold, &threshold);
    }

    /// Return the list of multi-sig admin signers and required threshold (#184).
    pub fn get_multisig_admin(env: Env) -> (Vec<Address>, u32) {
        let signers = env
            .storage()
            .instance()
            .get(&VaultKey::MultiSigSigners)
            .unwrap_or_else(|| Vec::new(&env));
        let threshold = env
            .storage()
            .instance()
            .get(&VaultKey::MultiSigThreshold)
            .unwrap_or(0);
        (signers, threshold)
    }

    // ── Multi-asset configuration (#133) ──────────────────────────────────────

    /// Return the primary accepted asset (USDC SAC address).
    /// Multi-asset vaults should extend this by adding accepted_assets to config.
    pub fn accepted_asset(env: Env) -> Address {
        require_current_state(&env);
        env.storage().instance().get(&VaultKey::UsdcSac).unwrap()
    }

    // ── Management fee (#7) ───────────────────────────────────────────────────

    /// Set the optional management fee deducted from each deposit.
    /// `fee_bps` is bounded by MAX_MANAGEMENT_FEE_BPS (500 = 5%).
    /// Pass `fee_bps = 0` to disable the fee entirely.
    #[only_owner]
    pub fn set_management_fee(env: Env, fee_bps: u32, recipient: Address) {
        require_not_paused(&env);
        require_current_state(&env);
        if fee_bps > MAX_MANAGEMENT_FEE_BPS {
            panic_with_error!(&env, VaultError::FeeExceedsMaximum);
        }
        let current_fee: u32 = env
            .storage()
            .instance()
            .get(&VaultKey::ManagementFeeBps)
            .unwrap_or(0);
        let current_recipient: Option<Address> = env
            .storage()
            .instance()
            .get(&VaultKey::ManagementFeeRecipient);
        if current_fee == fee_bps && current_recipient == Some(recipient.clone()) {
            return;
        }
        env.storage()
            .instance()
            .set(&VaultKey::ManagementFeeBps, &fee_bps);
        env.storage()
            .instance()
            .set(&VaultKey::ManagementFeeRecipient, &recipient);
        events::management_fee_set(&env, &recipient, fee_bps);
    }

    /// Return the current management fee in basis points (0 = disabled).
    pub fn get_management_fee_bps(env: Env) -> u32 {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&VaultKey::ManagementFeeBps)
            .unwrap_or(0)
    }

    // ── Secondary market trading (#126) ──────────────────────────────────────

    /// Enable secondary market trading for HBS shares. Admin-only.
    /// Once enabled, the flag is readable by external DEX integrations via
    /// `is_trading_enabled`. HBS is natively SEP-41 tradeable on Stellar DEX;
    /// this flag signals to UIs and aggregators that the token is officially listed.
    #[only_owner]
    pub fn enable_secondary_trading(env: Env) {
        require_current_state(&env);
        let enabled: bool = env
            .storage()
            .instance()
            .get(&VaultKey::TradingEnabled)
            .unwrap_or(false);
        if enabled {
            return;
        }
        env.storage()
            .instance()
            .set(&VaultKey::TradingEnabled, &true);
        events::trading_enabled(&env, true);
    }

    /// Return whether the admin has enabled secondary market trading for HBS.
    pub fn is_trading_enabled(env: Env) -> bool {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&VaultKey::TradingEnabled)
            .unwrap_or(false)
    }

    // ── Minimum funding thresholds (#47) ──────────────────────────────────────

    /// Set the minimum score thresholds a project must meet before it can be funded.
    ///
    /// Both values must be 0–100. The default is 0 (no restriction), which preserves
    /// backwards compatibility until the admin explicitly raises the bar.
    /// Emits `FundingThresholdsSet`. Admin-only.
    #[only_owner]
    pub fn set_funding_thresholds(env: Env, min_credit_quality: u32, min_green_impact: u32) {
        require_not_paused(&env);
        require_current_state(&env);
        if min_credit_quality > MAX_SCORE || min_green_impact > MAX_SCORE {
            panic_with_error!(&env, VaultError::ThresholdOutOfRange);
        }
        env.storage()
            .instance()
            .set(&VaultKey::MinCreditQuality, &min_credit_quality);
        env.storage()
            .instance()
            .set(&VaultKey::MinGreenImpact, &min_green_impact);
        events::funding_thresholds_set(&env, min_credit_quality, min_green_impact);
    }

    /// Return the minimum credit quality threshold (0–100). Default is 0 (no restriction).
    pub fn get_min_credit_quality(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&VaultKey::MinCreditQuality)
            .unwrap_or(0)
    }

    /// Return the minimum green impact threshold (0–100). Default is 0 (no restriction).
    pub fn get_min_green_impact(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&VaultKey::MinGreenImpact)
            .unwrap_or(0)
    }

    // ── Dependency injection (#76) ─────────────────────────────────────────────

    /// Replace the ProjectRegistry dependency. Admin-only (#76).
    ///
    /// The new address is validated immediately by calling `total_projects()` on it —
    /// panics if the address is not a deployed ProjectRegistry.
    ///
    /// **Security:** This is a high-privilege operation. The admin key is the only
    /// protection against swapping in a malicious registry. Treat the admin key as a
    /// security boundary (ideally a multisig account).
    ///
    /// Emits `RegistryChanged`.
    #[only_owner]
    pub fn set_registry(env: Env, new_registry: Address) {
        require_not_paused(&env);
        require_current_state(&env);
        // Validate that the new address is a deployed ProjectRegistry by calling it.
        // Panics at call time if the address is not a valid registry contract.
        registry_interface::Client::new(&env, &new_registry).total_projects();
        let old: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
        env.storage()
            .instance()
            .set(&VaultKey::Registry, &new_registry);
        events::registry_changed(&env, &old, &new_registry);
    }

    /// Return the current ProjectRegistry contract address.
    pub fn get_registry(env: Env) -> Address {
        require_current_state(&env);
        env.storage().instance().get(&VaultKey::Registry).unwrap()
    }

    /// Return HBS token metadata for DEX listing and secondary market integration.
    /// The `trading_enabled` field mirrors `is_trading_enabled()`.
    pub fn get_hbs_token_info(env: Env) -> HBSTokenInfo {
        require_current_state(&env);
        let trading_enabled: bool = env
            .storage()
            .instance()
            .get(&VaultKey::TradingEnabled)
            .unwrap_or(false);
        HBSTokenInfo {
            name: String::from_str(&env, "Heliobond Shares"),
            symbol: String::from_str(&env, "HBS"),
            decimals: 7,
            trading_enabled,
        }
    }

    /// Return the maximum total HBS share supply this contract build will ever mint (#20).
    pub fn max_hbs_supply(_env: Env) -> i128 {
        MAX_HBS_SUPPLY
    }

    /// Return the total USDC amount currently invested in `project_id` from this vault.
    pub fn get_project_investment(env: Env, project_id: u32) -> i128 {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&VaultKey::ProjectInvestment(project_id))
            .unwrap_or(0)
    }

    /// Return USDC investment amounts for a list of project IDs in one call (#35).
    ///
    /// Results are returned in the same order as `project_ids`. Unknown or
    /// unfunded projects return 0. Callers can map over the registry's
    /// `total_projects()` count and call this once instead of issuing one
    /// cross-contract call per project.
    pub fn get_project_investments_batch(env: Env, project_ids: Vec<u32>) -> Vec<i128> {
        require_current_state(&env);
        let mut results = Vec::new(&env);
        for id in project_ids.iter() {
            let amount: i128 = env
                .storage()
                .persistent()
                .get(&VaultKey::ProjectInvestment(id))
                .unwrap_or(0);
            results.push_back(amount);
        }
        results
    }

    /// Return USDC investment amounts for all projects 1..=`total_projects` (#35).
    ///
    /// Each tuple is `(project_id, invested_usdc)`. Projects with no recorded
    /// investment are included with a value of 0 so the caller can always
    /// correlate by position.
    pub fn get_all_project_investments(env: Env) -> Vec<(u32, i128)> {
        require_current_state(&env);
        let registry_addr: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
        let total = registry_interface::Client::new(&env, &registry_addr).total_projects();
        let mut results = Vec::new(&env);
        for id in 1..=total {
            let amount: i128 = env
                .storage()
                .persistent()
                .get(&VaultKey::ProjectInvestment(id))
                .unwrap_or(0);
            results.push_back((id, amount));
        }
        results
    }

    // ── Withdrawal sliding window (#36) ───────────────────────────────────────

    /// Configure the minimum number of ledgers that must pass after a deposit
    /// before the same address may withdraw (#36).
    ///
    /// `ledgers = 1` (the default) retains the existing behaviour: a deposit and
    /// a withdrawal cannot appear in the same ledger. Larger values enforce a
    /// longer cooldown — e.g., `ledgers = 720` (≈1 hour at 5 s/ledger) prevents
    /// instant-exit strategies in volatile markets.
    /// `ledgers = 0` disables the window entirely (not recommended).
    #[only_owner]
    pub fn set_withdrawal_window(env: Env, ledgers: u32) {
        require_not_paused(&env);
        require_current_state(&env);
        env.storage()
            .instance()
            .set(&VaultKey::WithdrawalWindowLedgers, &ledgers);
        events::withdrawal_window_set(&env, ledgers);
    }

    /// Return the currently configured withdrawal window in ledgers (#36).
    /// Returns 1 when no explicit window has been set (same-ledger protection).
    pub fn get_withdrawal_window(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&VaultKey::WithdrawalWindowLedgers)
            .unwrap_or(1)
    }

    // ── Dynamic fee structure (#39) ───────────────────────────────────────────

    /// Configure a two-tier volume-discount fee schedule for deposits (#39).
    ///
    /// When a deposit amount meets or exceeds `threshold`, the effective management
    /// fee rate is `discounted_bps` instead of the flat `ManagementFeeBps`. Pass
    /// `threshold = 0` to disable the tier (reverts to flat rate for all deposits).
    ///
    /// Constraints:
    /// - `discounted_bps` must not exceed the current `ManagementFeeBps` (discounts only).
    /// - `discounted_bps` must not exceed `MAX_MANAGEMENT_FEE_BPS`.
    ///
    /// Admin-only.
    #[only_owner]
    pub fn set_volume_fee_tier(env: Env, threshold: i128, discounted_bps: u32) {
        require_not_paused(&env);
        require_current_state(&env);
        if discounted_bps > MAX_MANAGEMENT_FEE_BPS {
            panic_with_error!(&env, VaultError::FeeExceedsMaximum);
        }
        if threshold == 0 {
            // Disable the tier entirely.
            env.storage()
                .instance()
                .remove(&VaultKey::VolumeTierThreshold);
            env.storage().instance().remove(&VaultKey::VolumeTierFeeBps);
            return;
        }
        env.storage()
            .instance()
            .set(&VaultKey::VolumeTierThreshold, &threshold);
        env.storage()
            .instance()
            .set(&VaultKey::VolumeTierFeeBps, &discounted_bps);
    }

    /// Return the configured volume-discount tier as `(threshold, discounted_bps)`.
    /// Returns `(0, 0)` when no tier is active.
    pub fn get_volume_fee_tier(env: Env) -> (i128, u32) {
        require_current_state(&env);
        let threshold: i128 = env
            .storage()
            .instance()
            .get(&VaultKey::VolumeTierThreshold)
            .unwrap_or(0);
        let bps: u32 = env
            .storage()
            .instance()
            .get(&VaultKey::VolumeTierFeeBps)
            .unwrap_or(0);
        (threshold, bps)
    }

    // ── Per-project investment cap (#32) ──────────────────────────────────────

    /// Set the maximum total USDC the vault may invest in any single project. Admin-only.
    ///
    /// Defaults to `MAX_INVESTMENT_PER_PROJECT` (5 M USDC) until explicitly changed.
    /// Pass 0 to restore the compile-time default.
    #[only_owner]
    pub fn set_max_investment_per_project(env: Env, cap: i128) {
        require_not_paused(&env);
        require_current_state(&env);
        if cap < 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }
        let stored_cap = if cap == 0 {
            MAX_INVESTMENT_PER_PROJECT
        } else {
            cap
        };
        env.storage()
            .instance()
            .set(&VaultKey::MaxInvestmentPerProject, &stored_cap);
        events::investment_cap_set(&env, stored_cap);
    }

    /// Return the remaining USDC capacity that may still be invested in `project_id`.
    ///
    /// A return value of 0 means the cap is already reached; further funding calls will
    /// fail with `InvestmentCapExceeded`.
    pub fn investment_capacity(env: Env, project_id: u32) -> i128 {
        require_current_state(&env);
        let cap: i128 = env
            .storage()
            .instance()
            .get(&VaultKey::MaxInvestmentPerProject)
            .unwrap_or(MAX_INVESTMENT_PER_PROJECT);
        let invested: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::ProjectInvestment(project_id))
            .unwrap_or(0);
        let remaining = cap - invested;
        if remaining < 0 {
            0
        } else {
            remaining
        }
    }

    // ── Deposit lock-up expiry query (#33) ────────────────────────────────────

    /// Return the Unix timestamp (seconds) at which `account`'s deposit lock expires.
    ///
    /// Returns 0 if `account` has never deposited, meaning no lock is in force.
    /// If the returned timestamp is in the future, `withdraw` will reject the call
    /// with `DepositLocked`.
    pub fn get_deposit_lock_expiry(env: Env, account: Address) -> u64 {
        let deposited_at: u64 = env
            .storage()
            .persistent()
            .get(&VaultKey::LastDeposit(account))
            .unwrap_or(0);
        if deposited_at == 0 {
            0
        } else {
            deposited_at + MIN_LOCK_PERIOD
        }
    }

    // ── Bridge ────────────────────────────────────────────────────────────────

    /// Set the cross-chain bridge contract address (owner-only) (#184).
    #[only_owner]
    pub fn set_bridge(env: Env, bridge: Address) {
        require_not_paused(&env);
        require_current_state(&env);
        let current: Option<Address> = env.storage().instance().get(&VaultKey::Bridge);
        if current == Some(bridge.clone()) {
            return;
        }
        env.storage().instance().set(&VaultKey::Bridge, &bridge);
        events::bridge_set(&env, &bridge);
    }

    /// Mint HBS shares resulting from an authorized cross-chain bridge transfer (#184).
    pub fn bridge_mint(env: Env, to: Address, amount: i128) {
        require_current_state(&env);
        let bridge: Address = env
            .storage()
            .instance()
            .get(&VaultKey::Bridge)
            .unwrap_or_else(|| panic_with_error!(&env, VaultError::BridgeNotSet));
        bridge.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }
        if Base::total_supply(&env) + amount > MAX_HBS_SUPPLY {
            panic_with_error!(&env, VaultError::MaxSupplyExceeded);
        }
        Base::mint(&env, &to, amount);
        lock_deposit(&env, &to);
        events::bridge_mint(&env, &to, amount);
    }

    /// Burn HBS shares to initiate an outbound cross-chain bridge transfer (#184).
    pub fn bridge_burn(env: Env, from: Address, amount: i128) {
        require_current_state(&env);
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }
        Base::burn(&env, &from, amount);
        events::bridge_burn(&env, &from, amount);
    }

    // ── Wormhole bridge ────────────────────────────────────────────────────────

    /// Set the Wormhole core contract address (owner-only) (#184).
    #[only_owner]
    pub fn set_wormhole_core(env: Env, core: Address) {
        require_not_paused(&env);
        require_current_state(&env);
        env.storage()
            .instance()
            .set(&BridgeDataKey::WormholeCore, &core);
    }

    /// Mark (or unmark) a `(chain_id, emitter_address)` pair as a trusted
    /// Wormhole message emitter. `complete_bridge_transfer` rejects any VAA
    /// whose emitter isn't marked trusted here (#267).
    #[only_owner]
    pub fn set_trusted_emitter(
        env: Env,
        chain_id: u32,
        emitter_address: BytesN<32>,
        trusted: bool,
    ) {
        require_not_paused(&env);
        require_current_state(&env);
        storage::set_persistent(
            &env,
            &BridgeDataKey::TrustedEmitter(chain_id, emitter_address.clone()),
            &trusted,
        );
        events::trusted_emitter_set(&env, chain_id, &emitter_address, trusted);
    }

    /// Initiate an outbound Wormhole cross-chain bridge transfer of HBS shares (#184).
    pub fn initiate_bridge_transfer(
        env: Env,
        from: Address,
        amount: i128,
        target_chain: u32,
        recipient: BytesN<32>,
        nonce: u64,
    ) -> u64 {
        require_current_state(&env);
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }
        Base::burn(&env, &from, amount);

        let token_address = wormhole::address_to_bytes32(&env, &env.current_contract_address());
        let payload = wormhole::BridgeTransferPayload {
            token_address,
            recipient: recipient.clone(),
            amount,
            source_chain: wormhole::chain_id::STELLAR,
            target_chain,
            nonce,
        };
        let payload_bytes = wormhole::serialize_bridge_payload(&env, &payload);
        let core: Address = env
            .storage()
            .instance()
            .get(&BridgeDataKey::WormholeCore)
            .unwrap_or_else(|| panic_with_error!(&env, VaultError::WormholeCoreNotSet));
        let client = WormholeCoreClient::new(&env, &core);
        let sequence = client.publish_message(&0u32, &payload_bytes);
        events::bridge_transfer_initiated(&env, &from, amount, target_chain, &recipient, sequence);
        sequence
    }

    /// Complete an inbound Wormhole cross-chain bridge transfer using a verified VAA (#184).
    pub fn complete_bridge_transfer(env: Env, vaa: Bytes) {
        require_current_state(&env);
        let core: Address = env
            .storage()
            .instance()
            .get(&BridgeDataKey::WormholeCore)
            .unwrap_or_else(|| panic_with_error!(&env, VaultError::WormholeCoreNotSet));
        let client = WormholeCoreClient::new(&env, &core);
        let parsed = client.verify_vaa(&vaa);
        let transfer = wormhole::parse_bridge_payload(&env, &parsed.payload);

        // Trust decision keyed off the VAA envelope's guardian-verified origin
        // chain, not the payload-embedded (unverified) transfer.source_chain (#452).
        let trusted: bool = env
            .storage()
            .persistent()
            .get(&BridgeDataKey::TrustedEmitter(
                parsed.emitter_chain,
                parsed.emitter_address.clone(),
            ))
            .unwrap_or(false);
        if !trusted {
            panic_with_error!(&env, VaultError::EmitterNotTrusted);
        }
        // The payload's target_chain is decoded and is now checked (#454) rather
        // than silently ignored — the field exists specifically to prevent a
        // message meant for a different destination from being processed here.
        if transfer.target_chain != wormhole::chain_id::STELLAR {
            panic_with_error!(&env, VaultError::BridgeWrongTargetChain);
        }
        // The payload's token_address is decoded and is now checked (#453) rather
        // than silently ignored — otherwise a VAA about a different asset would
        // be accepted and minted as HBS anyway if the emitter is ever reused for
        // a multi-asset bridge.
        if transfer.token_address
            != wormhole::address_to_bytes32(&env, &env.current_contract_address())
        {
            panic_with_error!(&env, VaultError::BridgeTokenMismatch);
        }
        let digest: BytesN<32> = env.crypto().sha256(&vaa).into();
        if env
            .storage()
            .persistent()
            .has(&BridgeDataKey::ConsumedVaa(digest.clone()))
        {
            panic_with_error!(&env, VaultError::VaaAlreadyConsumed);
        }
        storage::set_persistent(&env, &BridgeDataKey::ConsumedVaa(digest), &true);

        let to = wormhole::bytes32_to_address(&env, &transfer.recipient);
        if Base::total_supply(&env) + transfer.amount > MAX_HBS_SUPPLY {
            panic_with_error!(&env, VaultError::MaxSupplyExceeded);
        }
        Base::mint(&env, &to, transfer.amount);
        lock_deposit(&env, &to);
        events::bridge_transfer_completed(
            &env,
            parsed.emitter_chain,
            &parsed.emitter_address,
            &to,
            transfer.amount,
        );
    }

    // ── Flash loan ────────────────────────────────────────────────────────────

    const DEFAULT_FLASH_LOAN_FEE: u32 = 30;

    /// Set the flash loan fee in basis points (owner-only) (#184).
    #[only_owner]
    pub fn set_flash_loan_fee(env: Env, fee_bps: u32) {
        require_not_paused(&env);
        if !(0..=1000).contains(&fee_bps) {
            panic_with_error!(&env, VaultError::FlashLoanFeeOutOfRange);
        }
        if Self::flash_loan_fee(env.clone()) == fee_bps {
            return;
        }
        env.storage()
            .instance()
            .set(&VaultKey::FlashLoanFee, &fee_bps);
        events::flash_loan_fee_set(&env, fee_bps);
    }

    /// Query whether a funding round is currently active (#38).
    pub fn is_funding_round_active(env: Env) -> bool {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&VaultKey::FundingRoundActive)
            .unwrap_or(false)
    }

    /// Open a funding round — share transfers are blocked until it is closed (#38).
    #[only_owner]
    pub fn start_funding_round(env: Env) {
        require_current_state(&env);
        env.storage()
            .instance()
            .set(&VaultKey::FundingRoundActive, &true);
        events::funding_round_started(&env);
    }

    /// Close the active funding round, re-enabling share transfers (#38).
    #[only_owner]
    pub fn end_funding_round(env: Env) {
        require_current_state(&env);
        env.storage()
            .instance()
            .set(&VaultKey::FundingRoundActive, &false);
        events::funding_round_ended(&env);
    }

    /// Return the current flash loan fee in basis points (#184).
    pub fn flash_loan_fee(env: Env) -> u32 {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&VaultKey::FlashLoanFee)
            .unwrap_or(Self::DEFAULT_FLASH_LOAN_FEE)
    }

    /// Execute an uncollateralized flash loan of USDC capital (#184).
    pub fn execute_flash_loan(
        env: Env,
        initiator: Address,
        borrower: Address,
        amount: i128,
        data: Bytes,
    ) {
        require_current_state(&env);
        if amount <= 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }
        initiator.require_auth();

        let fee_bps = Self::flash_loan_fee(env.clone()) as i128;
        let fee = amount * fee_bps / BPS_SCALE;

        let vault = env.current_contract_address();

        if Base::total_supply(&env) + amount + fee > MAX_HBS_SUPPLY {
            panic_with_error!(&env, VaultError::MaxSupplyExceeded);
        }
        Base::mint(&env, &borrower, amount + fee);

        let client = FlashLoanReceiverClient::new(&env, &borrower);
        let ok = client.flash_loan_callback(&initiator, &vault, &amount, &fee, &data);
        if !ok {
            panic_with_error!(&env, VaultError::FlashLoanCallbackFailed);
        }

        Base::transfer(&env, &borrower, &MuxedAddress::from(&vault), amount + fee);
        Base::burn(&env, &vault, amount + fee);

        events::flash_loan(&env, &initiator, &borrower, amount, fee);
    }

    // ── Carbon credits ────────────────────────────────────────────────────────

    const CARBON_UNIT: i128 = 10_000_000_000;

    /// Set the carbon credit oracle address (owner-only) (#184).
    #[only_owner]
    pub fn set_carbon_oracle(env: Env, oracle: Address) {
        require_not_paused(&env);
        require_current_state(&env);
        let current: Option<Address> = env.storage().instance().get(&VaultKey::CarbonOracle);
        if current == Some(oracle.clone()) {
            return;
        }
        env.storage()
            .instance()
            .set(&VaultKey::CarbonOracle, &oracle);
        events::carbon_oracle_set(&env, &oracle);
    }

    /// Set the price per carbon credit (carbon oracle only) (#184).
    ///
    /// Informational/reserved only (issue #456): this value is not read by
    /// `calculate_carbon_credits`/`issue_carbon_credits` — credit amounts are
    /// computed purely from `project.green_impact`. It is stored and surfaced
    /// via `export_regulatory_data` for off-chain/future use, not as an
    /// on-chain input to credit issuance.
    pub fn set_carbon_credit_price(env: Env, price: i128) {
        require_not_paused(&env);
        require_current_state(&env);
        let oracle: Address = env
            .storage()
            .instance()
            .get(&VaultKey::CarbonOracle)
            .unwrap_or_else(|| panic_with_error!(&env, VaultError::CarbonOracleNotSet));
        oracle.require_auth();

        if price <= 0 {
            panic_with_error!(&env, VaultError::CarbonPriceNotPositive);
        }
        if Self::carbon_credit_price(env.clone()) == price {
            return;
        }
        env.storage()
            .instance()
            .set(&VaultKey::CarbonCreditPrice, &price);
        events::carbon_credit_price_set(&env, price);
    }

    /// Return the current carbon credit price (#184).
    pub fn carbon_credit_price(env: Env) -> i128 {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&VaultKey::CarbonCreditPrice)
            .unwrap_or(0)
    }

    /// Calculate carbon credit output based on investment amount and project green impact (#184).
    ///
    /// Panics with `AmountNotPositive` if `amount` is not positive (#403) -- a
    /// negative amount would otherwise silently produce a nonsensical negative
    /// `credits` value with no error, and only `issue_carbon_credits` (a
    /// separate caller) happened to reject that afterward.
    ///
    /// Emits `CarbonCreditsCalculated` on every call intentionally (#403): this
    /// is read-only and un-auth'd by design, mirroring `calculate_carbon_credits`
    /// being usable as a quote/preview before committing to `issue_carbon_credits`,
    /// so off-chain indexers can track calculation activity (e.g. for analytics
    /// on quoted-vs-issued credit volume) without requiring a state-mutating call.
    /// Each call still costs the caller their own transaction fee, which is
    /// sufficient to bound event-log spam for a function with no state to protect.
    pub fn calculate_carbon_credits(
        env: Env,
        project_id: u32,
        amount: i128,
    ) -> CarbonCreditCalculation {
        require_current_state(&env);
        if amount <= 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }
        let registry_addr: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
        let registry = registry_interface::Client::new(&env, &registry_addr);
        let project = registry.get_project(&project_id);

        let credits = amount * (project.green_impact as i128) / Self::CARBON_UNIT;

        events::carbon_credits_calculated(&env, project_id, amount, credits);

        CarbonCreditCalculation {
            project_id,
            amount_invested: amount,
            credits,
        }
    }

    /// Issue carbon credits to a specified recipient (#184).
    pub fn issue_carbon_credits(env: Env, to: Address, project_id: u32, amount: i128) -> i128 {
        require_current_state(&env);
        let calc = Self::calculate_carbon_credits(env.clone(), project_id, amount);

        if calc.credits <= 0 {
            panic_with_error!(&env, VaultError::NoCarbonCreditsToIssue);
        }

        let prev: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(to.clone()))
            .unwrap_or(0);
        storage::set_persistent(
            &env,
            &VaultKey::CarbonCreditBalance(to.clone()),
            &(prev + calc.credits),
        );

        calc.credits
    }

    /// Transfer carbon credits between accounts (#184).
    pub fn transfer_carbon_credits(env: Env, from: Address, to: Address, amount: i128) {
        require_current_state(&env);
        from.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, VaultError::AmountNotPositive);
        }

        let prev_from: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(from.clone()))
            .unwrap_or(0);
        if prev_from < amount {
            panic_with_error!(&env, VaultError::InsufficientCarbonCredits);
        }

        let prev_to: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(to.clone()))
            .unwrap_or(0);

        storage::set_persistent(
            &env,
            &VaultKey::CarbonCreditBalance(from.clone()),
            &(prev_from - amount),
        );
        storage::set_persistent(
            &env,
            &VaultKey::CarbonCreditBalance(to.clone()),
            &(prev_to + amount),
        );

        events::carbon_credits_transferred(&env, &from, &to, amount);
    }

    /// Return the carbon credit balance for a given address (#184).
    pub fn carbon_credit_balance(env: Env, address: Address) -> i128 {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(address))
            .unwrap_or(0)
    }

    // ── Compliance / regulatory reporting ─────────────────────────────────────

    const MAX_COMPLIANCE_EVENTS: u64 = 1000;

    /// Set the maximum transaction amount limit for compliance monitoring (owner-only) (#184).
    #[only_owner]
    pub fn set_max_transaction_amount(env: Env, amount: i128) {
        require_not_paused(&env);
        require_current_state(&env);
        if amount < 0 {
            panic_with_error!(&env, VaultError::NegativeMaxTransactionAmount);
        }
        if Self::max_transaction_amount(env.clone()) == amount {
            return;
        }
        env.storage()
            .instance()
            .set(&VaultKey::MaxTransactionAmount, &amount);
        events::max_transaction_amount_set(&env, amount);
    }

    /// Return the current compliance transaction limit (#184).
    pub fn max_transaction_amount(env: Env) -> i128 {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&VaultKey::MaxTransactionAmount)
            .unwrap_or(0)
    }

    /// Record a compliance or regulatory audit event in persistent storage (owner-only) (#184).
    #[only_owner]
    pub fn record_compliance_event(env: Env, event_type: String, data: String) {
        require_current_state(&env);
        let counter: u64 = env
            .storage()
            .instance()
            .get(&VaultKey::ComplianceEventCounter)
            .unwrap_or(0);
        let seq = counter + 1;

        let event = ComplianceEventData {
            seq,
            timestamp: env.ledger().timestamp(),
            event_type: event_type.clone(),
            data,
        };

        storage::set_persistent(&env, &VaultKey::ComplianceEvent(seq), &event);
        env.storage()
            .instance()
            .set(&VaultKey::ComplianceEventCounter, &seq);

        if seq > Self::MAX_COMPLIANCE_EVENTS {
            let prune = seq - Self::MAX_COMPLIANCE_EVENTS;
            env.storage()
                .persistent()
                .remove(&VaultKey::ComplianceEvent(prune));
        }

        events::compliance_event_recorded(&env, seq, &event_type);
    }

    /// Retrieve a specific compliance event by sequence number (#184).
    pub fn get_compliance_event(env: Env, seq: u64) -> ComplianceEventData {
        require_current_state(&env);
        env.storage()
            .persistent()
            .get(&VaultKey::ComplianceEvent(seq))
            .unwrap_or_else(|| panic_with_error!(&env, VaultError::ComplianceEventNotFound))
    }

    /// Retrieve a range of compliance events for reporting (#184).
    pub fn get_compliance_events(env: Env, from: u64, to: u64) -> Vec<ComplianceEventData> {
        require_current_state(&env);
        if from > to {
            return Vec::new(&env);
        }
        let max = if to - from > 100 { from + 100 } else { to };
        let mut events_vec = Vec::new(&env);
        for seq in from..=max {
            if let Some(event) = env
                .storage()
                .persistent()
                .get::<VaultKey, ComplianceEventData>(&VaultKey::ComplianceEvent(seq))
            {
                events_vec.push_back(event);
            }
        }
        events_vec
    }

    /// Capture a snapshot of key vault metrics for regulatory reporting (owner-only) (#184).
    #[only_owner]
    pub fn take_reporting_snapshot(env: Env) {
        require_current_state(&env);
        let snapshot = ReportingSnapshotData {
            timestamp: env.ledger().timestamp(),
            total_assets: Self::total_assets(env.clone()),
            total_supply: Base::total_supply(&env),
            total_investments: env
                .storage()
                .persistent()
                .get(&VaultKey::TotalInvestments)
                .unwrap_or(0),
        };
        env.storage()
            .instance()
            .set(&VaultKey::ReportingSnapshot, &snapshot);
        events::reporting_snapshot_taken(&env, snapshot.timestamp);
    }

    /// Retrieve the most recent regulatory reporting snapshot (#184).
    pub fn get_latest_snapshot(env: Env) -> ReportingSnapshotData {
        require_current_state(&env);
        env.storage()
            .instance()
            .get(&VaultKey::ReportingSnapshot)
            .unwrap_or_else(|| panic_with_error!(&env, VaultError::NoSnapshotTaken))
    }

    /// Export a full regulatory report combining current metrics and recent audit events (#184).
    pub fn export_regulatory_data(env: Env) -> RegulatoryReport {
        require_current_state(&env);
        let snapshot = env
            .storage()
            .instance()
            .get(&VaultKey::ReportingSnapshot)
            .unwrap_or(ReportingSnapshotData {
                timestamp: 0,
                total_assets: Self::total_assets(env.clone()),
                total_supply: Base::total_supply(&env),
                total_investments: env
                    .storage()
                    .persistent()
                    .get(&VaultKey::TotalInvestments)
                    .unwrap_or(0),
            });

        let counter: u64 = env
            .storage()
            .instance()
            .get(&VaultKey::ComplianceEventCounter)
            .unwrap_or(0);

        let start = if counter > 50 { counter - 50 + 1 } else { 1 };
        let recent_events = Self::get_compliance_events(env.clone(), start, counter);

        let max_amount = Self::max_transaction_amount(env.clone());
        let carbon_price = Self::carbon_credit_price(env.clone());

        RegulatoryReport {
            snapshot,
            recent_events,
            max_transaction_amount: max_amount,
            carbon_credit_price: carbon_price,
        }
    }
}

fn fund_project_internal(env: Env, project_id: u32, amount: i128) {
    require_current_state(&env);
    if amount <= 0 {
        panic_with_error!(&env, VaultError::AmountNotPositive);
    }
    // IDs start at 1; reject 0 before the cross-contract call (#87).
    if project_id == 0 {
        panic_with_error!(&env, VaultError::ProjectNotFound);
    }
    check_max_transaction_amount(&env, amount);

    let registry_addr: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
    let registry = registry_interface::Client::new(&env, &registry_addr);

    // The registry's own getters (including get_project) remain callable
    // while it's paused — only its state-mutating operations are blocked.
    // Explicitly reject new funding while the registry is paused (#72, #263):
    // scores/whitelist could be frozen mid-review, so treating the registry's
    // pause as "don't deploy new capital against this data" is safer than
    // silently funding against a registry an admin has intentionally halted.
    if registry.is_paused() {
        panic_with_error!(&env, VaultError::RegistryPaused);
    }

    // Removed total_projects() call — get_project() panics with ProjectNotFound
    // if the ID is unknown, so a separate bounds-check cross-contract call is
    // redundant and adds significant gas overhead (#87).
    let project = registry.get_project(&project_id);

    // Prevent the vault admin from funding a project they themselves own (#14).
    // fund_project/fund_project_with_approvals/batch_fund_projects all funnel
    // through here and are admin-gated, so without this check the admin could
    // create a project as themselves in the registry and self-fund it, moving
    // vault USDC to their own address disguised as legitimate project funding.
    if Some(project.owner.clone()) == get_owner(&env) {
        panic_with_error!(&env, VaultError::SelfFundingNotAllowed);
    }

    let min_credit: u32 = env
        .storage()
        .instance()
        .get(&VaultKey::MinCreditQuality)
        .unwrap_or(0);
    let min_green: u32 = env
        .storage()
        .instance()
        .get(&VaultKey::MinGreenImpact)
        .unwrap_or(0);
    if project.credit_quality < min_credit {
        panic_with_error!(&env, VaultError::BelowMinCreditQuality);
    }
    if project.green_impact < min_green {
        panic_with_error!(&env, VaultError::BelowMinGreenImpact);
    }

    // Enforce the per-project investment cap before committing any USDC (#32).
    let cap: i128 = env
        .storage()
        .instance()
        .get(&VaultKey::MaxInvestmentPerProject)
        .unwrap_or(MAX_INVESTMENT_PER_PROJECT);
    let current_investment: i128 = env
        .storage()
        .persistent()
        .get(&VaultKey::ProjectInvestment(project_id))
        .unwrap_or(0);
    if current_investment + amount > cap {
        panic_with_error!(&env, VaultError::InvestmentCapExceeded);
    }

    let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
    let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
        .balance(&env.current_contract_address());

    let insurance_reserve: i128 = env
        .storage()
        .persistent()
        .get(&VaultKey::InsuranceFund)
        .unwrap_or(0);
    let available = liquid - insurance_reserve;

    if amount > available {
        panic_with_error!(&env, VaultError::InsufficientDeployable);
    }

    soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
        &env.current_contract_address(),
        &project.owner,
        &amount,
    );

    let prev: i128 = env
        .storage()
        .persistent()
        .get(&VaultKey::ProjectInvestment(project_id))
        .unwrap_or(0);
    storage::set_persistent(
        &env,
        &VaultKey::ProjectInvestment(project_id),
        &(prev + amount),
    );

    // Record the first funding timestamp for time-weighted returns (#34).
    // Only set once — subsequent fund_project calls don't shift the origin.
    let ts_key = VaultKey::InvestmentTimestamp(project_id);
    if !env.storage().persistent().has(&ts_key) {
        storage::set_persistent(&env, &ts_key, &env.ledger().timestamp());
    }

    let total_inv: i128 = env
        .storage()
        .persistent()
        .get(&VaultKey::TotalInvestments)
        .unwrap_or(0);
    storage::set_persistent(&env, &VaultKey::TotalInvestments, &(total_inv + amount));

    events::project_funded(&env, project_id, amount, &project.owner);
}

fn receive_yield_internal(env: Env, from: Address, amount: i128) {
    require_current_state(&env);
    if amount <= 0 {
        panic_with_error!(&env, VaultError::YieldAmountNotPositive);
    }
    let total_shares = Base::total_supply(&env);
    if total_shares == 0 {
        panic_with_error!(&env, VaultError::NoSharesOutstanding);
    }

    let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
    soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
        &from,
        env.current_contract_address(),
        &amount,
    );

    let delta = amount * YIELD_SCALE / total_shares;
    let accum: i128 = env
        .storage()
        .persistent()
        .get(&VaultKey::YieldPerShareAccum)
        .unwrap_or(0);
    storage::set_persistent(&env, &VaultKey::YieldPerShareAccum, &(accum + delta));

    events::yield_received(&env, &from, amount);
}

fn claim_insurance_internal(env: Env, project_id: u32, recipient: Address, amount: i128) {
    require_current_state(&env);
    if amount <= 0 {
        panic_with_error!(&env, VaultError::ClaimAmountNotPositive);
    }
    check_max_transaction_amount(&env, amount);
    let already_claimed: bool = env
        .storage()
        .persistent()
        .get(&VaultKey::InsuranceClaimed(project_id))
        .unwrap_or(false);
    if already_claimed {
        panic_with_error!(&env, VaultError::InsuranceAlreadyClaimed);
    }
    let fund: i128 = env
        .storage()
        .persistent()
        .get(&VaultKey::InsuranceFund)
        .unwrap_or(0);
    if amount > fund {
        panic_with_error!(&env, VaultError::InsufficientInsurance);
    }

    storage::set_persistent(&env, &VaultKey::InsuranceClaimed(project_id), &true);
    storage::set_persistent(&env, &VaultKey::InsuranceFund, &(fund - amount));

    let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
    soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
        &env.current_contract_address(),
        &recipient,
        &amount,
    );

    events::insurance_claimed(&env, project_id, &recipient, amount);
}

/// Thin wrapper around the shared `multisig` crate (#459) mapping its
/// generic errors onto this contract's own `VaultError` codes.
fn validate_multisig_config(env: &Env, signers: &Vec<Address>, threshold: u32) {
    if let Err(e) = multisig::validate_multisig_config(signers, threshold, MAX_MULTISIG_SIGNERS) {
        match e {
            multisig::ConfigError::TooManySigners => {
                panic_with_error!(env, VaultError::TooManyMultiSigSigners)
            }
            multisig::ConfigError::InvalidThreshold => {
                panic_with_error!(env, VaultError::InvalidMultiSigThreshold)
            }
            multisig::ConfigError::DuplicateSigner => {
                panic_with_error!(env, VaultError::DuplicateApproval)
            }
        }
    }
}

fn require_admin_approval(env: &Env, approvals: Vec<Address>) {
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&VaultKey::MultiSigThreshold)
        .unwrap_or(0);
    let signers: Vec<Address> = env
        .storage()
        .instance()
        .get(&VaultKey::MultiSigSigners)
        .unwrap_or_else(|| Vec::new(env));
    let owner = stellar_access::ownable::get_owner(env).unwrap();
    if let Err(e) = multisig::require_admin_approval(&owner, threshold, &signers, approvals) {
        match e {
            multisig::ApprovalError::InvalidThreshold => {
                panic_with_error!(env, VaultError::InvalidMultiSigThreshold)
            }
            multisig::ApprovalError::DuplicateApproval => {
                panic_with_error!(env, VaultError::DuplicateApproval)
            }
            multisig::ApprovalError::NotSigner => {
                panic_with_error!(env, VaultError::NotMultiSigSigner)
            }
            multisig::ApprovalError::InsufficientApprovals => {
                panic_with_error!(env, VaultError::InsufficientApprovals)
            }
        }
    }
}

fn require_multisig_disabled(env: &Env) {
    let threshold: u32 = env
        .storage()
        .instance()
        .get(&VaultKey::MultiSigThreshold)
        .unwrap_or(0);
    if !multisig::is_multisig_disabled(threshold) {
        panic_with_error!(env, VaultError::InsufficientApprovals);
    }
}

fn read_state_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&VaultKey::StateVersion)
        .unwrap_or(0)
}

fn require_current_state(env: &Env) {
    if read_state_version(env) != STATE_VERSION {
        panic_with_error!(env, VaultError::UnsupportedStateVersion);
    }
}

/// Enforce the configured compliance transaction limit, if any (#457).
/// `0` (the default) means "no limit configured" — matches the documented
/// convention for `MaxTransactionAmount`.
fn check_max_transaction_amount(env: &Env, amount: i128) {
    let max: i128 = env
        .storage()
        .instance()
        .get(&VaultKey::MaxTransactionAmount)
        .unwrap_or(0);
    if max > 0 && amount > max {
        panic_with_error!(env, VaultError::ExceedsMaxTransactionAmount);
    }
}

fn require_not_paused(env: &Env) {
    let paused: bool = env
        .storage()
        .instance()
        .get(&VaultKey::Paused)
        .unwrap_or(false);
    if paused {
        panic_with_error!(env, VaultError::Paused);
    }
}

/// Panics unless `caller` is the configured emergency admin (#43).
fn require_emergency_admin(env: &Env, caller: &Address) {
    let emergency_admin: Option<Address> = env.storage().instance().get(&VaultKey::EmergencyAdmin);
    if emergency_admin.as_ref() != Some(caller) {
        panic_with_error!(env, VaultError::NotEmergencyAdmin);
    }
}

/// Record the current ledger timestamp as the depositor's lock origin (#33).
/// The lock prevents withdrawal for MIN_LOCK_PERIOD seconds after each deposit,
/// blocking flash-deposit-withdraw attacks that could manipulate share pricing.
fn lock_deposit(env: &Env, address: &Address) {
    storage::set_persistent(
        env,
        &VaultKey::LastDeposit(address.clone()),
        &env.ledger().timestamp(),
    );
}

/// Reject a withdrawal if the caller's deposit lock has not yet expired (#33).
fn check_deposit_lock(env: &Env, address: &Address) {
    if let Some(deposited_at) = env
        .storage()
        .persistent()
        .get::<_, u64>(&VaultKey::LastDeposit(address.clone()))
    {
        if env.ledger().timestamp() < deposited_at + MIN_LOCK_PERIOD {
            panic_with_error!(env, VaultError::DepositLocked);
        }
    }
}

#[contractimpl]
impl InvestmentVault {
    #[only_owner]
    pub fn pause(env: Env) {
        env.storage().instance().set(&VaultKey::Paused, &true);
        events::paused(&env);
    }

    #[only_owner]
    pub fn unpause(env: Env) {
        env.storage().instance().set(&VaultKey::Paused, &false);
        events::unpaused(&env);
    }

    /// Return whether the vault is currently paused (#72).
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&VaultKey::Paused)
            .unwrap_or(false)
    }

    /// Set (or clear, via `None`) the emergency-admin address. Owner-only (#43).
    ///
    /// The emergency admin may call `emergency_pause`/`emergency_unpause` without
    /// holding full owner privileges — intended for a fast-response operational
    /// role, separate from the owner who manages funding, fees, etc.
    #[only_owner]
    pub fn set_emergency_admin(env: Env, emergency_admin: Option<Address>) {
        match &emergency_admin {
            Some(addr) => env
                .storage()
                .instance()
                .set(&VaultKey::EmergencyAdmin, addr),
            None => env.storage().instance().remove(&VaultKey::EmergencyAdmin),
        }
        events::emergency_admin_changed(&env, emergency_admin);
    }

    /// Return the configured emergency-admin address, if any.
    pub fn get_emergency_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&VaultKey::EmergencyAdmin)
    }

    /// Pause the vault as the emergency admin, without requiring owner auth (#43).
    pub fn emergency_pause(env: Env, caller: Address) {
        caller.require_auth();
        require_emergency_admin(&env, &caller);
        env.storage().instance().set(&VaultKey::Paused, &true);
        events::paused(&env);
    }

    /// Unpause the vault as the emergency admin, without requiring owner auth (#43).
    pub fn emergency_unpause(env: Env, caller: Address) {
        caller.require_auth();
        require_emergency_admin(&env, &caller);
        env.storage().instance().set(&VaultKey::Paused, &false);
        events::unpaused(&env);
    }

    // ── Storage compaction (#88) ───────────────────────────────────────────────

    /// Remove zero-value `ProjectInvestment` persistent storage entries. Admin-only.
    ///
    /// After a project fully repays, its investment counter drops to 0 but the storage
    /// slot remains. This function iterates projects 1..=`total_projects` and removes
    /// any zero entries, reducing ongoing rent costs.
    /// Returns the number of entries removed.
    #[only_owner]
    pub fn compact_storage(env: Env) -> u32 {
        require_current_state(&env);
        let registry_addr: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
        let registry = registry_interface::Client::new(&env, &registry_addr);
        let total = registry.total_projects();

        let mut removed: u32 = 0;
        for id in 1..=total {
            let key = VaultKey::ProjectInvestment(id);
            if env.storage().persistent().has(&key) {
                let val: i128 = env.storage().persistent().get(&key).unwrap_or(0);
                if val == 0 {
                    env.storage().persistent().remove(&key);
                    removed += 1;
                }
            }
        }
        removed
    }

    #[only_owner]
    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        // events::upgraded(&env) could be called here if needed
    }
}
#[contractimpl(contracttrait)]
impl FungibleToken for InvestmentVault {
    type ContractType = Base;

    fn transfer(e: &Env, from: Address, to: MuxedAddress, amount: i128) {
        require_current_state(e);
        // Soroban has no zero address; the vault's own address is the closest
        // equivalent — shares sent here can never be recovered (#118).
        if to.address() == e.current_contract_address() {
            panic_with_error!(e, VaultError::TransferToVaultBlocked);
        }
        // Block share transfers while a funding round is active to prevent
        // vault-accounting manipulation during project funding (#38).
        let round_active: bool = e
            .storage()
            .instance()
            .get(&VaultKey::FundingRoundActive)
            .unwrap_or(false);
        if round_active {
            panic_with_error!(e, VaultError::FundingRoundActive);
        }
        Base::transfer(e, &from, &to, amount);
        lock_deposit(e, &to.address());
    }
}

#[contractimpl(contracttrait)]
impl FungibleBurnable for InvestmentVault {}

#[contractimpl(contracttrait)]
impl Ownable for InvestmentVault {
    /// Initiates a 2-step ownership transfer and emits a project-specific
    /// `OwnershipTransferred` event for auditing (#30).
    fn transfer_ownership(e: &Env, new_owner: Address, live_until_ledger: u32) {
        let old_owner =
            get_owner(e).unwrap_or_else(|| panic_with_error!(e, VaultError::OwnerNotSet));
        ownable_transfer_ownership(e, &new_owner, live_until_ledger);
        events::ownership_transferred(e, &old_owner, &new_owner);
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod wasm_test;
