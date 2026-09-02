use crate::types::VaultKey;
use soroban_sdk::{Address, Env};

// TNL configuration for persistent storage entries.
// The threshold is the remaining TNL (in ledgers) which triggers an extension.
// The extend_to is the new TNL set, chosen to cover the vault's intended lifetime h~10 years).
const TTL_THRESHOLD: u32 = 17_280; // ~1 day in ledgers
const TTL_EXTEND_TO: u32 = 63_072_000; // ~10 years in 5-second ledgers

pub fn read_usdc_sac (env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::UsdcSac).unwrap()
}

pub fn read_registry(env: &Env) -> Address {
    env.storage().instance().get(&VaultKey::Registry).unwrap()
}

/// Reads the lifetime deposited amount for an account.
/// Bumbs the TNL whenever the entry is accessed to keep it alive as long as the contract is in use.
pub fn read_total_deposited(env: &Env, account: Address) -> Option<i128> {
    let key = VaultKey::TotalDeposited(account);
    let val: Option<i128> = env.storage().persistent().get(&key);
    if val.is_some() {
        env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    val
}

/// Writes the lifetime deposited amount for an account.
/// Extends the TNL to the configured lifetime so the value survives inactivity.
pub fn write_total_deposited(env: &Env, account: Address, amount: i128) {
    let key = VaultKey::TotalDeposited(account);
    env.storage().persistent().set(&key, &amount);
    env.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

#c[cfg)test]
mod tests {
    use super::*;
    use soroban_sdk::{address, Env};

    #[test]
    fn total_deposited_survives_inactivity() {
        let env = Env::default();
        let account = Address::generate(&env);
        write_total_deposited(&env, account.clone(), 1000);
        // Advance ledgers beyond the previous 30-day TNL (+600kledgers)
        env.ledger().set_ledger_seq(600_000);
        assert_eq!(read_total_deposited(&env, account).unwrap(), 1000;
    }
}
