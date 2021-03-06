use cosmwasm_std::{Addr, Decimal, Fraction, StdResult, Storage, Uint128};
use cosmwasm_storage::{bucket, bucket_read, singleton, singleton_read};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use terranames::utils::{Timedelta, Timestamp};

pub static CONFIG_KEY: &[u8] = b"config";
pub static STATE_KEY: &[u8] = b"state";
pub static STAKE_STATE_PREFIX: &[u8] = b"stake";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Base token
    pub base_token: Addr,
    /// Stablecoin denomination
    pub stable_denom: String,
    /// Unstake delay
    pub unstake_delay: Timedelta,
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
pub struct State {
    /// Current multiplier
    pub multiplier: Decimal,
    /// Total tokens staked
    pub total_staked: Uint128,
    /// Residual funds
    pub residual: Uint128,
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, STATE_KEY).load()
}

#[must_use]
pub fn store_state(
    storage: &mut dyn Storage,
    state: &State,
) -> StdResult<()> {
    singleton(storage, STATE_KEY).save(state)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakeState {
    /// Staked amount
    pub staked_amount: Uint128,
    /// Unstaking amount
    pub unstaking_amount: Uint128,
    /// Unstaking begin time
    pub unstaking_begin_time: Option<Timestamp>,
    /// Pre-computed unstaked amount
    ///
    /// This does not include unstaked amount that has not yet been computed into state.
    pub unstaked_amount: Uint128,
    /// Initial multiplier
    pub multiplier: Decimal,
    /// Pre-computed dividend
    ///
    /// This does not include dividend that has not yet been computed into state.
    pub dividend: Uint128,
}

impl StakeState {
    /// Return full dividend
    pub fn dividend(&self, global_multiplier: Decimal) -> Uint128 {
        // Compute dividend since last state update
        let dividend_per_token_numer = global_multiplier.numerator().saturating_sub(
            self.multiplier.numerator()
        );
        let new_dividend = self.staked_amount.multiply_ratio(
            dividend_per_token_numer, Decimal::one().denominator(),
        );

        self.dividend + new_dividend
    }

    /// Update dividend in state
    pub fn update_dividend(&mut self, global_multiplier: Decimal) {
        self.dividend = self.dividend(global_multiplier);
        self.multiplier = global_multiplier;
    }

    /// Return full unstaked amount
    pub fn unstaking_unstaked_amount(&self, timestamp: Timestamp, unstake_delay: Timedelta) -> (Uint128, Uint128) {
        let (new_unstaking, add_unstaked) = if let Some(begin_time) = self.unstaking_begin_time {
            if begin_time + unstake_delay < timestamp {
                (Uint128::zero(), self.unstaking_amount)
            } else {
                (self.unstaking_amount, Uint128::zero())
            }
        } else {
            (Uint128::zero(), Uint128::zero())
        };

        (new_unstaking, self.unstaked_amount + add_unstaked)
    }

    /// Update unstaked amount in state
    pub fn update_unstaked_amount(&mut self, timestamp: Timestamp, unstake_delay: Timedelta) {
        if let Some(begin_time) = self.unstaking_begin_time {
            if begin_time + unstake_delay <= timestamp {
                self.unstaked_amount += self.unstaking_amount;
                self.unstaking_amount = Uint128::zero();
                self.unstaking_begin_time = None;
            }
        }
    }
}

pub fn read_stake_state(
    storage: &dyn Storage,
    address: &Addr
) -> StdResult<StakeState> {
    bucket_read(storage, STAKE_STATE_PREFIX)
        .load(address.as_ref().as_bytes())
}

pub fn read_option_stake_state(
    storage: &dyn Storage,
    address: &Addr,
) -> StdResult<Option<StakeState>> {
    bucket_read(storage, STAKE_STATE_PREFIX)
        .may_load(address.as_ref().as_bytes())
}

pub fn store_stake_state(
    storage: &mut dyn Storage,
    address: &Addr,
    stake_state: &StakeState,
) -> StdResult<()> {
    bucket(storage, STAKE_STATE_PREFIX).save(address.as_ref().as_bytes(), stake_state)
}
