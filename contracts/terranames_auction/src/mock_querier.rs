use cosmwasm_std::{
    from_slice, Coin, OwnedDeps, Querier, QuerierResult, QueryRequest,
    SystemError, SystemResult,
};
use cosmwasm_std::testing::{
    MockApi, MockQuerier as CosmMockQuerier, MockStorage, MOCK_CONTRACT_ADDR,
};
use terra_cosmwasm::{
    TerraQueryWrapper,
};
use terranames::testing::terra::TaxQuerier;

pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, MockQuerier> {
    let contract_addr = MOCK_CONTRACT_ADDR;
    let querier: MockQuerier = MockQuerier::new()
            .with_base_querier(CosmMockQuerier::new(&[(&contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: querier,
    }
}

pub struct MockQuerier {
    pub tax_querier: TaxQuerier,
    pub base_querier: CosmMockQuerier<TerraQueryWrapper>,
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
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                });
            }
        };
        self.handle_query(request)
    }
}
