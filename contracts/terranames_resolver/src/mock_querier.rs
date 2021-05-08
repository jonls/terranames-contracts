use cosmwasm_std::{
    from_slice, Coin, Empty, Extern, HumanAddr, Querier, QuerierResult,
    QueryRequest, SystemError,
};
use cosmwasm_std::testing::{
    MockApi, MockQuerier as CosmMockQuerier, MockStorage, MOCK_CONTRACT_ADDR,
};

use terranames::testing::auction::AuctionQuerier;

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
    pub auction_querier: AuctionQuerier,
    pub base_querier: CosmMockQuerier<Empty>,
}

impl MockQuerier {
    pub fn new() -> MockQuerier {
        MockQuerier {
            auction_querier: AuctionQuerier::new(),
            base_querier: CosmMockQuerier::new(&[]),
        }
    }

    pub fn with_base_querier(mut self, base_querier: CosmMockQuerier<Empty>) -> Self {
        self.base_querier = base_querier;
        self
    }

    pub fn with_auction_querier(mut self, wasm_querier: AuctionQuerier) -> Self {
        self.auction_querier = wasm_querier;
        self
    }

    fn handle_query(&self, request: QueryRequest<Empty>) -> QuerierResult {
        if let Some(res) = self.auction_querier.handle_query(&request) {
            return res;
        }
        self.base_querier.handle_query(&request)
    }
}

impl Querier for MockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
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
