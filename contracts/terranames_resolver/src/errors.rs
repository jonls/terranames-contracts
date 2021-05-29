use cosmwasm_std::StdError;
use snafu::Snafu;

#[derive(Snafu, Debug)]
#[snafu(visibility = "pub(crate)")]
pub enum ContractError {
    #[snafu(display("StdError: {}", source))]
    Std { source: StdError },
    #[snafu(display("NameExpired"))]
    NameExpired { backtrace: Option<snafu::Backtrace> },
    #[snafu(display("Unauthorized"))]
    Unauthorized { backtrace: Option<snafu::Backtrace> },
}

impl From<StdError> for ContractError {
    fn from(source: StdError) -> Self {
        ContractError::Std { source }
    }
}
