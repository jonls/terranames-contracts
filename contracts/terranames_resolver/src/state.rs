use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read,
};

pub static CONFIG_KEY: &[u8] = b"config";
pub static VALUE_PREFIX: &[u8] = b"value";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Auction contract
    pub auction_contract: CanonicalAddr,
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

pub fn read_name_value<S: Storage>(
    storage: &S,
    name: &str,
) -> StdResult<Option<String>> {
    bucket_read(VALUE_PREFIX, storage).load(name.as_bytes())
}

#[must_use]
pub fn store_name_value<S: Storage>(
    storage: &mut S,
    name: &str,
    value: Option<String>,
) -> StdResult<()> {
    bucket(VALUE_PREFIX, storage).save(name.as_bytes(), &value)
}
