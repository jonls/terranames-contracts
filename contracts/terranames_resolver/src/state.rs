use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, StdResult, Storage};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read,
};

pub static CONFIG_KEY: &[u8] = b"config";
pub static VALUE_PREFIX: &[u8] = b"value";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Auction contract
    pub auction_contract: Addr,
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

pub fn read_name_value(
    storage: &dyn Storage,
    name: &str,
) -> StdResult<Option<String>> {
    bucket_read(storage, VALUE_PREFIX).load(name.as_bytes())
}

#[must_use]
pub fn store_name_value(
    storage: &mut dyn Storage,
    name: &str,
    value: Option<String>,
) -> StdResult<()> {
    bucket(storage, VALUE_PREFIX).save(name.as_bytes(), &value)
}
