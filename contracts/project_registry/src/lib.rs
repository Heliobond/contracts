#![no_std]
#![allow(dependency_on_unit_never_type_fallback)]

use soroban_sdk::{
    contract,
    contracterror,
    contractevent,
    contractimpl,
    contracttype,
    panic_with_error,
    Address,
    Bytes,
    BytesN,
    Env,
};

const COMMIT_REVEAL_TIMEOUT_SECONDS: u64 = 3600;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidReveal = 4,
    CommitNotFound = 5,
    CommitExpired = 6,
}

#[contracttype]
enum InstanceKey {
    Initialized,
    Admin,
    Whitelister,
    ProjectOwner(BytesN<32>),
    WhitelistStatus(BytesN<32>),
    ScoreCommit(BytesN<32>),
    ImpactScore(BytesN<32>),
}

#[contracttype]
pub struct ScoreCommit {
    pub oracle: Address,
    pub hash: BytesN<32>,
    pub deadline: u64,
}

#[contracttype]
pub struct ImpactScore {
    pub credit_quality: i32,
    pub green_impact: u32,
}

#[contractevent(topics = ["project_id"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectCreated {
    #[topic]
    pub project_id: BytesN<32>,
    pub owner: Address,
}

#[contractevent(topics = ["project_id", "oracle"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectUpdated {
    #[topic]
    pub project_id: BytesN<32>,
    #[topic]
    pub oracle: Address,
    pub credit_quality: i32,
    pub green_impact: u32,
}

#[contractevent(topics = ["project_id", "whitelister"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhitelistSet {
    #[topic]
    pub project_id: BytesN<32>,
    #[topic]
    pub whitelister: Address,
    pub whitelisted: bool,
}

#[contract]
pub struct ProjectRegistry;

#[contractimpl]
impl ProjectRegistry {
    pub fn initialize(env: Env, admin: Address, whitelister: Address) {
        admin.require_auth();
        if env.storage().instance().has(&InstanceKey::Initialized) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&InstanceKey::Initialized, &true);
        env.storage().instance().set(&InstanceKey::Admin, &admin);
        env.storage().instance().set(&InstanceKey::Whitelister, &whitelister);
    }

    pub fn set_whitelister(env: Env, admin: Address, whitelister: Address) {
        admin.require_auth();
        env.storage().instance().set(&InstanceKey::Whitelister, &whitelister);
    }

    pub fn set_whitelist(env: Env, caller: Address, project_id: BytesN<32>, whitelisted: bool) {
        caller.require_auth();
        let whitelister: Address = env
            .storage()
            .instance()
            .get(&InstanceKey::Whitelister)
            .unwrap_or_else(|| panic_with_error!(&env, Error::Unauthorized));
        if caller != whitelister {
            panic_with_error!(&env, Error::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&InstanceKey::WhitelistStatus(project_id.clone()), &whitelisted);
        env.events().publish(WhitelistSet {
            project_id,
            whitelister: caller,
            whitelisted,
        });
    }

    pub fn commit_update_impact_score(
        env: Env,
        oracle: Address,
        project_id: BytesN<32>,
        commit_hash: BytesN<32>,
    ) {
        oracle.require_auth();
        let deadline = env.ledger().timestamp() + COMMIT_REVEAL_TIMEOUT_SECONDS;
        env.storage()
            .instance()
            .set(
                &InstanceKey::ScoreCommit(project_id),
                &ScoreCommit {
                    oracle,
                    hash: commit_hash,
                    deadline,
                },
            );
    }

    pub fn update_impact_score(
        env: Env,
        oracle: Address,
        project_id: BytesN<32>,
        credit_quality: i32,
        green_impact: u32,
        salt: BytesN<32>,
    ) {
        oracle.require_auth();
        let commit: ScoreCommit = env
            .storage()
            .instance()
            .get(&InstanceKey::ScoreCommit(project_id.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, Error::CommitNotFound));
        if env.ledger().timestamp() > commit.deadline {
            panic_with_error!(&env, Error::CommitExpired);
        }
        if commit.oracle != oracle {
            panic_with_error!(&env, Error::Unauthorized);
        }
        let expected = Self::score_commit_hash(&env, &project_id, credit_quality, green_impact, &salt);
        if commit.hash != expected {
            panic_with_error!(&env, Error::InvalidReveal);
        }
        env.storage()
            .instance()
            .set(
                &InstanceKey::ImpactScore(project_id.clone()),
                &ImpactScore {
                    credit_quality,
                    green_impact,
                },
            );
        env.storage().instance().remove(&InstanceKey::ScoreCommit(project_id.clone()));
        env.events().publish(ProjectUpdated {
            project_id,
            oracle,
            credit_quality,
            green_impact,
        });
    }

    fn score_commit_hash(
        env: &Env,
        project_id: &BytesN<32>,
        credit_quality: i32,
        green_impact: u32,
        salt: &BytesN<32>,
    ) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.append(&Bytes::from_slice(env, project_id.as_ref()));
        buf.append(&Bytes::from_slice(env, &credit_quality.to_be_bytes()));
        buf.append(&Bytes::from_slice(env, &green_impact.to_be_bytes()));
        buf.append(&Bytes::from_slice(env, salt.as_ref()));
        env.crypto().sha256(&buf).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{MockAuth, MockAuthInvoke}, Address, Env, IntoVal};

    fn setup() -> (Env, Address, ProjectRegistryClient<'static>, Address, Address, Address, BytesN<32>) {
        let env = Env::default();
        let contract_id = env.register_contract(None, ProjectRegistry);
        let client = ProjectRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let whitelister = Address::generate(&env);
        let unauthorized_user = Address::generate(&env);
        let project_id = BytesN::from_array(&env, &[1u8; 32]);
        client.initialize(&admin, &whitelister);
        (env, contract_id, client, admin, whitelister, unauthorized_user, project_id)
    }

    #[test]
    fn test_set_whitelist_non_whitelister_panics() {
        let (env, contract_id, client, _admin, _whitelister, unauthorized_user, project_id) = setup();
        env.mock_auths(&[MockAuth {
            address: &unauthorized_user,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "set_whitelist",
                args: (&unauthorized_user, &project_id, &true).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        let result = client.try_set_whitelist(&unauthorized_user, &project_id, &true);
        assert!(result.is_err());
    }
}
