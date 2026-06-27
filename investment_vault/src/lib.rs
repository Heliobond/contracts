#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, MuxedAddress, String, Vec};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;
use stellar_tokens::fungible::burnable::FungibleBurnable;
use stellar_tokens::fungible::{Base, FungibleToken};

mod events;
mod types;
mod wormhole;

mod registry_interface {
    soroban_sdk::contractimport!(file = "../target/wasm32v1-none/release/project_registry.wasm");
}

/// Wormhole core contract client interface.
/// In production, replace with `contractimport!` pointing to the
/// deployed Wormhole core contract WASM.
#[soroban_sdk::contractclient(name = "WormholeCoreClient")]
pub trait WormholeCore {
    /// Verify a VAA and return emitter chain, emitter address, and payload.
    fn verify_vaa(env: Env, vaa: Bytes) -> wormhole::ParsedVaa;

    /// Publish a message to the guardian network. Returns sequence number.
    fn publish_message(env: Env, consistency_level: u32, payload: Bytes) -> u64;
}

/// Interface for flash loan receiver contracts.
/// Any contract receiving HBS via `execute_flash_loan` must implement this.
#[soroban_sdk::contractclient(name = "FlashLoanReceiverClient")]
pub trait FlashLoanReceiver {
    /// Called by the vault after lending HBS. The receiver must repay
    /// `amount + fee` HBS to the vault contract before returning.
    /// Returns `true` on success.
    ///
    /// - `initiator` — the address that called `execute_flash_loan`.
    /// - `vault`    — this vault's address (repay HBS here).
    /// - `amount`   — principal borrowed.
    /// - `fee`      — flash loan fee.
    /// - `data`     — arbitrary forwarder data.
    fn flash_loan_callback(
        env: Env,
        initiator: Address,
        vault: Address,
        amount: i128,
        fee: i128,
        data: Bytes,
    ) -> bool;
}

pub use types::{
    CarbonCreditCalculation, ComplianceEventData, RegulatoryReport, ReportingSnapshotData, VaultKey,
};
pub use wormhole::{BridgeDataKey, BridgeTransferPayload};

#[contract]
pub struct InvestmentVault;

#[contractimpl]
impl InvestmentVault {
    pub fn __constructor(env: Env, admin: Address, usdc_sac: Address, registry: Address) {
        set_owner(&env, &admin);
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
    pub fn set_bridge(env: Env, bridge: Address) {
        env.storage().instance().set(&VaultKey::Bridge, &bridge);
        events::bridge_set(&env, &bridge);
    }

    pub fn bridge_mint(env: Env, to: Address, amount: i128) {
        let bridge: Address = env
            .storage()
            .instance()
            .get(&VaultKey::Bridge)
            .expect("bridge not set");
        bridge.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        Base::mint(&env, &to, amount);
        events::bridge_mint(&env, &to, amount);
    }

    /// Burn HBS tokens for outbound bridging.
    /// Requires authentication from `from`.
    pub fn bridge_burn(env: Env, from: Address, amount: i128) {
        from.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        Base::burn(&env, &from, amount);
        events::bridge_burn(&env, &from, amount);
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

        if amount > liquid {
            panic!("insufficient liquid USDC");
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
        if total_shares == 0 {
            usdc_amount
        } else {
            usdc_amount * total_shares / total_assets
        }
    }

    pub fn convert_to_assets(env: Env, shares_amount: i128) -> i128 {
        let total_assets = Self::total_assets(env.clone());
        let total_shares = Base::total_supply(&env);
        if total_shares == 0 {
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

        let shares = Self::convert_to_shares(env.clone(), usdc_amount);

        let usdc_sac: Address = env.storage().instance().get(&VaultKey::UsdcSac).unwrap();
        soroban_sdk::token::TokenClient::new(&env, &usdc_sac).transfer(
            &from,
            &env.current_contract_address(),
            &usdc_amount,
        );

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
            panic!("insufficient liquid USDC");
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

    // -----------------------------------------------------------------------
    // Flash loan
    // -----------------------------------------------------------------------

    const DEFAULT_FLASH_LOAN_FEE: i128 = 30; // 0.3% in basis points

    /// Set the flash loan fee in basis points (1 bp = 0.01%).
    /// Only callable by the contract owner.
    #[only_owner]
    pub fn set_flash_loan_fee(env: Env, fee_bps: i128) {
        if fee_bps < 0 || fee_bps > 1000 {
            panic!("fee must be 0–1000 bps (0%–10%)");
        }
        env.storage()
            .instance()
            .set(&VaultKey::FlashLoanFee, &fee_bps);
        events::flash_loan_fee_set(&env, fee_bps);
    }

    /// Return the current flash loan fee in basis points.
    pub fn flash_loan_fee(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&VaultKey::FlashLoanFee)
            .unwrap_or(Self::DEFAULT_FLASH_LOAN_FEE)
    }

    /// Execute a flash loan of HBS tokens.
    ///
    /// 1. Mints `amount` HBS to `borrower`.
    /// 2. Calls `borrower.flash_loan_callback(initiator, vault, amount, fee, data)`.
    /// 3. After the callback, transfers `amount + fee` back from the borrower
    ///    via internal bookkeeping (no cross-contract re-entrancy).
    /// 4. Burns the principal; the fee stays in the vault as protocol revenue.
    ///
    /// Panics if the callback fails or the borrower cannot repay.
    pub fn execute_flash_loan(
        env: Env,
        initiator: Address,
        borrower: Address,
        amount: i128,
        data: Bytes,
    ) {
        if amount <= 0 {
            panic!("amount must be positive");
        }

        initiator.require_auth();

        let fee_bps = Self::flash_loan_fee(env.clone());
        let fee = amount * fee_bps / 10000;

        let vault = env.current_contract_address();

        // Mint HBS to borrower (amount + fee so they can repay without
        // needing pre-existing HBS — Soroban re-entrancy prevents acquiring
        // HBS from external sources during the callback).
        Base::mint(&env, &borrower, amount + fee);

        // Call borrower's callback
        let client = FlashLoanReceiverClient::new(&env, &borrower);
        let ok = client.flash_loan_callback(&initiator, &vault, &amount, &fee, &data);
        if !ok {
            panic!("flash loan callback failed");
        }

        // Reclaim repayment — fails if borrower's balance is insufficient
        Base::transfer(&env, &borrower, &MuxedAddress::from(&vault), amount + fee);

        // Burn the principal from vault — fee stays as protocol revenue
        Base::burn(&env, &vault, amount);

        events::flash_loan(&env, &initiator, &borrower, amount, fee);
    }

    // -----------------------------------------------------------------------
    // Carbon credit integration
    // -----------------------------------------------------------------------

    /// The carbon-unit constant used to convert green-impact × investment
    /// into carbon credits.
    ///
    /// ## Formula
    ///
    /// ```ignore
    /// credits = amount_invested × project.green_impact / CARBON_UNIT
    /// ```
    ///
    /// With `CARBON_UNIT = 10_000_000_000`:
    /// - 1 USDC invested in a 100-green-impact project → 1 carbon credit
    /// - 10 USDC at 50 green-impact → 0.5 credits (truncated to 0)
    /// - 500 USDC at 60 green-impact → 30 credits
    const CARBON_UNIT: i128 = 10_000_000_000;

    /// Set the carbon credit oracle address.
    /// The oracle is trusted to report accurate carbon credit prices.
    /// Only callable by the contract owner.
    #[only_owner]
    pub fn set_carbon_oracle(env: Env, oracle: Address) {
        env.storage()
            .instance()
            .set(&VaultKey::CarbonOracle, &oracle);
        events::carbon_oracle_set(&env, &oracle);
    }

    /// Update the carbon credit price (USD per credit, in micro-units).
    /// Only the carbon oracle may call this.
    pub fn set_carbon_credit_price(env: Env, price: i128) {
        let oracle: Address = env
            .storage()
            .instance()
            .get(&VaultKey::CarbonOracle)
            .expect("carbon oracle not set");
        oracle.require_auth();

        if price <= 0 {
            panic!("price must be positive");
        }
        env.storage()
            .instance()
            .set(&VaultKey::CarbonCreditPrice, &price);
        events::carbon_credit_price_set(&env, price);
    }

    /// Return the current carbon credit price (USD × 10⁷ per credit), or 0 if
    /// not yet set.
    pub fn carbon_credit_price(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&VaultKey::CarbonCreditPrice)
            .unwrap_or(0)
    }

    /// Calculate the number of carbon credits that a given investment in a
    /// project would generate.  This is a pure view function — it does not
    /// modify any state.
    ///
    /// ## Calculation
    ///
    /// ```ignore
    /// credits = amount × project.green_impact / CARBON_UNIT
    /// ```
    ///
    /// where `CARBON_UNIT = 10_000_000_000`.
    /// - `amount` is the investment in micro-USDC (SAC format, 7 decimals).
    /// - `green_impact` is the project's 0–100 green-impact score.
    ///
    /// ## Examples
    ///
    /// | USDC invested | green_impact | Credits |
    /// |---------------|--------------|---------|
    /// | 1             | 100          | 0       |
    /// | 10            | 100          | 1       |
    /// | 500           | 60           | 30      |
    pub fn calculate_carbon_credits(env: Env, project_id: u32, amount: i128) -> CarbonCreditCalculation {
        let registry_addr: Address = env
            .storage()
            .instance()
            .get(&VaultKey::Registry)
            .unwrap();
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

    /// Issue carbon credits to an address.  Credits are calculated from the
    /// given `amount` of USDC invested in `project_id`.
    ///
    /// Anyone may call this (e.g. the vault owner, an investor, or a project
    /// owner) to record the carbon credits that a project investment generated.
    /// The credits are minted to the caller's balance.
    pub fn issue_carbon_credits(env: Env, to: Address, project_id: u32, amount: i128) -> i128 {
        let calc = Self::calculate_carbon_credits(env.clone(), project_id, amount);

        if calc.credits <= 0 {
            panic!("no carbon credits to issue");
        }

        let prev: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&VaultKey::CarbonCreditBalance(to.clone()), &(prev + calc.credits));

        calc.credits
    }

    /// Transfer carbon credits from one address to another.
    /// `from` must have a sufficient balance.
    pub fn transfer_carbon_credits(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        let prev_from: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(from.clone()))
            .unwrap_or(0);
        if prev_from < amount {
            panic!("insufficient carbon credits");
        }

        let prev_to: i128 = env
            .storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(to.clone()))
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&VaultKey::CarbonCreditBalance(from.clone()), &(prev_from - amount));
        env.storage()
            .persistent()
            .set(&VaultKey::CarbonCreditBalance(to.clone()), &(prev_to + amount));

        events::carbon_credits_transferred(&env, &from, &to, amount);
    }

    /// Return the carbon credit balance of an address.
    pub fn carbon_credit_balance(env: Env, address: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&VaultKey::CarbonCreditBalance(address))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Compliance / regulatory reporting
    // -----------------------------------------------------------------------

    /// Maximum number of compliance events retained on-chain.
    const MAX_COMPLIANCE_EVENTS: u64 = 1000;

    /// Set a maximum transaction amount for regulatory compliance.
    /// Zero (default) means no limit.
    #[only_owner]
    pub fn set_max_transaction_amount(env: Env, amount: i128) {
        if amount < 0 {
            panic!("amount must be non-negative");
        }
        env.storage()
            .instance()
            .set(&VaultKey::MaxTransactionAmount, &amount);
        events::max_transaction_amount_set(&env, amount);
    }

    /// Return the current max transaction amount (0 = no limit).
    pub fn max_transaction_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&VaultKey::MaxTransactionAmount)
            .unwrap_or(0)
    }

    /// Record a compliance event for audit trail purposes.
    /// Only callable by the contract owner.
    #[only_owner]
    pub fn record_compliance_event(env: Env, event_type: String, data: String) {
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

        env.storage()
            .persistent()
            .set(&VaultKey::ComplianceEvent(seq), &event);
        env.storage()
            .instance()
            .set(&VaultKey::ComplianceEventCounter, &seq);

        // Evict oldest events once the limit is exceeded
        if seq > Self::MAX_COMPLIANCE_EVENTS {
            let prune = seq - Self::MAX_COMPLIANCE_EVENTS;
            env.storage()
                .persistent()
                .remove(&VaultKey::ComplianceEvent(prune));
        }

        events::compliance_event_recorded(&env, seq, &event_type);
    }

    /// Return a specific compliance event by sequence number.
    /// Panics if the event does not exist.
    pub fn get_compliance_event(env: Env, seq: u64) -> ComplianceEventData {
        env.storage()
            .persistent()
            .get(&VaultKey::ComplianceEvent(seq))
            .unwrap_or_else(|| panic!("compliance event not found"))
    }

    /// Return a range of compliance events `[from, to]` (inclusive).
    /// Maximum 100 events per call to limit gas.
    /// If `from > to`, returns an empty vector.
    pub fn get_compliance_events(env: Env, from: u64, to: u64) -> Vec<ComplianceEventData> {
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

    /// Take a regulatory reporting snapshot of the vault's current state.
    /// Only callable by the contract owner.
    #[only_owner]
    pub fn take_reporting_snapshot(env: Env) {
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

    /// Return the latest reporting snapshot.
    /// Panics if no snapshot has been taken yet.
    pub fn get_latest_snapshot(env: Env) -> ReportingSnapshotData {
        env.storage()
            .instance()
            .get(&VaultKey::ReportingSnapshot)
            .unwrap_or_else(|| panic!("no snapshot taken"))
    }

    /// Export a comprehensive regulatory data package combining the latest
    /// snapshot with recent compliance events and key parameters.
    pub fn export_regulatory_data(env: Env) -> RegulatoryReport {
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

    // -----------------------------------------------------------------------
    // Wormhole bridge integration
    // -----------------------------------------------------------------------

    /// Set the Wormhole core contract address.
    /// This is the canonical Wormhole core bridge deployed on Stellar.
    /// Only callable by the contract owner.
    ///
    /// ## Security
    ///
    /// Setting this to a malicious contract would allow arbitrary minting.
    /// Verify the address against the official Wormhole contract registry.
    #[only_owner]
    pub fn set_wormhole_core(env: Env, core: Address) {
        env.storage().instance().set(&BridgeDataKey::WormholeCore, &core);
    }

    /// Add or remove a trusted emitter (a bridge contract on another chain
    /// allowed to mint HBS via Wormhole). Only owner.
    #[only_owner]
    pub fn set_trusted_emitter(
        env: Env,
        chain_id: u32,
        emitter_address: BytesN<32>,
        trusted: bool,
    ) {
        env.storage()
            .persistent()
            .set(&BridgeDataKey::TrustedEmitter(chain_id, emitter_address), &trusted);
    }

    /// Initiate an outbound bridge transfer of HBS to another chain.
    /// Burns `amount` HBS from `from` and publishes a Wormhole message.
    pub fn initiate_bridge_transfer(
        env: Env,
        from: Address,
        amount: i128,
        target_chain: u32,
        recipient: BytesN<32>,
        nonce: u64,
    ) -> u64 {
        from.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        Base::burn(&env, &from, amount);

        let token_address = wormhole::address_to_bytes32(&env, &env.current_contract_address());
        let payload = BridgeTransferPayload {
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
            .expect("Wormhole core not set");
        let client = WormholeCoreClient::new(&env, &core);
        let sequence = client.publish_message(&0u32, &payload_bytes);

        events::bridge_transfer_initiated(
            &env, &from, amount, target_chain, &recipient, sequence,
        );

        sequence
    }

    /// Complete an inbound bridge transfer.
    /// Verifies a Wormhole VAA and mints HBS to the recipient.
    pub fn complete_bridge_transfer(env: Env, vaa: Bytes) {
        let core: Address = env
            .storage()
            .instance()
            .get(&BridgeDataKey::WormholeCore)
            .expect("Wormhole core not set");
        let client = WormholeCoreClient::new(&env, &core);

        let parsed = client.verify_vaa(&vaa);

        let transfer = wormhole::parse_bridge_payload(&env, &parsed.payload);

        let trusted: bool = env
            .storage()
            .persistent()
            .get(&BridgeDataKey::TrustedEmitter(
                transfer.source_chain,
                parsed.emitter_address.clone(),
            ))
            .unwrap_or(false);
        if !trusted {
            panic!("emitter not trusted");
        }

        let digest: BytesN<32> = env.crypto().sha256(&vaa).into();
        if env
            .storage()
            .persistent()
            .has(&BridgeDataKey::ConsumedVaa(digest.clone()))
        {
            panic!("VAA already consumed");
        }
        env.storage()
            .persistent()
            .set(&BridgeDataKey::ConsumedVaa(digest), &true);

        let to = wormhole::bytes32_to_address(&env, &transfer.recipient);
        Base::mint(&env, &to, transfer.amount);

        events::bridge_transfer_completed(
            &env,
            transfer.source_chain,
            &parsed.emitter_address,
            &to,
            transfer.amount,
        );
    }
}

#[contractimpl(contracttrait)]
impl FungibleToken for InvestmentVault {
    type ContractType = Base;
}

#[contractimpl(contracttrait)]
impl FungibleBurnable for InvestmentVault {}

#[contractimpl(contracttrait)]
impl Ownable for InvestmentVault {}

#[cfg(test)]
mod test;
