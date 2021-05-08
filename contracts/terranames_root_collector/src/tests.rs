use cosmwasm_std::{
    coins, from_binary, to_binary, CosmosMsg, Decimal, HumanAddr, Uint128,
    WasmMsg,
};
use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use terranames::collector::AcceptFunds;
use terranames::root_collector::{
    ConfigResponse, HandleMsg, InitMsg, QueryMsg, ReceiveMsg, StateResponse,
};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::HandleMsg as PairHandleMsg;

use crate::contract::{handle, init, query};
use crate::mock_querier::mock_dependencies;

static ABC_COIN: &str = "uabc";

fn default_init() -> InitMsg {
    InitMsg {
        terraswap_factory: HumanAddr::from("terraswap_factory"),
        terranames_token: HumanAddr::from("token_contract"),
        stable_denom: ABC_COIN.into(),
        init_token_price: Decimal::from_ratio(1u64, 10u64),
    }
}

fn default_pair_info() -> PairInfo {
    PairInfo {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: ("token_contract").into(),
            },
            AssetInfo::NativeToken {
                denom: ABC_COIN.into(),
            },
        ],
        contract_addr: "token_stable_pair".into(),
        liquidity_token: "lp_token".into(),
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // it worked, let's query the config
    let res = query(&deps, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(config.terranames_token.as_str(), "token_contract");
    assert_eq!(config.terraswap_pair.as_str(), "token_stable_pair");
    assert_eq!(config.stable_denom, "uabc");
    assert_eq!(config.init_token_price, Decimal::from_ratio(10u64, 100u64));

    let res = query(&deps, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool, Uint128::zero());
}

#[test]
fn init_fails_without_swap_pair() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg);
    assert_eq!(res.is_err(), true);
}

#[test]
fn provide_initial_token_pool() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Initialize token funds
    let initial_token_amount = 1_000_000_000;
    deps.querier.terranames_token_querier.balances.insert(
        MOCK_CONTRACT_ADDR.into(), Uint128::from(initial_token_amount),
    );
    let env = mock_env("token_contract", &[]);
    let res = handle(&mut deps, env, HandleMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(initial_token_amount),
        sender: "governor".into(),
        msg: Some(to_binary(&ReceiveMsg::AcceptInitialTokens { }).unwrap()),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool.u128(), initial_token_amount);
}

#[test]
fn accept_funds_releases_tokens() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Initialize token funds
    let initial_token_amount = 1_000_000_000;
    deps.querier.terranames_token_querier.balances.insert(
        MOCK_CONTRACT_ADDR.into(), Uint128::from(initial_token_amount),
    );
    let env = mock_env("token_contract", &[]);
    let res = handle(&mut deps, env, HandleMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(initial_token_amount),
        sender: "governor".into(),
        msg: Some(to_binary(&ReceiveMsg::AcceptInitialTokens { }).unwrap()),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Let the collector accept funds from the auction
    let deposit = 1_491_362;
    let tax_amount = 6016;
    let expected_tokens_released = 14_853_460;

    let env = mock_env("auction_contract", &coins(deposit, ABC_COIN));
    let res = handle(&mut deps, env, HandleMsg::AcceptFunds(AcceptFunds {
        source_addr: "source".into(),
    })).unwrap();
    assert_eq!(res.messages.len(), 2);

    let increase_allowance_message = &res.messages[0];
    match increase_allowance_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_contract");
            assert_eq!(send, &[]);

            match from_binary(&msg).unwrap() {
                Cw20HandleMsg::IncreaseAllowance { spender, amount, expires } => {
                    assert_eq!(spender.as_str(), "token_stable_pair");
                    assert_eq!(amount.u128(), expected_tokens_released);
                    assert_eq!(expires, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", increase_allowance_message),
    }

    let provide_liquidity_message = &res.messages[1];
    match provide_liquidity_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_stable_pair");
            assert_eq!(send, &coins(deposit - tax_amount, ABC_COIN));

            match from_binary(&msg).unwrap() {
                PairHandleMsg::ProvideLiquidity { assets, slippage_tolerance } => {
                    assert_eq!(assets[0], Asset {
                        info: AssetInfo::Token {
                            contract_addr: "token_contract".into(),
                        },
                        amount: Uint128::from(expected_tokens_released),
                    });
                    assert_eq!(assets[1], Asset {
                        info: AssetInfo::NativeToken {
                            denom: ABC_COIN.into(),
                        },
                        amount: Uint128::from(deposit - tax_amount),
                    });
                    assert_eq!(slippage_tolerance, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", provide_liquidity_message),
    }

    let res = query(&deps, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool.u128(), initial_token_amount - expected_tokens_released);
}
