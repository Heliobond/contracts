use crate::types::{DataKey, ProjectData, Proposal};
use soroban_sdk::{Address, Env};

#[allow(dead_code)]
pub fn read_project(env: &Env, id: u32) -> Option<ProjectData> {
    env.storage().persistent().get(&DataKey::Project(id))
}

#[allow(dead_code)]
pub fn write_project(env: &Env, id: u32, project: &ProjectData) {
    env.storage()
        .persistent()
        .set(&DataKey::Project(id), project);
}

#[allow(dead_code)]
pub fn read_proposal(env: &Env, id: u32) -> Option<Proposal> {
    env.storage().persistent().get(&DataKey::Proposal(id))
}

#[allow(dead_code)]
pub fn write_proposal(env: &Env, id: u32, proposal: &Proposal) {
    env.storage()
        .persistent()
        .set(&DataKey::Proposal(id), proposal);
}

#[allow(dead_code)]
pub fn read_whitelist(env: &Env, account: Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Whitelist(account))
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn write_whitelist(env: &Env, account: Address, status: bool) {
    env.storage()
        .persistent()
        .set(&DataKey::Whitelist(account), &status);
}
