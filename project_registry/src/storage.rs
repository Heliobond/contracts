use crate::types::{DataKey, ProjectData, Proposal};
use soroban_sdk::{Address, Env, IntoVal, Val};

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
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(id), proposal);
}

pub fn read_whitelist(env: &Env, account: Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Whitelist(account))
        .unwrap_or(false)
}

pub fn write_whitelist(env: &Env, account: Address, status: bool) {
    env.storage()
        .persistent()
        .set(&DataKey::Whitelist(account), &status);
}
