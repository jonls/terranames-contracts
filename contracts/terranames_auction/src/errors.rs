use cosmwasm_std::{OverflowError, StdError, Uint128};
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
    #[snafu(display("Closed For Bids"))]
    ClosedForBids { backtrace: Option<snafu::Backtrace> },
    #[snafu(display("Bid rate too low (min {})", rate))]
    BidRateTooLow { rate: Uint128, backtrace: Option<snafu::Backtrace> },
    #[snafu(display("Bid deposit too low (min {})", deposit))]
    BidDepositTooLow { deposit: Uint128, backtrace: Option<snafu::Backtrace> },
    #[snafu(display("Bid has invalid interval"))]
    BidInvalidInterval { backtrace: Option<snafu::Backtrace> },
    #[snafu(display("Unexpected state"))]
    UnexpectedState { backtrace: Option<snafu::Backtrace> },
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
