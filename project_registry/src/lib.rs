#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, String};

mod events;
mod types;

pub use types::{DataKey, ProjectData};

#[contract]
pub struct ProjectRegistry;

#[contractimpl]
impl ProjectRegistry {
    pub fn initialize(env: Env, admin: Address, whitelister: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Whitelister, &whitelister);
        env.storage().instance().set(&DataKey::ProjectCounter, &0u32);
    }

    pub fn set_whitelist(env: Env, account: Address, status: bool) {
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        whitelister.require_auth();
        env.storage().persistent().set(&DataKey::Whitelist(account.clone()), &status);
        events::whitelist_set(&env, &account, status);
    }

    pub fn create_project(env: Env, creator: Address, uri: String) -> u32 {
        creator.require_auth();
        let is_whitelisted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Whitelist(creator.clone()))
            .unwrap_or(false);
        if !is_whitelisted {
            panic!("not whitelisted");
        }

        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        let project_id = counter + 1;

        let project = ProjectData {
            owner: creator.clone(),
            uri: uri.clone(),
            credit_quality: 0,
            green_impact: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        env.storage()
            .instance()
            .set(&DataKey::ProjectCounter, &project_id);

        events::project_created(&env, project_id, &creator, &uri);

        project_id
    }

    pub fn get_project(env: Env, id: u32) -> ProjectData {
        env.storage()
            .persistent()
            .get(&DataKey::Project(id))
            .unwrap_or_else(|| panic!("project not found"))
    }

    pub fn total_projects(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
