use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Order, StdError, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read,
};

use terranames::auction::{
    seconds_from_deposit, deposit_from_seconds_ceil,
    deposit_from_seconds_floor,
};
use terranames::utils::{Timedelta, Timestamp};

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
    /// Timestamp from where transition delay is calculated from
    pub transition_reference_time: Timestamp,

    /// Amount of stablecoin per RATE_SEC_DENOM charged
    pub rate: Uint128,
    /// Timestamp when lease begun
    pub begin_time: Timestamp,
    /// Deposit when lease begun
    pub begin_deposit: Uint128,

    /// Previous owner
    pub previous_owner: Option<Addr>,
    /// Previous transition reference timestamp
    pub previous_transition_reference_time: Timestamp,
}

impl NameState {
    /// Return seconds spent since bid was won
    pub fn seconds_spent_since_bid(&self, current_time: Timestamp) -> Option<Timedelta> {
        current_time.checked_sub(self.begin_time).ok()
    }

    /// Return seconds spent since transition from previous owner
    pub fn seconds_spent_since_transition(&self, current_time: Timestamp) -> Option<Timedelta> {
        current_time.checked_sub(self.transition_reference_time).ok()
    }

    /// Return timestamp when counter delay ends
    pub fn counter_delay_end(&self, config: &Config) -> Timestamp {
        self.begin_time + config.counter_delay_secs
    }

    /// Return timestamp when transition delay ends
    pub fn transition_delay_end(&self, config: &Config) -> Timestamp {
        if self.transition_reference_time.is_zero() {
            // Special case for new bids
            self.begin_time
        } else {
            self.transition_reference_time + config.counter_delay_secs +
                config.transition_delay_secs
        }
    }

    /// Return timestamp when bid delay ends
    ///
    /// Note: There is no effective bid delay when the rate is zero.
    pub fn bid_delay_end(&self, config: &Config) -> Timestamp {
        let delay = if !self.rate.is_zero() {
            config.counter_delay_secs + config.bid_delay_secs
        } else {
            Timedelta::zero()
        };
        self.begin_time + delay
    }

    /// Return number of seconds since beginning that the deposit allows for
    pub fn max_seconds(&self) -> Option<Timedelta> {
        seconds_from_deposit(self.begin_deposit, self.rate)
    }

    /// Return timestamp when ownership expires
    pub fn expire_time(&self) -> Option<Timestamp> {
        self.max_seconds().map(|max_seconds| self.begin_time + max_seconds)
    }

    /// Return current remaining deposit
    pub fn current_deposit(&self, current_time: Timestamp) -> Uint128 {
        let seconds_spent = match self.seconds_spent_since_bid(current_time) {
            Some(seconds_spent) => seconds_spent,
            None => return Uint128::zero(),
        };
        let deposit_spent = deposit_from_seconds_ceil(seconds_spent, self.rate);
        self.begin_deposit - deposit_spent
    }

    /// Return max allowed deposit for the name
    pub fn max_allowed_deposit(&self, config: &Config, current_time: Timestamp) -> Uint128 {
        let seconds_spent = match self.seconds_spent_since_bid(current_time) {
            Some(seconds_spent) => seconds_spent,
            None => return Uint128::zero(),
        };
        let max_seconds_from_beginning = config.max_lease_secs + seconds_spent;
        deposit_from_seconds_floor(max_seconds_from_beginning, self.rate)
    }

    /// Return owner status
    pub fn owner_status(&self, config: &Config, current_time: Timestamp) -> OwnerStatus {
        let seconds_spent_since_bid = match self.seconds_spent_since_bid(current_time) {
            Some(seconds_spent) => seconds_spent,
            None => return OwnerStatus::Expired {
                expire_time: Timestamp::zero(),
                transition_reference_time: self.transition_reference_time,
            },
        };

        if let Some(max_seconds) = self.max_seconds() {
            if seconds_spent_since_bid >= max_seconds {
                return OwnerStatus::Expired {
                    expire_time: self.begin_time + max_seconds,
                    transition_reference_time: self.transition_reference_time,
                };
            }
        }

        let seconds_spent_since_transition = match self.seconds_spent_since_transition(current_time) {
            Some(seconds_spent) => seconds_spent,
            None => return OwnerStatus::Expired {
                expire_time: Timestamp::zero(),
                transition_reference_time: self.transition_reference_time,
            },
        };
        if seconds_spent_since_bid < config.counter_delay_secs {
            OwnerStatus::CounterDelay {
                name_owner: self.previous_owner.clone(),
                bid_owner: self.owner.clone(),
                transition_reference_time: self.previous_transition_reference_time,
            }
        } else if seconds_spent_since_transition < config.counter_delay_secs + config.transition_delay_secs {
            OwnerStatus::TransitionDelay {
                owner: self.owner.clone(),
                transition_reference_time: self.transition_reference_time,
            }
        } else {
            OwnerStatus::Valid {
                owner: self.owner.clone(),
                transition_reference_time: self.transition_reference_time,
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum OwnerStatus {
    CounterDelay {
        name_owner: Option<Addr>,
        bid_owner: Addr,
        transition_reference_time: Timestamp,
    },
    TransitionDelay {
        owner: Addr,
        transition_reference_time: Timestamp,
    },
    Valid {
        owner: Addr,
        transition_reference_time: Timestamp,
    },
    Expired {
        expire_time: Timestamp,
        transition_reference_time: Timestamp,
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
