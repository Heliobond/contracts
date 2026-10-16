use crate::types::{DataKey, ProjectData, Proposal, TimelockEntry, WhitelistEntry};
use soroban_sdk::{Address, BytesN, Env, IntoVal, Vec, Val};

/// Minimum remaining TTL in ledgers before extending persistent storage rent (#388).
pub(crate) const TTL_EXTEND_THRESHOLD_LEDGERS: u32 = 17_280;
/// Target TTL in ledgers after extension (#388).
pub(crate) const TTL_EXTEND_TO_LEDGERS: u32 = 518_400;

pub fn read_project(env: &Env, id: u32) -> Option<ProjectData> {
    env.storage().persistent().get(&DataKey::Project(id))
}

/// Writes `project` and re-extends its persistent TTL (#328) — every
/// mutating call site goes through this so rent is refreshed on each write,
/// not just at creation.
pub fn write_project(env: &Env, id: u32, project: &ProjectData) {
    write_persistent(env, &DataKey::Project(id), project);
}

/// Writes any persistent `key` and re-extends its TTL (#552). Persistent
/// writes should go through this (or a typed wrapper like `write_project`)
/// rather than an inline `.persistent().set(..)`, so no key is left to
/// silently expire — e.g. the compacted `Arch` record or `HasVoted` guards.
pub fn write_persistent<V: IntoVal<Env, Val>>(env: &Env, key: &DataKey, val: &V) {
    env.storage().persistent().set(key, val);
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_EXTEND_THRESHOLD_LEDGERS, TTL_EXTEND_TO_LEDGERS);
}

pub fn read_proposal(env: &Env, id: u32) -> Option<Proposal> {
    env.storage().persistent().get(&DataKey::Proposal(id))
}

pub fn write_proposal(env: &Env, id: u32, proposal: &Proposal) {
    write_persistent(env, &DataKey::Proposal(id), proposal);
}

pub fn read_whitelist(env: &Env, account: Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Whitelist(account))
        .unwrap_or(false)
}

/// Read whitelist entry with expiry (#752).
pub fn read_whitelist_entry(env: &Env, account: Address) -> Option<WhitelistEntry> {
    env.storage()
        .persistent()
        .get(&DataKey::WhitelistExpiry(account))
}

/// Write whitelist entry with expiry (#752).
pub fn write_whitelist_entry(env: &Env, account: Address, entry: &WhitelistEntry) {
    write_persistent(env, &DataKey::WhitelistExpiry(account), entry);
    // Also update the legacy bool key for backward compatibility
    env.storage()
        .persistent()
        .set(&DataKey::Whitelist(account), &entry.status);
}

pub fn write_whitelist(env: &Env, account: Address, status: bool) {
    write_persistent(env, &DataKey::Whitelist(account), &status);
}

// ── Timelock storage (#749) ──────────────────────────────────────────────

/// Read a timelock entry by ID.
pub fn read_timelock(env: &Env, id: u32) -> Option<TimelockEntry> {
    env.storage()
        .instance()
        .get(&DataKey::TimelockQueue)
        .map(|q: soroban_sdk::Map<u32, TimelockEntry>| q.get(id))
        .flatten()
}

/// Write/update a timelock entry.
pub fn write_timelock(env: &Env, id: u32, entry: &TimelockEntry) {
    let mut queue: soroban_sdk::Map<u32, TimelockEntry> = env
        .storage()
        .instance()
        .get(&DataKey::TimelockQueue)
        .unwrap_or_else(|| soroban_sdk::Map::new(env));
    queue.set(id, entry.clone());
    env.storage().instance().set(&DataKey::TimelockQueue, &queue);
}

/// Read all timelock entries.
pub fn read_timelock_queue(env: &Env) -> soroban_sdk::Map<u32, TimelockEntry> {
    env.storage()
        .instance()
        .get(&DataKey::TimelockQueue)
        .unwrap_or_else(|| soroban_sdk::Map::new(env))
}

/// Get next timelock ID.
pub fn next_timelock_id(env: &Env) -> u32 {
    let counter: u32 = env
        .storage()
        .instance()
        .get(&DataKey::TimelockCounter)
        .unwrap_or(0);
    env.storage().instance().set(&DataKey::TimelockCounter, &(counter + 1));
    counter + 1
}
