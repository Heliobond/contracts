#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, MuxedAddress, String};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;
use stellar_tokens::fungible::burnable::FungibleBurnable;
use stellar_tokens::fungible::{Base, FungibleToken};

/// Maximum single deposit: 1 billion USDC (7 decimals) — prevents i128 overflow
/// in share calculations and caps single-user concentration risk (#112).
const MAX_DEPOSIT: i128 = 1_000_000_000 * 10_000_000;

/// Scaling factor for the yield-per-share accumulator (#125).
/// Large enough to preserve precision when total_shares >> yield amount.
const YIELD_SCALE: i128 = 1_000_000_000_000_000_000; // 1e18

/// Basis points deducted from each deposit as an insurance premium (#135).
/// 50 bps = 0.5 % of deposit amount.
const INSURANCE_PREMIUM_BPS: i128 = 50;

mod events;
mod types;

mod registry_interface {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/project_registry.wasm");
}

pub use types::{HBSTokenInfo, PortfolioInfo, VaultKey};

/// Hard cap on the management fee to protect investors (#7).
/// 500 bps = 5% maximum.
const MAX_MANAGEMENT_FEE_BPS: u32 = 500;

#[contract]
pub struct InvestmentVault;

#[contractimpl]
impl InvestmentVault {
    pub fn __constructor(env: Env, admin: Address, usdc_sac: Address, registry: Address) {
        set_owner(&env, &admin);
        // Validate that registry is a deployed ProjectRegistry contract by calling it.
        // This panics at construction time if the address is invalid.
        registry_interface::Client::new(&env, &registry).total_projects();
        env.storage().instance().set(&VaultKey::UsdcSac, &usdc_sac);
        env.storage().instance().set(&VaultKey::Registry, &registry);
        env.storage()
            .persistent()
            .set(&VaultKey::TotalInvestments, &0i128);
        Base::set_metadata(
            &env,
            7,
            String::from_str(&env, "Heliobond Shares"),
            String::from_str(&env, "HBS"),
        );
    }

    #[only_owner]
    pub fn fund_project(env: Env, project_id: u32, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let registry_addr: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
        let registry = registry_interface::Client::new(&env, &registry_addr);
        let project = registry.get_project(&project_id);

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());

        // Reserve the insurance fund — it must never be used for project funding (#113).
        // Reading and subtracting here means concurrent fund_project calls in different
        // transactions each see the current on-chain balance, so no double-spend is possible
        // (Soroban transactions are serialised per ledger). The explicit reserve check
        // prevents the admin from accidentally draining USDC that belongs to the fund.
        let insurance_reserve: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::InsuranceFund)
            .unwrap_or(0);
        let available = liquid - insurance_reserve;

        if amount > available {
            panic!("insufficient deployable USDC (insurance reserve is protected)");
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
        env.storage()
            .persistent()
            .set(&VaultKey::ProjectInvestment(project_id), &(prev + amount));

        let total_inv: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::TotalInvestments)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&VaultKey::TotalInvestments, &(total_inv + amount));

        events::project_funded(&env, project_id, amount, &project.owner);
    }

    pub fn get_expected_returns(env: Env) -> i128 {
        let registry_addr: Address = env.storage().instance().get(&VaultKey::Registry).unwrap();
        let registry = registry_interface::Client::new(&env, &registry_addr);
        let total_projects = registry.total_projects();

        let mut expected: i128 = 0;
        for i in 1..=total_projects {
            let investment: i128 = env
                .storage()
                .persistent()
                .get(&VaultKey::ProjectInvestment(i))
                .unwrap_or(0);
            if investment > 0 {
                let project = registry.get_project(&i);
                expected += investment
                    * (project.credit_quality as i128 + project.green_impact as i128)
                    / 200;
            }
        }
        expected
    }

    pub fn total_assets(env: Env) -> i128 {
        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());
        let investments: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::TotalInvestments)
            .unwrap_or(0);
        liquid + investments + Self::get_expected_returns(env.clone())
    }

    pub fn convert_to_shares(env: Env, usdc_amount: i128) -> i128 {
        let total_assets = Self::total_assets(env.clone());
        let total_shares = Base::total_supply(&env);
        if total_shares == 0 || total_assets == 0 {
            // 1:1 mint when vault is empty (#111)
            usdc_amount
        } else {
            usdc_amount * total_shares / total_assets
        }
    }

    pub fn convert_to_assets(env: Env, shares_amount: i128) -> i128 {
        let total_assets = Self::total_assets(env.clone());
        let total_shares = Base::total_supply(&env);
        if total_shares == 0 || total_assets == 0 {
            // No assets to redeem when vault is empty (#111)
            0
        } else {
            shares_amount * total_assets / total_shares
        }
    }

    pub fn deposit(env: Env, from: Address, usdc_amount: i128) -> i128 {
        from.require_auth();
        if usdc_amount <= 0 {
            panic!("deposit must be positive");
        }
        if usdc_amount > MAX_DEPOSIT {
            panic!("deposit exceeds maximum");
        }

        // Deduct insurance premium before share calculation (#135)
        let premium = usdc_amount * INSURANCE_PREMIUM_BPS / 10_000;

        // Deduct optional management fee (#7)
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&VaultKey::ManagementFeeBps)
            .unwrap_or(0);
        let fee_amount = usdc_amount * (fee_bps as i128) / 10_000;

        let investable = usdc_amount - premium - fee_amount;

        let shares = Self::convert_to_shares(env.clone(), investable);

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let token = soroban_sdk::token::TokenClient::new(&env, &usdc_sac);
        token.transfer(
            &from,
            &env.current_contract_address(),
            &usdc_amount,
        );

        // Credit insurance fund with the premium (#135)
        let ins: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::InsuranceFund)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&VaultKey::InsuranceFund, &(ins + premium));

        // Transfer management fee to recipient if non-zero (#7)
        if fee_amount > 0 {
            let recipient: Address = env
                .storage()
                .instance()
                .get(&VaultKey::ManagementFeeRecipient)
                .unwrap_or_else(|| panic!("fee recipient not set"));
            token.transfer(&env.current_contract_address(), &recipient, &fee_amount);
        }

        // Track lifetime deposits for portfolio analytics (#132)
        let prev_dep: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::TotalDeposited(from.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&VaultKey::TotalDeposited(from.clone()), &(prev_dep + usdc_amount));

        Base::mint(&env, &from, shares);
        events::deposit(&env, &from, usdc_amount, shares);

        shares
    }

    pub fn withdraw(env: Env, from: Address, shares_amount: i128) -> i128 {
        // Note: from.require_auth() is called inside Base::burn
        if shares_amount <= 0 {
            panic!("shares must be positive");
        }

        let usdc_returned = Self::convert_to_assets(env.clone(), shares_amount);

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());

        if usdc_returned > liquid {
            panic!("insufficient liquid USDC: funds may be deployed to projects");
        }

        Base::burn(&env, &from, shares_amount);
        soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
            &env.current_contract_address(),
            &from,
            &usdc_returned,
        );

        events::withdraw(&env, &from, shares_amount, usdc_returned);
        usdc_returned
    }

    // ── Yield distribution (#125) ──────────────────────────────────────────────

    /// Deposit USDC yield into the vault and update the per-share accumulator.
    /// Called by the owner when a project makes a repayment.
    #[only_owner]
    pub fn receive_yield(env: Env, from: Address, amount: i128) {
        if amount <= 0 {
            panic!("yield amount must be positive");
        }
        let total_shares = Base::total_supply(&env);
        if total_shares == 0 {
            panic!("no shares outstanding");
        }

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
            &from,
            &env.current_contract_address(),
            &amount,
        );

        // Increase global accumulator: delta = amount * YIELD_SCALE / total_shares
        let delta = amount * YIELD_SCALE / total_shares;
        let accum: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::YieldPerShareAccum)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&VaultKey::YieldPerShareAccum, &(accum + delta));

        events::yield_received(&env, &from, amount);
    }

    /// Return the USDC yield claimable by `account` without modifying state.
    pub fn claimable_yield(env: Env, account: Address) -> i128 {
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
        env.storage()
            .persistent()
            .set(&VaultKey::YieldDebt(from.clone()), &accum);

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        let liquid = soroban_sdk::token::TokenClient::new(&env, &usdc_sac)
            .balance(&env.current_contract_address());
        if claimable > liquid {
            panic!("insufficient liquid USDC for yield");
        }

        soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
            &env.current_contract_address(),
            &from,
            &claimable,
        );

        events::yield_claimed(&env, &from, claimable);
        claimable
    }

    // ── Portfolio analytics (#132) ─────────────────────────────────────────────

    /// Return a full on-chain portfolio snapshot for `account`.
    pub fn get_portfolio(env: Env, account: Address) -> PortfolioInfo {
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
            shares * 10_000 / total_shares
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
        env.storage()
            .persistent()
            .get(&VaultKey::InsuranceFund)
            .unwrap_or(0)
    }

    /// Pay out an insurance claim for a defaulted project (owner only).
    /// Transfers `amount` from the insurance fund to `recipient`.
    #[only_owner]
    pub fn claim_insurance(env: Env, project_id: u32, recipient: Address, amount: i128) {
        if amount <= 0 {
            panic!("claim amount must be positive");
        }
        let already_claimed: bool = env
            .storage()
            .persistent()
            .get(&VaultKey::InsuranceClaimed(project_id))
            .unwrap_or(false);
        if already_claimed {
            panic!("insurance already claimed for this project");
        }
        let fund: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::InsuranceFund)
            .unwrap_or(0);
        if amount > fund {
            panic!("insufficient insurance fund");
        }
        // Mark as claimed before transfer (CEI)
        env.storage()
            .persistent()
            .set(&VaultKey::InsuranceClaimed(project_id), &true);
        env.storage()
            .persistent()
            .set(&VaultKey::InsuranceFund, &(fund - amount));

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
            &env.current_contract_address(),
            &recipient,
            &amount,
        );

        events::insurance_claimed(&env, project_id, &recipient, amount);
    }

    // ── Multi-asset configuration (#133) ──────────────────────────────────────

    /// Return the primary accepted asset (USDC SAC address).
    /// Multi-asset vaults should extend this by adding accepted_assets to config.
    pub fn accepted_asset(env: Env) -> Address {
        env.storage().instance().get(&VaultKey::UsdcSac).unwrap()
    }

    // ── Management fee (#7) ───────────────────────────────────────────────────

    /// Set the optional management fee deducted from each deposit.
    /// `fee_bps` is bounded by MAX_MANAGEMENT_FEE_BPS (500 = 5%).
    /// Pass `fee_bps = 0` to disable the fee entirely.
    #[only_owner]
    pub fn set_management_fee(env: Env, fee_bps: u32, recipient: Address) {
        if fee_bps > MAX_MANAGEMENT_FEE_BPS {
            panic!("fee exceeds maximum of {} bps", MAX_MANAGEMENT_FEE_BPS);
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
        env.storage()
            .instance()
            .set(&VaultKey::TradingEnabled, &true);
        events::trading_enabled(&env, true);
    }

    /// Return whether the admin has enabled secondary market trading for HBS.
    pub fn is_trading_enabled(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&VaultKey::TradingEnabled)
            .unwrap_or(false)
    }

    /// Return HBS token metadata for DEX listing and secondary market integration.
    /// The `trading_enabled` field mirrors `is_trading_enabled()`.
    pub fn get_hbs_token_info(env: Env) -> HBSTokenInfo {
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
}

#[contractimpl(contracttrait)]
impl FungibleToken for InvestmentVault {
    type ContractType = Base;

    fn transfer(e: &Env, from: Address, to: MuxedAddress, amount: i128) {
        // Soroban has no zero address; the vault's own address is the closest
        // equivalent — shares sent here can never be recovered (#118).
        if to.address() == e.current_contract_address() {
            panic!("transfer to vault address not allowed");
        }
        Base::transfer(e, &from, &to, amount);
    }
}

#[contractimpl(contracttrait)]
impl FungibleBurnable for InvestmentVault {}

#[contractimpl(contracttrait)]
impl Ownable for InvestmentVault {}

#[cfg(test)]
mod test;
