use crate::types::VaultKey;
use soroban_sdk::{Address, Env};

pub fn read_usdc_sac(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::UsdcSac).unwrap()
}

#[allow(dead_code)]
pub fn read_registry(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::Registry).unwrap()
}
