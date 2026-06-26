#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, MuxedAddress, String};
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

pub use types::VaultKey;
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
