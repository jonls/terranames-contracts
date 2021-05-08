use std::collections::HashMap;

use cosmwasm_std::{
    from_binary, to_binary, HumanAddr, QuerierResult, QueryRequest, Uint128,
    WasmQuery,
};
use cw20::{Cw20QueryMsg, BalanceResponse};

/// Mock querier for cw20 contracts
#[derive(Clone)]
pub struct Cw20Querier {
    pub token_addr: HumanAddr,
    pub balances: HashMap<HumanAddr, Uint128>,
}

impl Cw20Querier {
    pub fn new(token_addr: HumanAddr) -> Self {
        Self {
            token_addr,
            balances: HashMap::new(),
        }
    }

    pub fn handle_query<T>(&self, request: &QueryRequest<T>) -> Option<QuerierResult> {
        let res = match &request {
            QueryRequest::Wasm(WasmQuery::Smart { msg, contract_addr }) => {
                if contract_addr == &self.token_addr {
                    match from_binary(msg).unwrap() {
                        Cw20QueryMsg::Balance { address } => {
                            let balance = self.balances.get(&address).cloned().unwrap_or_default();
                            Ok(to_binary(&BalanceResponse { balance }))
                        },
                        _ => unimplemented!(),
                    }
                } else {
                    return None;
                }
            },
            _ => return None,
        };
        Some(res)
    }
}
