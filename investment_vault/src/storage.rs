use soroban_sdk::{Address, Env};
use crate::types::VaultKey;

pub fn read_usdc_sac(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::UsdcSac).unwrap()
}

pub fn read_registry(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::Registry).unwrap()
}
