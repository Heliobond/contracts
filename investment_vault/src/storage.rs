use crate::types::VaultKey;
use soroban_sdk::{Address, Env};

// TTL configuration for persistent storage entries.
// The threshold is the remaining TTL (in ledgers) which triggers an extension.
// The extend_to is the new TTL set, chosen to cover the vault's intended lifetime (~10 years).
const TTL_THRESHOLD: u32 = crate::TTL_EXTEND_THRESHOLD_LEDGERS; // ~1 day in ledgers
const TTL_EXTEND_TO: u32 = crate::TTL_EXTEND_TO_LEDGERS; // ~10 years in 5-second ledgers

pub fn read_usdc_sac(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::UsdcSac).unwrap()
}

pub fn read_registry(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::Registry).unwrap()
}

/// Reads the lifetime deposited amount for an account.
/// Bumps the TTL whenever the entry is accessed to keep it alive as long as the contract is in use.
pub fn read_total_deposited(env: &Env, account: Address) -> Option<i128> {
    let key = VaultKey::TotalDeposited(account);
    let val: Option<i128> = env.storage().persistent().get(&key);
    if val.is_some() {
        env.storage().persistent().extend_ttl(
            &key,
            TTL_THRESHOLD,
            TTL_EXTEND_TO.min(env.storage().max_ttl()),
        );
    }
    val
}

/// Writes the lifetime deposited amount for an account.
/// Extends the TTL to the configured lifetime so the value survives inactivity.
pub fn write_total_deposited(env: &Env, account: Address, amount: i128) {
    let key = VaultKey::TotalDeposited(account);
    env.storage().persistent().set(&key, &amount);
    env.storage().persistent().extend_ttl(
        &key,
        TTL_THRESHOLD,
        TTL_EXTEND_TO.min(env.storage().max_ttl()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[soroban_sdk::contract]
    struct StorageTestContract;
    #[soroban_sdk::contractimpl]
    impl StorageTestContract {}

    #[test]
    fn total_deposited_survives_inactivity() {
        let env = Env::default();
        // Storage is only reachable from inside a contract context.
        let contract_id = env.register(StorageTestContract, ());
        let account = Address::generate(&env);
        env.as_contract(&contract_id, || {
            write_total_deposited(&env, account.clone(), 1000);
        });
        // Advance ledgers well past the old ~30-day TTL (+600k ledgers).
        env.ledger().set_sequence_number(600_000);
        env.as_contract(&contract_id, || {
            assert_eq!(read_total_deposited(&env, account).unwrap(), 1000);
        });
    }
}
