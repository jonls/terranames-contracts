use cosmwasm_std::{to_binary, HumanAddr, Querier, QueryRequest, StdResult, WasmQuery};

use crate::auction::{QueryMsg as AuctionQueryMsg, NameStateResponse};

pub fn query_name_state<Q: Querier>(
    querier: &Q,
    auction_contract: &HumanAddr,
    name: &str,
) -> StdResult<NameStateResponse> {
    querier.query::<NameStateResponse>(
        &QueryRequest::Wasm(
            WasmQuery::Smart {
                contract_addr: auction_contract.clone(),
                msg: to_binary(
                    &AuctionQueryMsg::GetNameState {
                        name: name.to_string(),
                    }
                )?,
            },
        )
    )
}
