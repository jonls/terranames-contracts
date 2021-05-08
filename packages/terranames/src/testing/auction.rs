use cosmwasm_std::{
    from_binary, to_binary, QuerierResult, QueryRequest, SystemError, WasmQuery,
};

use crate::auction::{NameStateResponse, QueryMsg};

/// Mock querier for the auction contract
#[derive(Clone)]
pub struct AuctionQuerier {
    pub response: Option<NameStateResponse>,
}

impl AuctionQuerier {
    pub fn new() -> Self {
        Self {
            response: None,
        }
    }
}

impl AuctionQuerier {
    pub fn handle_query<T>(&self, request: &QueryRequest<T>) -> Option<QuerierResult> {
        let res = match &request {
            QueryRequest::Wasm(WasmQuery::Smart { msg, .. }) => {
                match from_binary(&msg).unwrap() {
                    QueryMsg::GetNameState { .. } => {
                        match &self.response {
                            Some(response) => Ok(to_binary(response)),
                            None => Err(SystemError::InvalidRequest {
                                error: "Mock auction querier does not contain a response".to_string(),
                                request: msg.as_slice().into(),
                            })
                        }

                    },
                    _ => unimplemented!(),
                }
            },
            _ => return None,
        };
        Some(res)
    }
}
