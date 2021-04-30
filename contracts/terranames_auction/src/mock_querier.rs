use std::collections::HashMap;
use std::iter::FromIterator;

use cosmwasm_std::{
    from_slice, to_binary, Coin, Decimal, Extern, HumanAddr, Querier, QuerierResult,
    QueryRequest, SystemError, Uint128,
};
use cosmwasm_std::testing::{
    MockApi, MockQuerier as CosmMockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use terra_cosmwasm::{
    TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute,
};

pub fn mock_dependencies(
    canonical_length: usize,
    contract_balance: &[Coin],
) -> Extern<MockStorage, MockApi, MockQuerier> {
    let contract_addr = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let querier: MockQuerier = MockQuerier::new()
            .with_base_querier(CosmMockQuerier::new(&[(&contract_addr, contract_balance)]));

    Extern {
        storage: MockStorage::default(),
        api: MockApi::new(canonical_length),
        querier: querier,
    }
}

pub struct MockQuerier {
    tax_querier: TaxQuerier,
    base_querier: CosmMockQuerier<TerraQueryWrapper>,
}

impl MockQuerier {
    pub fn new() -> MockQuerier {
        MockQuerier {
            tax_querier: TaxQuerier::default(),
            base_querier: CosmMockQuerier::new(&[]),
        }
    }

    pub fn with_base_querier(mut self, base_querier: CosmMockQuerier<TerraQueryWrapper>) -> Self {
        self.base_querier = base_querier;
        self
    }

    pub fn with_tax_querier(mut self, tax_querier: TaxQuerier) -> Self {
        self.tax_querier = tax_querier;
        self
    }

    fn handle_query(&self, request: QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        if let Some(res) = self.tax_querier.handle_query(&request) {
            return res;
        }
        self.base_querier.handle_query(&request)
    }
}

impl Querier for MockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                });
            }
        };
        self.handle_query(request)
    }
}

#[derive(Clone)]
pub struct TaxQuerier {
    rate: Decimal,
    caps: HashMap<String, Uint128>,
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
    fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> Option<QuerierResult> {
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
