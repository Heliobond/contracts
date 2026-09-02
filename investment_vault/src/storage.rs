use crate::types::VaultKey;
use soroban_sdk::{Address, Env, IntoVal, Val};

/// Minimum remaining TTL in ledgers before extending persistent storage rent (#317).
/// At 5 s/ledger this equals ~1 day (17 280 ledgers).
pub(crate) const TTL_EXTEND_THRESHOLD_LEDGERS: u32 = 17_280;

/// Target TTL in ledgers after extension (#317).
/// At 5 s/ledger this equals ~30 days (518 400 ledgers).
pub(crate) const TTL_EXTEND_TO_LEDGERS: u32 = 518_400;

#[allow(dead_code)]
pub fn read_usdc_sac(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::UsdcSac).unwrap()
}

#[allow(dead_code)]
pub fn read_registry(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::Registry).unwrap()
}

/// Write a persistent entry and refresh its rent so idle vaults do not
/// archive yield, queue, insurance, or carbon-credit state (#317).
///
/// Soroban does not auto-extend TTL on write beyond the network minimum;
/// every persistent `.set` in this contract must go through this helper.
pub fn set_persistent<K, V>(env: &Env, key: &K, val: &V)
where
    K: IntoVal<Env, Val>,
    V: IntoVal<Env, Val>,
{
    env.storage().persistent().set(key, val);
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_EXTEND_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
}
