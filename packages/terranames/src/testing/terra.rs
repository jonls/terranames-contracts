use std::collections::HashMap;
use std::iter::FromIterator;

use cosmwasm_std::{
    to_binary, Decimal, QuerierResult, QueryRequest, Uint128,
};
use terra_cosmwasm::{
    TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute,
};

// Mock querier for queries to Terra tax state
#[derive(Clone)]
pub struct TaxQuerier {
    pub rate: Decimal,
    pub caps: HashMap<String, Uint128>,
}

impl Default for TaxQuerier {
    fn default() -> Self {
        Self {
            rate: Decimal::from_ratio(405u128, 100_000u128),
            caps: caps_to_map(&[
                ("uabc", &Uint128::from(1_500_000u128)),
            ]),
        }
    }
}

impl TaxQuerier {
    pub fn new(rate: Decimal, caps: &[(&str, &Uint128)]) -> Self {
        TaxQuerier {
            rate,
            caps: caps_to_map(caps),
        }
    }
}

pub(crate) fn caps_to_map(caps: &[(&str, &Uint128)]) -> HashMap<String, Uint128> {
    HashMap::from_iter(
        caps.into_iter().map(|(denom, &value)| (denom.to_string(), value)),
    )
}

impl TaxQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> Option<QuerierResult> {
        let res = match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                match route {
                    TerraRoute::Treasury => {
                        match query_data {
                            TerraQuery::TaxRate {} => {
                                Ok(to_binary(&TaxRateResponse {
                                    rate: self.rate,
                                }))
                            },
                            TerraQuery::TaxCap { denom } => {
                                let cap = self.caps.get(denom).copied().unwrap_or_default();
                                Ok(to_binary(&TaxCapResponse { cap }))
                            },
                            _ => return None,
                        }
                    },
                    _ => return None,
                }
            },
            _ => return None,
        };
        Some(res)
    }
}
