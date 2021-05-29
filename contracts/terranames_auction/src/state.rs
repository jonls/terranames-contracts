use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Order, StdError, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read,
};

use terranames::auction::{blocks_from_deposit, deposit_from_blocks_floor};

pub static CONFIG_KEY: &[u8] = b"config";
pub static NAME_STATE_PREFIX: &[u8] = b"name";

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 30;

fn calc_range_start_str(start_after: Option<&str>) -> Option<Vec<u8>> {
    start_after.map(|s| {
        let mut v: Vec<u8> = s.into();
        v.push(0);
        v
    })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Collector of funds
    pub collector_addr: Addr,
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
pub struct NameState {
    /// Owner of the name
    pub owner: Addr,
    /// Controller of the name
    pub controller: Option<Addr>,
    /// Block height from where transition delay is calculated from
    pub transition_reference_block: u64,

    /// Amount of stablecoin per RATE_BLOCK_DENOM charged
    pub rate: Uint128,
    /// Block height when lease begun
    pub begin_block: u64,
    /// Deposit when lease begun
    pub begin_deposit: Uint128,

    /// Previous owner
    pub previous_owner: Option<Addr>,
    /// Previous transition reference block
    pub previous_transition_reference_block: u64,
}

impl NameState {
    /// Return blocks spent since bid was won
    pub fn blocks_spent_since_bid(&self, block_height: u64) -> Option<u64> {
        if block_height >= self.begin_block {
            Some(block_height - self.begin_block)
        } else {
            None
        }
    }

    /// Return blocks spent since transition from previous owner
    pub fn blocks_spent_since_transition(&self, block_height: u64) -> Option<u64> {
        if block_height >= self.transition_reference_block {
            Some(block_height - self.transition_reference_block)
        } else {
            None
        }
    }

    /// Return block when counter delay ends
    pub fn counter_delay_end(&self, config: &Config) -> u64 {
        self.begin_block + config.counter_delay_blocks
    }

    /// Return block when transition delay ends
    pub fn transition_delay_end(&self, config: &Config) -> u64 {
        if self.transition_reference_block == 0 {
            // Special case for new bids
            self.begin_block
        } else {
            self.transition_reference_block + config.counter_delay_blocks +
                config.transition_delay_blocks
        }
    }

    /// Return block when bid delay ends
    ///
    /// Note: There is no effective bid delay when the rate is zero.
    pub fn bid_delay_end(&self, config: &Config) -> u64 {
        let delay = if !self.rate.is_zero() {
            config.counter_delay_blocks + config.bid_delay_blocks
        } else {
            0
        };
        self.begin_block + delay
    }

    /// Return number of blocks since beginning that the deposit allows for
    pub fn max_blocks(&self) -> Option<u64> {
        blocks_from_deposit(self.begin_deposit, self.rate)
    }

    /// Return block when ownership expires
    pub fn expire_block(&self) -> Option<u64> {
        self.max_blocks().map(|max_blocks| max_blocks + self.begin_block)
    }

    /// Return max allowed deposit for the name
    pub fn max_allowed_deposit(&self, config: &Config, block_height: u64) -> Uint128 {
        let blocks_spent = match self.blocks_spent_since_bid(block_height) {
            Some(blocks_spent) => blocks_spent,
            None => return Uint128::zero(),
        };
        let max_blocks_from_beginning = config.max_lease_blocks + blocks_spent;
        deposit_from_blocks_floor(max_blocks_from_beginning, self.rate)
    }

    /// Return owner status
    pub fn owner_status(&self, config: &Config, block_height: u64) -> OwnerStatus {
        let blocks_spent_since_bid = match self.blocks_spent_since_bid(block_height) {
            Some(blocks_spent) => blocks_spent,
            None => return OwnerStatus::Expired {
                expire_block: 0,
                transition_reference_block: self.transition_reference_block,
            },
        };

        if let Some(max_blocks) = self.max_blocks() {
            if blocks_spent_since_bid >= max_blocks {
                return OwnerStatus::Expired {
                    expire_block: self.begin_block + max_blocks,
                    transition_reference_block: self.transition_reference_block,
                };
            }
        }

        let blocks_spent_since_transition = match self.blocks_spent_since_transition(block_height) {
            Some(blocks_spent) => blocks_spent,
            None => return OwnerStatus::Expired {
                expire_block: 0,
                transition_reference_block: self.transition_reference_block,
            },
        };
        if blocks_spent_since_bid < config.counter_delay_blocks {
            OwnerStatus::CounterDelay {
                name_owner: self.previous_owner.clone(),
                bid_owner: self.owner.clone(),
                transition_reference_block: self.previous_transition_reference_block,
            }
        } else if blocks_spent_since_transition < config.counter_delay_blocks + config.transition_delay_blocks {
            OwnerStatus::TransitionDelay {
                owner: self.owner.clone(),
                transition_reference_block: self.transition_reference_block,
            }
        } else {
            OwnerStatus::Valid {
                owner: self.owner.clone(),
                transition_reference_block: self.transition_reference_block,
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum OwnerStatus {
    CounterDelay {
        name_owner: Option<Addr>,
        bid_owner: Addr,
        transition_reference_block: u64,
    },
    TransitionDelay {
        owner: Addr,
        transition_reference_block: u64,
    },
    Valid {
        owner: Addr,
        transition_reference_block: u64,
    },
    Expired {
        expire_block: u64,
        transition_reference_block: u64,
    },
}

impl OwnerStatus {
    pub fn can_set_rate(&self, sender: &Addr) -> bool {
        match self {
            OwnerStatus::Valid { owner, .. } |
            OwnerStatus::TransitionDelay { owner, .. } => sender == owner,
            _ => false,
        }
    }

    pub fn can_transfer_name_owner(&self, sender: &Addr) -> bool {
        match self {
            OwnerStatus::Valid { owner, .. } |
            OwnerStatus::CounterDelay { name_owner: Some(owner), .. } |
            OwnerStatus::TransitionDelay { owner, .. } => sender == owner,
            _ => false,
        }
    }

    pub fn can_transfer_bid_owner(&self, sender: &Addr) -> bool {
        match self {
            OwnerStatus::CounterDelay { bid_owner, .. } => sender == bid_owner,
            _ => false,
        }
    }

    pub fn can_set_controller(&self, sender: &Addr) -> bool {
        match self {
            OwnerStatus::Valid { owner, .. } |
            OwnerStatus::CounterDelay { name_owner: Some(owner), .. } |
            OwnerStatus::TransitionDelay { owner, .. } => sender == owner,
            _ => false,
        }
    }
}

pub fn read_name_state(
    storage: &dyn Storage,
    name: &str,
) -> StdResult<NameState> {
    bucket_read(storage, NAME_STATE_PREFIX).load(name.as_bytes())
}

pub fn read_option_name_state(
    storage: &dyn Storage,
    name: &str,
) -> StdResult<Option<NameState>> {
    bucket_read(storage, NAME_STATE_PREFIX).may_load(name.as_bytes())
}

pub fn collect_name_states(
    storage: &dyn Storage,
    start_after: Option<&str>,
    limit: Option<u32>,
) -> StdResult<Vec<(String, NameState)>> {
    let bucket = bucket_read(storage, NAME_STATE_PREFIX);
    let start = calc_range_start_str(start_after);
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    bucket.range(start.as_deref(), None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (key, value) = item?;
            let key = String::from_utf8(key)
                .or_else(|_| Err(StdError::generic_err("Invalid utf-8")))?;
            Ok((key, value))
        })
        .collect()
}

#[must_use]
pub fn store_name_state(
    storage: &mut dyn Storage,
    name: &str,
    name_info: &NameState,
) -> StdResult<()> {
    bucket(storage, NAME_STATE_PREFIX).save(name.as_bytes(), name_info)
}
