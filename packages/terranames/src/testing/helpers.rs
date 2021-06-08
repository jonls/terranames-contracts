use cosmwasm_std::{Env, Timestamp};

/// Helper trait for modifying Env
pub trait EnvBuilder {
    fn at_time(self, timestamp: u64) -> Self;
}

impl EnvBuilder for Env {
    /// Set block time for Env
    fn at_time(mut self, timestamp: u64) -> Self {
        self.block.time = Timestamp::from_seconds(timestamp).into();
        self
    }
}
