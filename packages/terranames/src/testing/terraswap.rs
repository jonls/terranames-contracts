use cosmwasm_std::{
    from_binary, to_binary, Binary, HumanAddr, QuerierResult, QueryRequest,
    SystemError, Uint128, WasmQuery,
};
use terraswap::asset::{Asset, PairInfo};
use terraswap::factory::QueryMsg as FactoryQueryMsg;
use terraswap::pair::{QueryMsg as PairQueryMsg, PoolResponse};

/// Mock querier for terraswap contracts
#[derive(Clone)]
pub struct TerraswapQuerier {
    pub factory_addr: HumanAddr,
    pub pair: Option<PairInfo>,
    pub pair_total_share: Uint128,
    pub pair_1_amount: Uint128,
    pub pair_2_amount: Uint128,
}

impl TerraswapQuerier {
    pub fn new(factory_addr: HumanAddr) -> Self {
        Self {
            factory_addr,
            pair: None,
            pair_total_share: Uint128::zero(),
            pair_1_amount: Uint128::zero(),
            pair_2_amount: Uint128::zero(),
        }
    }

    pub fn handle_query<T>(&self, request: &QueryRequest<T>) -> Option<QuerierResult> {
        let res = match &request {
            QueryRequest::Wasm(WasmQuery::Smart { msg, contract_addr }) => {
                if contract_addr == &self.factory_addr {
                    self.handle_factory_query(msg)?
                } else if let Some(ref pair) = self.pair {
                    if contract_addr == &pair.contract_addr {
                        self.handle_pair_query(msg)?
                    } else if contract_addr == &pair.liquidity_token {
                        unimplemented!()
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            },
            _ => return None,
        };
        Some(res)
    }

    fn handle_factory_query(&self, msg: &Binary) -> Option<QuerierResult> {
        match from_binary(msg).unwrap() {
            FactoryQueryMsg::Pair { asset_infos } => {
                if let Some(ref pair) = self.pair {
                    if asset_infos == pair.asset_infos ||
                            (&asset_infos[1], &asset_infos[0]) == (&pair.asset_infos[0], &pair.asset_infos[1]) {
                        return Some(Ok(to_binary(&pair)));
                    }
                }

                return Some(Err(SystemError::InvalidRequest {
                    error: "Mock terraswap registry does not contain this pair".to_string(),
                    request: msg.as_slice().into(),
                }));
            },
            _ => unimplemented!(),
        };
    }

    fn handle_pair_query(&self, msg: &Binary) -> Option<QuerierResult> {
        let res = match from_binary(msg).unwrap() {
            PairQueryMsg::Pool {} => {
                if let Some(ref pair) = self.pair {
                    Ok(to_binary(&PoolResponse {
                        assets: [
                            Asset {
                                info: pair.asset_infos[0].clone(),
                                amount: Uint128::zero(),
                            },
                            Asset {
                                info: pair.asset_infos[1].clone(),
                                amount: Uint128::zero(),
                            },
                        ],
                        total_share: Uint128::from(1_000_000u64),
                    }))
                } else {
                    return Some(Err(SystemError::InvalidRequest {
                        error: "Mock terraswap registry doe not contain this pair".to_string(),
                        request: msg.as_slice().into(),
                    }));
                }
            },
            _ => unimplemented!(),
        };
        Some(res)
    }
}