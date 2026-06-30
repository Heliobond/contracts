use soroban_sdk::{Env, Address};
use crate::types::{DataKey, VaultConfig};

pub fn read_vault_config(env: &Env) -> Option<VaultConfig> {
    env.storage().instance().get(&DataKey::Config)
}

pub fn write_vault_config(env: &Env, config: &VaultConfig) {
    env.storage().instance().set(&DataKey::Config, config);
}
