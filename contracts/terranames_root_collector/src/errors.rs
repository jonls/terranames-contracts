use cosmwasm_std::{OverflowError, StdError};
use snafu::Snafu;

#[derive(Snafu, Debug)]
#[snafu(visibility = "pub(crate)")]
pub enum ContractError {
    #[snafu(display("StdError: {}", source))]
    Std { source: StdError },
    #[snafu(display("Overflow: {}", source))]
    Overflow { source: OverflowError },
    #[snafu(display("Unauthorized"))]
    Unauthorized { backtrace: Option<snafu::Backtrace> },
    #[snafu(display("Unfunded"))]
    Unfunded { backtrace: Option<snafu::Backtrace> },
    #[snafu(display("Invalid Config"))]
    InvalidConfig { backtrace: Option<snafu::Backtrace> },
}

impl From<StdError> for ContractError {
    fn from(source: StdError) -> Self {
        ContractError::Std { source }
    }
}

impl From<OverflowError> for ContractError {
    fn from(source: OverflowError) -> Self {
        ContractError::Overflow { source }
    }
}
