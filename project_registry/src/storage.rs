use soroban_sdk::{Address, Env};
use crate::types::{DataKey, ProjectData, Proposal};

pub fn read_project(env: &Env, id: u32) -> Option<ProjectData> {
    env.storage().persistent().get(&DataKey::Project(id))
}

pub fn write_project(env: &Env, id: u32, project: &ProjectData) {
    env.storage().persistent().set(&DataKey::Project(id), project);
}

pub fn read_proposal(env: &Env, id: u32) -> Option<Proposal> {
    env.storage().persistent().get(&DataKey::Proposal(id))
}

pub fn write_proposal(env: &Env, id: u32, proposal: &Proposal) {
    env.storage().persistent().set(&DataKey::Proposal(id), proposal);
}

pub fn read_whitelist(env: &Env, account: Address) -> bool {
    env.storage().persistent().get(&DataKey::Whitelist(account)).unwrap_or(false)
}

pub fn write_whitelist(env: &Env, account: Address, status: bool) {
    env.storage().persistent().set(&DataKey::Whitelist(account), &status);
}
