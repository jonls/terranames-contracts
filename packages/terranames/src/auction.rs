use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::utils::{Timedelta, Timestamp};

/// Rate is provided as number of stablecoins per day
pub const RATE_SEC_DENOM: Timedelta = Timedelta::from_seconds(24 * 60 * 60);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Collector of funds
    pub collector_addr: String,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Minimum number of seconds to allow bidding for
    pub min_lease_secs: Timedelta,
    /// Maximum number of seconds to allow bidding for at once
    pub max_lease_secs: Timedelta,
    /// Number of seconds to allow counter-bids
    pub counter_delay_secs: Timedelta,
    /// Number of transition delay seconds after successful counter-bid
    pub transition_delay_secs: Timedelta,
    /// Number of seconds until a new bid can start
    pub bid_delay_secs: Timedelta,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    BidName {
        /// Name to bid on
        name: String,
        /// Amount of stablecoin to bid for full the full interval length
        rate: Uint128,
    },
    FundName {
        /// Name to fund
        name: String,
        /// Current owner (fails if this is not the owner)
        owner: String,
    },
    SetNameRate {
        /// Name to change rate of
        name: String,
        /// Rate to change to
        rate: Uint128,
    },
    TransferNameOwner {
        /// Name to transfer
        name: String,
        /// Destination to transfer ownership of name to
        to: String,
    },
    SetNameController {
        /// Name to set controller for
        name: String,
        /// New controller (someone who can set values only)
        controller: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    GetNameState {
        /// Name to obtain state for
        name: String,
    },
    GetAllNameStates {
        /// Start after (for pagination)
        start_after: Option<String>,
        /// Number of values to return
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Collector of funds
    pub collector_addr: Addr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Minimum number of seconds to allow bidding for
    pub min_lease_secs: Timedelta,
    /// Maximum number of seconds to allow bidding for at once
    pub max_lease_secs: Timedelta,
    /// Number of seconds to allow counter-bids
    pub counter_delay_secs: Timedelta,
    /// Number of transition delay seconds after successful counter-bid
    pub transition_delay_secs: Timedelta,
    /// Number of seconds until a new bid can start
    pub bid_delay_secs: Timedelta,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NameStateResponse {
    /// Owner of the name
    pub name_owner: Option<Addr>,
    /// Owner of the current highest bid
    pub bid_owner: Option<Addr>,
    /// Controller of the name
    pub controller: Option<Addr>,

    /// Amount of stablecoin per RATE_SEC_DENOM charged
    pub rate: Uint128,
    /// Timestamp in seconds when lease begun
    pub begin_time: Timestamp,
    /// Deposit when lease begun
    pub begin_deposit: Uint128,
    /// Deposit now
    pub current_deposit: Uint128,

    /// Counter-delay end timestamp
    pub counter_delay_end: Timestamp,
    /// Transition-delay end timestamp
    pub transition_delay_end: Timestamp,
    /// Bid-delay end timestamp
    pub bid_delay_end: Timestamp,
    /// Expire timestamp
    pub expire_time: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NameStateItem {
    pub name: String,
    pub state: NameStateResponse,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllNameStatesResponse {
    pub names: Vec<NameStateItem>,
}

/// Return deposit needed for seconds and rate rounded down.
///
/// Rounded down to nearest raw unit (e.g. to 1 uusd NOT 1 whole usd).
pub fn deposit_from_seconds_floor(seconds: Timedelta, rate: Uint128) -> Uint128 {
    rate.multiply_ratio(seconds, RATE_SEC_DENOM)
}

/// Return deposit needed for seconds and rate rounded up.
///
/// Rounded up to nearest raw unit (e.g. to 1 uusd NOT 1 whole usd).
pub fn deposit_from_seconds_ceil(seconds: Timedelta, rate: Uint128) -> Uint128 {
    let a: u128 = (seconds.value() as u128) * rate.u128() + (RATE_SEC_DENOM.value() as u128) - 1;
    Uint128::from(1u64).multiply_ratio(a, RATE_SEC_DENOM)
}

/// Return number of seconds corresponding to deposit and rate
pub fn seconds_from_deposit(deposit: Uint128, rate: Uint128) -> Option<Timedelta> {
    if rate.is_zero() {
        None
    } else {
        Some(Timedelta::from_seconds(deposit.multiply_ratio(RATE_SEC_DENOM, rate).u128() as u64))
    }
}
