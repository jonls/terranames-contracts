use cosmwasm_std::{HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Rate is provided as number of stablecoins per this number of blocks
pub const RATE_BLOCK_DENOM: u64 = 1_000_000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    /// Collector of funds
    pub collector_addr: HumanAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Minimum number of blocks to allow bidding for
    pub min_lease_blocks: u64,
    /// Maximum number of blocks to allow bidding for at once
    pub max_lease_blocks: u64,
    /// Number of blocks to allow counter-bids
    pub counter_delay_blocks: u64,
    /// Number of transition delay blocks after successful counter-bid
    pub transition_delay_blocks: u64,
    /// Number of blocks until a new bid can start
    pub bid_delay_blocks: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
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
        owner: HumanAddr,
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
        to: HumanAddr,
    },
    SetNameController {
        /// Name to set controller for
        name: String,
        /// New controller (someone who can set values only)
        controller: HumanAddr,
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
    pub collector_addr: HumanAddr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Minimum number of blocks to allow bidding for
    pub min_lease_blocks: u64,
    /// Maximum number of blocks to allow bidding for at once
    pub max_lease_blocks: u64,
    /// Number of blocks to allow counter-bids
    pub counter_delay_blocks: u64,
    /// Number of transition delay blocks after successful counter-bid
    pub transition_delay_blocks: u64,
    /// Number of blocks until a new bid can start
    pub bid_delay_blocks: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NameStateResponse {
    /// Owner of the name
    pub owner: HumanAddr,
    /// Controller of the name
    pub controller: Option<HumanAddr>,

    /// Amount of stablecoin per RATE_BLOCK_DENOM charged
    pub rate: Uint128,
    /// Block height when lease begun
    pub begin_block: u64,
    /// Deposit when lease begun
    pub begin_deposit: Uint128,

    /// Previous owner
    pub previous_owner: Option<HumanAddr>,

    /// Counter-delay block end
    pub counter_delay_end: u64,
    /// Transition-delay block end
    pub transition_delay_end: u64,
    /// Bid-delay block end
    pub bid_delay_end: u64,
    /// Expire block
    pub expire_block: Option<u64>,
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

/// Return deposit needed for blocks and rate rounded down.
///
/// Rounded down to nearest raw unit (e.g. to 1 uusd NOT 1 whole usd).
pub fn deposit_from_blocks_floor(blocks: u64, rate: Uint128) -> Uint128 {
    rate.multiply_ratio(blocks, RATE_BLOCK_DENOM)
}

/// Return deposit needed for blocks and rate rounded up.
///
/// Rounded up to nearest raw unit (e.g. to 1 uusd NOT 1 whole usd).
pub fn deposit_from_blocks_ceil(blocks: u64, rate: Uint128) -> Uint128 {
    let a = blocks as u128 * rate.u128() + RATE_BLOCK_DENOM as u128 - 1;
    Uint128::from(1u64).multiply_ratio(a, RATE_BLOCK_DENOM)
}

/// Return number of blocks corresponding to deposit and rate
pub fn blocks_from_deposit(deposit: Uint128, rate: Uint128) -> Option<u64> {
    if rate.is_zero() {
        None
    } else {
        Some((deposit.multiply_ratio(RATE_BLOCK_DENOM, rate)).u128() as u64)
    }
}
