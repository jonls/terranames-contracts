use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, StdResult, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read};

pub static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Terranames token
    pub terranames_token: CanonicalAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Terraswap pair
    pub terraswap_pair: CanonicalAddr,
    /// Initial token price (if the swap pool is empty)
    pub init_token_price: Uint128,
    /// Tokens left from the initial allocation
    pub initial_tokens_left: Uint128,
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
