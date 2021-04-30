use cosmwasm_std::{HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RootCollectorInitMsg {
    /// Terraswap registry
    pub terraswap_registry: HumanAddr,
    /// Terranames token
    pub terranames_token: HumanAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Initial token price (if the swap pool is empty)
    pub init_token_price: Uint128,
    /// Tokens left from the initial allocation
    pub initial_tokens_left: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AcceptFunds {
    pub denom: String,
    pub source_addr: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    AcceptFunds(AcceptFunds),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RootCollectorQueryMsg {
    Config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RootCollectorHandleMsg {
    AcceptFunds(AcceptFunds),
    BurnExcess {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct RootCollectorConfigResponse {
    /// Terranames token
    pub terranames_token: HumanAddr,
    /// Terraswap pair
    pub terraswap_pair: HumanAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Initial token price (if the swap pool is empty)
    pub init_token_price: Uint128,
    /// Tokens left from the initial allocation
    pub initial_tokens_left: Uint128,
}
