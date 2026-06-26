#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;

/// Maximum URI length in bytes. Prevents excessively large ledger entries (#114).
const MAX_URI_LEN: u32 = 512;
/// Minimum URI length — must contain at least a scheme and one character (#117).
const MIN_URI_LEN: u32 = 8;

mod events;
mod types;

pub use types::{DataKey, ProjectData};

#[contract]
pub struct ProjectRegistry;

#[contractimpl]
impl ProjectRegistry {
    pub fn __constructor(env: Env, admin: Address, whitelister: Address) {
        set_owner(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Whitelister, &whitelister);
        env.storage()
            .instance()
            .set(&DataKey::ProjectCounter, &0u32);
    }

    pub fn set_whitelist(env: Env, account: Address, status: bool) {
        let whitelister: Address = env.storage().instance().get(&DataKey::Whitelister).unwrap();
        whitelister.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Whitelist(account.clone()), &status);
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
        // URI validation (#117, #114)
        let uri_len = uri.len();
        if uri_len < MIN_URI_LEN {
            panic!("uri too short");
        }
        if uri_len > MAX_URI_LEN {
            panic!("uri too long");
        }

        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        if counter == u32::MAX {
            panic!("project limit reached");
        }
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

    #[only_owner]
    pub fn update_impact_score(env: Env, project_id: u32, credit_quality: u32, green_impact: u32) {
        if credit_quality > 100 || green_impact > 100 {
            panic!("scores must be 0-100");
        }
        let mut project: ProjectData = env
            .storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .unwrap_or_else(|| panic!("project {} not found", project_id));

        project.credit_quality = credit_quality;
        project.green_impact = green_impact;
        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        events::project_updated(&env, project_id, credit_quality, green_impact);
    }

    pub fn get_all_projects(env: Env) -> Vec<(u32, ProjectData)> {
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProjectCounter)
            .unwrap_or(0);
        let mut result = Vec::new(&env);
        for i in 1..=counter {
            if let Some(project) = env
                .storage()
                .persistent()
                .get::<DataKey, ProjectData>(&DataKey::Project(i))
            {
                result.push_back((i, project));
            }
        }
        result
    }
}

#[contractimpl(contracttrait)]
impl Ownable for ProjectRegistry {}

#[cfg(test)]
mod test;
