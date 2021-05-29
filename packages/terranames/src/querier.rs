use cosmwasm_std::{to_binary, Addr, QuerierWrapper, StdResult, WasmQuery};

use crate::auction::{QueryMsg as AuctionQueryMsg, NameStateResponse};

pub fn query_name_state(
    querier: &QuerierWrapper,
    auction_contract: &Addr,
    name: &str,
) -> StdResult<NameStateResponse> {
    let msg = AuctionQueryMsg::GetNameState {
        name: name.into(),
    };
    let query = WasmQuery::Smart {
        contract_addr: auction_contract.into(),
        msg: to_binary(&msg)?,
    }.into();
    querier.query(&query)
}
