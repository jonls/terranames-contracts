use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cw20::Cw20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Terraswap factory
    pub terraswap_factory: String,
    /// Terranames token
    pub terranames_token: String,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Minimum token price in stables (also used if the swap pool is empty)
    ///
    /// Tokens are not released to the swap pool at a lower implied price than
    /// this (in tokens/stablecoin).
    pub min_token_price: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    ConsumeExcessStable {},
    ConsumeExcessTokens {},
    Receive(Cw20ReceiveMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    /// Terranames token
    pub terranames_token: Addr,
    /// Terraswap pair
    pub terraswap_pair: Addr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Minimum token price in stables (also used if the swap pool is empty)
    ///
    /// Tokens are not released to the swap pool at a lower implied price than
    /// this (in stablecoin/token).
    pub min_token_price: Decimal,
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
