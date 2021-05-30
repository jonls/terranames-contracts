use cosmwasm_std::{
    coins, from_binary, to_binary, Addr, CosmosMsg, Decimal, Uint128,
    WasmMsg,
};
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use terranames::root_collector::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg, StateResponse,
};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::ExecuteMsg as PairExecuteMsg;

use crate::contract::{execute, instantiate, query};
use crate::errors::ContractError;
use crate::mock_querier::mock_dependencies;

static ABC_COIN: &str = "uabc";

fn default_init() -> InstantiateMsg {
    InstantiateMsg {
        terraswap_factory: "terraswap_factory".into(),
        terranames_token: "token_contract".into(),
        stable_denom: ABC_COIN.into(),
        min_token_price: Decimal::from_ratio(1u64, 10u64),
    }
}

fn default_pair_info() -> PairInfo {
    PairInfo {
        asset_infos: [
            AssetInfo::Token {
                contract_addr: Addr::unchecked("token_contract"),
            },
            AssetInfo::NativeToken {
                denom: ABC_COIN.into(),
            },
        ],
        contract_addr: Addr::unchecked("token_stable_pair"),
        liquidity_token: Addr::unchecked("lp_token"),
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // it worked, let's query the config
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(config.terranames_token.as_str(), "token_contract");
    assert_eq!(config.terraswap_pair.as_str(), "token_stable_pair");
    assert_eq!(config.stable_denom, "uabc");
    assert_eq!(config.min_token_price, Decimal::from_ratio(10u64, 100u64));

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool, Uint128::zero());
}

#[test]
fn init_fails_without_swap_pair() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg);
    assert!(matches!(res, Err(ContractError::Std { .. })));
}

#[test]
fn provide_initial_token_pool() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Initialize token funds
    let initial_token_amount = 1_000_000_000;
    deps.querier.terranames_token_querier.balances.insert(
        Addr::unchecked(MOCK_CONTRACT_ADDR), Uint128::from(initial_token_amount),
    );
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(initial_token_amount),
        sender: "governor".into(),
        msg: to_binary(&ReceiveMsg::AcceptInitialTokens { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool.u128(), initial_token_amount);
}

#[test]
fn accept_funds_releases_tokens_into_empty_pool() {
    // Create contract with balance simulating that the auction has deposited
    // some funds already.
    let deposit = 1_491_362;
    let mut deps = mock_dependencies(&coins(deposit, ABC_COIN));

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Initialize token funds
    let initial_token_amount = 1_000_000_000;
    deps.querier.terranames_token_querier.balances.insert(
        Addr::unchecked(MOCK_CONTRACT_ADDR), Uint128::from(initial_token_amount),
    );
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(initial_token_amount),
        sender: "governor".into(),
        msg: to_binary(&ReceiveMsg::AcceptInitialTokens { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Activate the consume stable coin funds endpoint. The pool is empty so this
    // should use the initial price from the contract init.
    let tax_amount = 6016;
    let expected_tokens_released = 14_853_460;

    let env = mock_env();
    let info = mock_info("user", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::ConsumeExcessStable {}).unwrap();
    assert_eq!(res.messages.len(), 2);

    // Expect first message to increase the allowance for the terraswap pair to
    // be able to withdraw tokens when providing liquidity.
    let increase_allowance_message = &res.messages[0];
    match increase_allowance_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_contract");
            assert_eq!(send, &[]);

            match from_binary(&msg).unwrap() {
                Cw20ExecuteMsg::IncreaseAllowance { spender, amount, expires } => {
                    assert_eq!(spender.as_str(), "token_stable_pair");
                    assert_eq!(amount.u128(), expected_tokens_released);
                    assert_eq!(expires, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", increase_allowance_message),
    }

    // Expect second message to provide liquidity to the terraswap pair.
    let provide_liquidity_message = &res.messages[1];
    match provide_liquidity_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_stable_pair");
            assert_eq!(send, &coins(deposit - tax_amount, ABC_COIN));

            match from_binary(&msg).unwrap() {
                PairExecuteMsg::ProvideLiquidity { assets, slippage_tolerance } => {
                    assert_eq!(assets[0], Asset {
                        info: AssetInfo::Token {
                            contract_addr: Addr::unchecked("token_contract"),
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

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool.u128(), initial_token_amount - expected_tokens_released);
}

#[test]
fn accept_funds_releases_tokens_into_existing_pool() {
    // Create contract with balance simulating that the auction has deposited
    // some funds already.
    let deposit = 1_491_362;
    let mut deps = mock_dependencies(&coins(deposit, ABC_COIN));

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Initialize token funds
    let initial_token_amount = 906_122_399_771;
    deps.querier.terranames_token_querier.balances.insert(
        Addr::unchecked(MOCK_CONTRACT_ADDR), Uint128::from(initial_token_amount),
    );
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(initial_token_amount),
        sender: "governor".into(),
        msg: to_binary(&ReceiveMsg::AcceptInitialTokens { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Set the current number of tokens and stable coints in the terraswap
    // pool pair.
    deps.querier.terraswap_querier.pair_1_amount = 93_877_600_229;
    deps.querier.terraswap_querier.pair_2_amount = 22_141_001_913;
    deps.querier.terraswap_querier.pair_total_share = 1_234_567_890;

    // Activate the consume stable coin funds endpoint. The pool has funds so
    // this should use the implied price from the pool.
    let tax_amount = 6016;
    let expected_tokens_released = 6_297_850;

    let env = mock_env();
    let info = mock_info("user", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::ConsumeExcessStable {}).unwrap();
    assert_eq!(res.messages.len(), 2);

    // Expect first message to increase the allowance for the terraswap pair to
    // be able to withdraw tokens when providing liquidity.
    let increase_allowance_message = &res.messages[0];
    match increase_allowance_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_contract");
            assert_eq!(send, &[]);

            match from_binary(&msg).unwrap() {
                Cw20ExecuteMsg::IncreaseAllowance { spender, amount, expires } => {
                    assert_eq!(spender.as_str(), "token_stable_pair");
                    assert_eq!(amount.u128(), expected_tokens_released);
                    assert_eq!(expires, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", increase_allowance_message),
    }

    // Expect second message to provide liquidity to the terraswap pair.
    let provide_liquidity_message = &res.messages[1];
    match provide_liquidity_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_stable_pair");
            assert_eq!(send, &coins(deposit - tax_amount, ABC_COIN));

            match from_binary(&msg).unwrap() {
                PairExecuteMsg::ProvideLiquidity { assets, slippage_tolerance } => {
                    assert_eq!(assets[0], Asset {
                        info: AssetInfo::Token {
                            contract_addr: Addr::unchecked("token_contract"),
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

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool.u128(), initial_token_amount - expected_tokens_released);
}

#[test]
fn accept_funds_buys_tokens() {
    // Create contract with balance simulating that the auction has deposited
    // some funds already.
    let deposit = 1_491_362;
    let mut deps = mock_dependencies(&coins(deposit, ABC_COIN));

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Activate the consume stable coin funds endpoint
    let env = mock_env();
    let info = mock_info("user", &coins(deposit, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::ConsumeExcessStable {}).unwrap();
    assert_eq!(res.messages.len(), 1);

    let tax_amount = 6016;

    // There are no initially deposited tokens in the contract so expect
    // the first message to swap stable denom to tokens
    let swap_message = &res.messages[0];
    match swap_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_stable_pair");
            assert_eq!(send, &coins(deposit - tax_amount, ABC_COIN));

            match from_binary(&msg).unwrap() {
                PairExecuteMsg::Swap { offer_asset, to, belief_price, max_spread } => {
                    assert_eq!(offer_asset, Asset {
                        info: AssetInfo::NativeToken {
                            denom: ABC_COIN.into(),
                        },
                        amount: Uint128::from(deposit - tax_amount),
                    });
                    assert_eq!(to, None);
                    assert_eq!(belief_price, None);
                    assert_eq!(max_spread, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", swap_message),
    }

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool.u128(), 0);
}

#[test]
fn accept_funds_releases_remaining_tokens_then_buys_tokens() {
    // Create contract with balance simulating that the auction has deposited
    // some funds already.
    let deposit = 48_799_125;
    let mut deps = mock_dependencies(&coins(deposit, ABC_COIN));

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    deps.querier.terraswap_querier.pair = Some(default_pair_info());

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Initialize token funds
    let initial_token_amount: u128 = 10_000_000;
    deps.querier.terranames_token_querier.balances.insert(
        Addr::unchecked(MOCK_CONTRACT_ADDR), Uint128::from(initial_token_amount),
    );
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(initial_token_amount),
        sender: "governor".into(),
        msg: to_binary(&ReceiveMsg::AcceptInitialTokens { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Set the current number of tokens and stable coints in the terraswap
    // pool pair.
    deps.querier.terraswap_querier.pair_1_amount = 999_990_000_000;
    deps.querier.terraswap_querier.pair_2_amount = 3_205_117_988_431;
    deps.querier.terraswap_querier.pair_total_share = 1_234_567_890;

    let liquidity_net_amount = 32_051_500;
    let liquidity_tax_amount = 129_808;
    let swap_amount = deposit - liquidity_net_amount - liquidity_tax_amount;
    let swap_tax_amount = 67_031;

    // Activate the consume stable coin funds endpoint
    let env = mock_env();
    let info = mock_info("user", &coins(deposit, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::ConsumeExcessStable {}).unwrap();
    assert_eq!(res.messages.len(), 3);

    // Expect first message to increase the allowance for the terraswap pair to
    // be able to withdraw tokens when providing liquidity.
    let increase_allowance_message = &res.messages[0];
    match increase_allowance_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_contract");
            assert_eq!(send, &[]);

            match from_binary(&msg).unwrap() {
                Cw20ExecuteMsg::IncreaseAllowance { spender, amount, expires } => {
                    assert_eq!(spender.as_str(), "token_stable_pair");
                    assert_eq!(amount.u128(), initial_token_amount);
                    assert_eq!(expires, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", increase_allowance_message),
    }

    // Expect second message to provide liquidity to the terraswap pair.
    let provide_liquidity_message = &res.messages[1];
    match provide_liquidity_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_stable_pair");
            assert_eq!(send, &coins(liquidity_net_amount, ABC_COIN));

            match from_binary(&msg).unwrap() {
                PairExecuteMsg::ProvideLiquidity { assets, slippage_tolerance } => {
                    assert_eq!(assets[0], Asset {
                        info: AssetInfo::Token {
                            contract_addr: Addr::unchecked("token_contract"),
                        },
                        amount: Uint128::from(initial_token_amount),
                    });
                    assert_eq!(assets[1], Asset {
                        info: AssetInfo::NativeToken {
                            denom: ABC_COIN.into(),
                        },
                        amount: Uint128::from(liquidity_net_amount),
                    });
                    assert_eq!(slippage_tolerance, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", provide_liquidity_message),
    }

    // There are no tokens left in the contract so expect the last message to
    // swap stable denom to tokens
    let swap_message = &res.messages[2];
    match swap_message {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "token_stable_pair");
            assert_eq!(send, &coins(swap_amount - swap_tax_amount, ABC_COIN));

            match from_binary(&msg).unwrap() {
                PairExecuteMsg::Swap { offer_asset, to, belief_price, max_spread } => {
                    assert_eq!(offer_asset, Asset {
                        info: AssetInfo::NativeToken {
                            denom: ABC_COIN.into(),
                        },
                        amount: Uint128::from(swap_amount - swap_tax_amount),
                    });
                    assert_eq!(to, None);
                    assert_eq!(belief_price, None);
                    assert_eq!(max_spread, None);
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", swap_message),
    }

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.initial_token_pool.u128(), 0);
}
