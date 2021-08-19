use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cw20::Cw20ReceiveMsg;

use crate::utils::{Timedelta, Timestamp};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Base token
    pub base_token: String,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Unstake delay
    pub unstake_delay: Timedelta,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    StakeState {
        /// Address to query stake state for
        address: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Deposit {},
    UnstakeTokens {
        /// Amount to unstake
        amount: Uint128,
    },
    WithdrawTokens {
        /// Amount to withdraw
        amount: Uint128,
        /// Address to withdraw to
        to: Option<String>,
    },
    WithdrawDividends {
        /// Address to withdraw to
        to: Option<String>,
    },
    Receive(Cw20ReceiveMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    /// Base token
    pub base_token: Addr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Unstake delay
    pub unstake_delay: Timedelta,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StateResponse {
    /// Current multiplier
    pub multiplier: Decimal,
    /// Total tokens staked
    pub total_staked: Uint128,
    /// Residual funds
    pub residual: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StakeStateResponse {
    /// Staked amount
    pub staked_amount: Uint128,
    /// Unstaking amount
    pub unstaking_amount: Uint128,
    /// Unstaked amount
    pub unstaked_amount: Uint128,
    /// Unstake time
    pub unstake_time: Option<Timestamp>,
    /// Initial multiplier
    pub multiplier: Decimal,
    /// Dividend
    pub dividend: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveMsg {
    Stake {},
}
