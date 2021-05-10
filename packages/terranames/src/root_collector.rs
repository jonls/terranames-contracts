use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cw20::Cw20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InitMsg {
    /// Terraswap factory
    pub terraswap_factory: HumanAddr,
    /// Terranames token
    pub terranames_token: HumanAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Initial token price in stables (used if the swap pool is empty)
    pub init_token_price: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    ConsumeExcessStable {},
    ConsumeExcessTokens {},
    Receive(Cw20ReceiveMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    /// Terranames token
    pub terranames_token: HumanAddr,
    /// Terraswap pair
    pub terraswap_pair: HumanAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Initial token price in stables (used if the swap pool is empty)
    pub init_token_price: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StateResponse {
    /// Number of tokens in initial release pool
    pub initial_token_pool: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    AcceptInitialTokens {},
}
