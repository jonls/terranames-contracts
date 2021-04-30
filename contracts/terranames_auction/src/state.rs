use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read,
};

use terranames::auction::{blocks_from_deposit, deposit_from_blocks_floor};

pub static CONFIG_KEY: &[u8] = b"config";
pub static NAME_STATE_PREFIX: &[u8] = b"name";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Collector of funds
    pub collector_addr: CanonicalAddr,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NameState {
    /// Owner of the name
    pub owner: CanonicalAddr,
    /// Controller of the name
    pub controller: CanonicalAddr,
    /// Block height from where transition delay is calculated from
    pub transition_reference_block: u64,

    /// Amount of stablecoin per RATE_BLOCK_DENOM charged
    pub rate: Uint128,
    /// Block height when lease begun
    pub begin_block: u64,
    /// Deposit when lease begun
    pub begin_deposit: Uint128,

    /// Previous owner
    pub previous_owner: CanonicalAddr,
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
            None => return OwnerStatus::Expired { expire_block: 0 },
        };

        if let Some(max_blocks) = self.max_blocks() {
            if blocks_spent_since_bid >= max_blocks {
                return OwnerStatus::Expired { expire_block: self.begin_block + max_blocks };
            }
        }

        let blocks_spent_since_transition = match self.blocks_spent_since_transition(block_height) {
            Some(blocks_spent) => blocks_spent,
            None => return OwnerStatus::Expired { expire_block: 0 },
        };
        if blocks_spent_since_bid < config.counter_delay_blocks {
            OwnerStatus::CounterDelay {
                name_owner: self.previous_owner.clone(),
                bid_owner: self.owner.clone(),
            }
        } else if blocks_spent_since_transition < config.counter_delay_blocks + config.transition_delay_blocks {
            OwnerStatus::TransitionDelay {
                owner: self.owner.clone(),
            }
        } else {
            OwnerStatus::Valid {
                owner: self.owner.clone(),
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum OwnerStatus {
    CounterDelay { name_owner: CanonicalAddr, bid_owner: CanonicalAddr },
    TransitionDelay { owner: CanonicalAddr },
    Valid { owner: CanonicalAddr },
    Expired { expire_block: u64 },
}

impl OwnerStatus {
    pub fn can_set_rate(&self, sender: &CanonicalAddr) -> bool {
        match self {
            OwnerStatus::Valid { owner } |
            OwnerStatus::TransitionDelay { owner } => sender == owner,
            _ => false,
        }
    }

    pub fn can_transfer_name_owner(&self, sender: &CanonicalAddr) -> bool {
        match self {
            OwnerStatus::Valid { owner } |
            OwnerStatus::CounterDelay { name_owner: owner, .. } |
            OwnerStatus::TransitionDelay { owner } => sender == owner,
            _ => false,
        }
    }

    pub fn can_transfer_bid_owner(&self, sender: &CanonicalAddr) -> bool {
        match self {
            OwnerStatus::CounterDelay { bid_owner, .. } => sender == bid_owner,
            _ => false,
        }
    }

    pub fn can_set_controller(&self, sender: &CanonicalAddr) -> bool {
        match self {
            OwnerStatus::Valid { owner } |
            OwnerStatus::CounterDelay { name_owner: owner, .. } |
            OwnerStatus::TransitionDelay { owner } => sender == owner,
            _ => false,
        }
    }
}

pub fn read_name_state<S: Storage>(
    storage: &S,
    name: &str,
) -> StdResult<NameState> {
    bucket_read(NAME_STATE_PREFIX, storage).load(name.as_bytes())
}

pub fn read_option_name_state<S: Storage>(
    storage: &S,
    name: &str,
) -> StdResult<Option<NameState>> {
    bucket_read(NAME_STATE_PREFIX, storage).may_load(name.as_bytes())
}

#[must_use]
pub fn store_name_state<S: Storage>(
    storage: &mut S,
    name: &str,
    name_info: &NameState,
) -> StdResult<()> {
    bucket(NAME_STATE_PREFIX, storage).save(name.as_bytes(), name_info)
}
