use cosmwasm_std::{CanonicalAddr, Decimal, StdResult, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static CONFIG_KEY: &[u8] = b"config";
pub static STATE_KEY: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Terranames token
    pub terranames_token: CanonicalAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Terraswap pair
    pub terraswap_pair: CanonicalAddr,
    /// Initial token price in stables (used if the swap pool is empty)
    pub init_token_price: Decimal,
}

pub fn read_config<S: Storage>(storage: &S) -> StdResult<Config> {
    singleton_read(storage, CONFIG_KEY).load()
}

#[must_use]
pub fn store_config<S: Storage>(
    storage: &mut S,
    config: &Config,
) -> StdResult<()> {
    singleton(storage, CONFIG_KEY).save(config)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    /// Number of tokens in initial release pool
    pub initial_token_pool: Uint128,
}

pub fn read_state<S: Storage>(storage: &S) -> StdResult<State> {
    singleton_read(storage, STATE_KEY).load()
}

#[must_use]
pub fn store_state<S: Storage>(
    storage: &mut S,
    state: &State,
) -> StdResult<()> {
    singleton(storage, STATE_KEY).save(state)
}
