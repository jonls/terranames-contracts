use cosmwasm_std::{
    from_slice, Coin, Extern, HumanAddr, Querier, QuerierResult,
    QueryRequest, SystemError,
};
use cosmwasm_std::testing::{
    MockApi, MockQuerier as CosmMockQuerier, MockStorage, MOCK_CONTRACT_ADDR,
};
use terranames::testing::cw20::Cw20Querier;
use terranames::testing::terra::TaxQuerier;
use terranames::testing::terraswap::TerraswapQuerier;
use terra_cosmwasm::TerraQueryWrapper;

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
    pub tax_querier: TaxQuerier,
    pub terraswap_querier: TerraswapQuerier,
    pub terranames_token_querier: Cw20Querier,
    pub base_querier: CosmMockQuerier<TerraQueryWrapper>,
}

impl MockQuerier {
    pub fn new() -> MockQuerier {
        MockQuerier {
            tax_querier: TaxQuerier::default(),
            terraswap_querier: TerraswapQuerier::new("terraswap_factory".into()),
            terranames_token_querier: Cw20Querier::new("token_contract".into()),
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
        } else if let Some(res) = self.terraswap_querier.handle_query(&request) {
            return res;
        } else if let Some(res) = self.terranames_token_querier.handle_query(&request) {
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