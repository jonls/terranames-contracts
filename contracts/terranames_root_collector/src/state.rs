use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static CONFIG_KEY: &[u8] = b"config";
pub static STATE_KEY: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Terranames token
    pub terranames_token: Addr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Terraswap pair
    pub terraswap_pair: Addr,
    /// Minimum token price in stables (also used if the swap pool is empty)
    ///
    /// Tokens are not released to the swap pool at a lower implied price than
    /// this (in stablecoin/token).
    pub min_token_price: Decimal,
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, CONFIG_KEY).load()
}

#[must_use]
pub fn store_config(
    storage: &mut dyn Storage,
    config: &Config,
) -> StdResult<()> {
    singleton(storage, CONFIG_KEY).save(config)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    /// Number of tokens in initial release pool
    pub initial_token_pool: Uint128,
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, STATE_KEY).load()
}

#[must_use]
pub fn store_state(
    storage: &mut dyn Storage,
    state: &State,
) -> StdResult<()> {
    singleton(storage, STATE_KEY).save(state)
}
