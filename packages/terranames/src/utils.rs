use std::ops::Add;

use cosmwasm_std::{Timestamp as CWTimestamp, OverflowError, Uint64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Timestamp (in seconds)
#[derive(
    Serialize, Deserialize, Copy, Clone, Default, Debug, PartialEq, Eq,
    PartialOrd, Ord, JsonSchema,
)]
pub struct Timestamp(Uint64);

impl Timestamp {
    pub const fn from_seconds(seconds: u64) -> Timestamp {
        Timestamp(Uint64::new(seconds))
    }

    pub const fn zero() -> Timestamp {
        Timestamp(Uint64::zero())
    }

    pub const fn value(&self) -> u64 {
        self.0.u64()
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn checked_add(self, other: Timedelta) -> Result<Timestamp, OverflowError> {
        self.0.checked_add(other.0).map(Timestamp)
    }

    pub fn checked_sub(self, other: Self) -> Result<Timedelta, OverflowError> {
        self.0.checked_sub(other.0).map(Timedelta)
    }
}

impl Add<Timedelta> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Timedelta) -> Self::Output {
        Timestamp(self.0 + rhs.0)
    }
}

impl From<Timestamp> for u64 {
    fn from(other: Timestamp) -> u64 {
        other.value()
    }
}

impl From<Timestamp> for CWTimestamp {
    fn from(other: Timestamp) -> CWTimestamp {
        CWTimestamp::from_seconds(other.value())
    }
}

/// Timedelta (in seconds)
#[derive(
    Serialize, Deserialize, Copy, Clone, Default, Debug, PartialEq, Eq,
    PartialOrd, Ord, JsonSchema,
)]
pub struct Timedelta(Uint64);

impl Timedelta {
    pub const fn from_seconds(seconds: u64) -> Timedelta {
        Timedelta(Uint64::new(seconds))
    }

    pub const fn zero() -> Timedelta {
        Timedelta(Uint64::zero())
    }

    pub const fn value(&self) -> u64 {
        self.0.u64()
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl Add for Timedelta {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Timedelta(self.0 + rhs.0)
    }
}

impl From<Timedelta> for u64 {
    fn from(other: Timedelta) -> u64 {
        other.value()
    }
}

impl From<Timedelta> for u128 {
    fn from(other: Timedelta) -> u128 {
        other.value() as u128
    }
}

/// Implement Into<Timestamp> for cosmwasm Timestamp
impl From<CWTimestamp> for Timestamp {
    fn from(other: CWTimestamp) -> Timestamp {
        Timestamp::from_seconds(other.nanos() / 1_000_000_000)
    }
}
