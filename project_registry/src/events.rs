use soroban_sdk::{Address, Env, String, symbol_short};

pub fn project_created(env: &Env, project_id: u32, owner: &Address, uri: &String) {
    env.events().publish(
        (symbol_short!("registry"), symbol_short!("created")),
        (project_id, owner.clone(), uri.clone()),
    );
}

pub fn project_updated(env: &Env, project_id: u32, credit_quality: u32, green_impact: u32) {
    env.events().publish(
        (symbol_short!("registry"), symbol_short!("updated")),
        (project_id, credit_quality, green_impact),
    );
}

pub fn whitelist_set(env: &Env, account: &Address, status: bool) {
    env.events().publish(
        (symbol_short!("registry"), symbol_short!("whitelist")),
        (account.clone(), status),
    );
}
