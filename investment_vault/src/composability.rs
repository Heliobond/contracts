//! Minimal interface traits for external contract composability (#71).
//!
//! These traits define the API surface that other Soroban contracts
//! (DEXes, lending protocols, yield aggregators) should depend on when
//! integrating with the Heliobond vault.
//!
//! # Usage
//!
//! External contracts should use `soroban_sdk::contractimport!` to import the
//! vault's WASM, which generates a typed client. These traits document the
//! expected interface and can be implemented by mock contracts in tests.

use soroban_sdk::{Address, Env};

use crate::types::{CarbonCreditCalculation, PortfolioInfo, RegulatoryReport};

/// Minimal vault interface for share price queries and basic operations.
/// External protocols (DEXes, lending pools) should depend on this trait
/// rather than the full vault implementation for loose coupling.
pub trait VaultBaseInterface {
    /// Return the vault's net asset value (NAV) in USDC.
    fn total_assets(env: Env) -> i128;

    /// Convert a USDC amount to vault shares at the current NAV.
    fn convert_to_shares(env: Env, usdc_amount: i128) -> i128;

    /// Convert vault shares to USDC at the current NAV.
    fn convert_to_assets(env: Env, shares_amount: i128) -> i128;

    /// Return the HBS token balance of `account`.
    fn balance(env: Env, account: Address) -> i128;

    /// Return the total HBS supply.
    fn total_supply(env: Env) -> i128;

    /// Return the primary accepted asset (USDC SAC address).
    fn accepted_asset(env: Env) -> Address;
}

/// Vault query interface for portfolio analytics and reporting.
pub trait VaultQueryInterface {
    /// Return full portfolio snapshot for `account`.
    fn get_portfolio(env: Env, account: Address) -> PortfolioInfo;

    /// Return the vault utilization in basis points.
    fn get_utilization_bps(env: Env) -> u32;

    /// Return the insurance fund USDC balance.
    fn insurance_fund_balance(env: Env) -> i128;

    /// Return unclaimed yield for `account`.
    fn claimable_yield(env: Env, account: Address) -> i128;

    /// Return cached expected returns (O(1), no iteration).
    fn get_expected_returns(env: Env) -> i128;

    /// Manually recompute expected returns from registry (O(n)).
    fn refresh_expected_returns(env: Env) -> i128;

    /// Manually recompute total_assets from current on-chain state.
    fn refresh_total_assets(env: Env) -> i128;

    /// Return comprehensive regulatory data.
    fn export_regulatory_data(env: Env) -> RegulatoryReport;

    /// Calculate carbon credits for a project investment.
    fn calculate_carbon_credits(env: Env, project_id: u32, amount: i128) -> CarbonCreditCalculation;
}

/// Vault operation interface for deposits, withdrawals, and yield claims.
pub trait VaultOperationInterface {
    /// Deposit USDC and mint HBS shares.
    fn deposit(env: Env, from: Address, usdc_amount: i128) -> i128;

    /// Burn HBS shares and withdraw USDC (may enqueue if liquidity low).
    fn withdraw(env: Env, from: Address, shares_amount: i128) -> i128;

    /// Claim accumulated yield for `from`.
    fn claim_yield(env: Env, from: Address) -> i128;

    /// Settle queued redemptions in FIFO order.
    fn claim(env: Env) -> i128;

    /// Fund a registered project (admin only).
    fn fund_project(env: Env, project_id: u32, amount: i128);
}

/// Bridge query interface for cross-chain integration.
pub trait BridgeQueryInterface {
    /// Return whether secondary market trading is enabled.
    fn is_trading_enabled(env: Env) -> bool;

    /// Return HBS token metadata for DEX listings.
    fn get_hbs_token_info(env: Env) -> crate::types::HBSTokenInfo;
}
