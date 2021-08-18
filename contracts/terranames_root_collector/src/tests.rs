use std::str::FromStr;

use cosmwasm_std::{
    coins, from_binary, to_binary, BankMsg, CosmosMsg, Decimal, SubMsg,
    Uint128, WasmMsg,
};
use cosmwasm_std::testing::{mock_env, mock_info};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use terranames::root_collector::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg,
    StakeStateResponse, StateResponse,
};

use crate::contract::{execute, instantiate, query};
use crate::errors::ContractError;
use crate::mock_querier::mock_dependencies;

static ABC_COIN: &str = "uabc";

fn default_init() -> InstantiateMsg {
    InstantiateMsg {
        base_token: "token_contract".into(),
        stable_denom: ABC_COIN.into(),
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // it worked, let's query the config
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(config.base_token.as_str(), "token_contract");
    assert_eq!(config.stable_denom, "uabc");

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.multiplier, Decimal::zero());
    assert_eq!(state.residual, Uint128::zero());
    assert_eq!(state.total_staked, Uint128::zero());
}

#[test]
fn stake_tokens() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let stake_amount: u128 = 9_122_993;

    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_amount),
        sender: "staker".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.token_amount.u128(), stake_amount);
    assert_eq!(stake_state.multiplier, Decimal::zero());
    assert_eq!(stake_state.dividend.u128(), 0);

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State { }).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.multiplier, Decimal::zero());
    assert_eq!(state.residual.u128(), 0);
    assert_eq!(state.total_staked.u128(), stake_amount);
}

#[test]
fn deposit_funds_with_nothing_staked() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // When nothing is staked, the residual should be bumped
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.total_staked.u128(), 0);
    assert_eq!(state.multiplier, Decimal::zero());
    assert_eq!(state.residual.u128(), deposit_amount);
}

#[test]
fn deposit_funds_with_residual() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // When nothing is staked, the residual should be bumped
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.residual.u128(), deposit_amount_1);

    // Stake tokens
    let stake_amount: u128 = 9_122_993;

    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_amount),
        sender: "staker".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // This deposit should trigger the release of both residual and new
    // deposits.
    let deposit_amount_2 = 54_018;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_2, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Since there is only a single staker, the full deposit is paid as dividend
    // to this account. Note: Calculation is off by one due to rounding.
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.multiplier, Decimal::zero());
    assert_eq!(stake_state.dividend.u128(), deposit_amount_1 + deposit_amount_2 - 1);
}

#[test]
fn multiple_stakers() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;

    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 stakes tokens
    let stake_2_amount: u128 = 3_451_902_999_741;

    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_2_amount),
        sender: "staker_2".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Check total staked amount and multiplier
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.total_staked.u128(), stake_1_amount + stake_2_amount);
    assert_eq!(state.multiplier, Decimal::from_str("0.014689258229179831").unwrap());
    assert_eq!(state.residual.u128(), 1);

    // Check that staker 1 stake has a dividend
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_1".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.multiplier, Decimal::zero());
    assert_eq!(stake_state.dividend.u128(), deposit_amount_1 - 1);

    // Check staker 2 stake has no dividend
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_2".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.multiplier, Decimal::from_str("0.014689258229179831").unwrap());
    assert_eq!(stake_state.dividend.u128(), 0);

    let deposit_amount_2 = 23_810_833;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_2, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Check total staked amount and multiplier
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.total_staked.u128(), stake_1_amount + stake_2_amount);
    assert_eq!(state.multiplier, Decimal::from_str("0.014696156097130519").unwrap());
    assert_eq!(state.residual.u128(), 1);

    // Check that staker 1 stake has a slightly larger dividend from deposit 1
    // and a small portion of deposit 2.
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_1".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.multiplier, Decimal::zero());
    assert_eq!(stake_state.dividend.u128(), 134_072);

    // Check staker 2 stake has dividend from the majority of deposit 2.
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_2".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.multiplier, Decimal::from_str("0.014689258229179831").unwrap());
    assert_eq!(stake_state.dividend.u128(), 23_810_771);
}

#[test]
fn withdraw_tokens() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 stakes tokens
    let stake_2_amount: u128 = 3_451_902_999_741;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_2_amount),
        sender: "staker_2".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_2 = 23_810_833;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_2, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 partial withdrawal
    let stake_withdrawal_1: u128 = 3_009_852;
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_withdrawal_1),
        to: None,
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Check send token message
    let send_token_msg = &res.messages[0];
    match send_token_msg {
        SubMsg { msg: CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, funds }), .. } => {
            assert_eq!(contract_addr.as_str(), "token_contract");
            assert_eq!(funds, &[]);
            let cw20_msg: Cw20ExecuteMsg = from_binary(&msg).unwrap();
            match cw20_msg {
                Cw20ExecuteMsg::Transfer { recipient, amount } => {
                    assert_eq!(recipient.as_str(), "staker_1");
                    assert_eq!(amount.u128(), stake_withdrawal_1);
                },
                _ => panic!("Unexpected contract message: {:?}", cw20_msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", send_token_msg),
    }

    // Check total staked amount
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.total_staked.u128(), stake_1_amount + stake_2_amount - stake_withdrawal_1);

    // Check that staker 1 stake has a calculated dividend and the correct
    // number of staked tokens.
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_1".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.token_amount.u128(), stake_1_amount - stake_withdrawal_1);
    assert_eq!(stake_state.multiplier, Decimal::from_str("0.014696156097130519").unwrap());
    assert_eq!(stake_state.dividend.u128(), 134_072);

    // Staker 1 full withdrawal
    let stake_withdrawal_2: u128 = stake_1_amount - stake_withdrawal_1;
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_withdrawal_2),
        to: None,
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Check send token message
    let send_token_msg = &res.messages[0];
    match send_token_msg {
        SubMsg { msg: CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, funds }), .. } => {
            assert_eq!(contract_addr.as_str(), "token_contract");
            assert_eq!(funds, &[]);
            let cw20_msg: Cw20ExecuteMsg = from_binary(&msg).unwrap();
            match cw20_msg {
                Cw20ExecuteMsg::Transfer { recipient, amount } => {
                    assert_eq!(recipient.as_str(), "staker_1");
                    assert_eq!(amount.u128(), stake_withdrawal_2);
                },
                _ => panic!("Unexpected contract message: {:?}", cw20_msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", send_token_msg),
    }

    // Check total staked amount
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.total_staked.u128(), stake_2_amount);

    // Check that staker 1 stake has a calculated dividend and the correct
    // number of staked tokens.
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_1".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.token_amount.u128(), 0);
    assert_eq!(stake_state.multiplier, Decimal::from_str("0.014696156097130519").unwrap());
    assert_eq!(stake_state.dividend.u128(), 134_072);
}

#[test]
fn withdraw_tokens_to_address() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker partial withdrawal to address
    let stake_withdrawal_1: u128 = 3_009_852;
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_withdrawal_1),
        to: Some("recipient".into()),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Check send token message
    let send_token_msg = &res.messages[0];
    match send_token_msg {
        SubMsg { msg: CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, funds }), .. } => {
            assert_eq!(contract_addr.as_str(), "token_contract");
            assert_eq!(funds, &[]);
            let cw20_msg: Cw20ExecuteMsg = from_binary(&msg).unwrap();
            match cw20_msg {
                Cw20ExecuteMsg::Transfer { recipient, amount } => {
                    assert_eq!(recipient.as_str(), "recipient");
                    assert_eq!(amount.u128(), stake_withdrawal_1);
                },
                _ => panic!("Unexpected contract message: {:?}", cw20_msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", send_token_msg),
    }
}

#[test]
fn withdraw_tokens_then_restake() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 stakes tokens
    let stake_2_amount: u128 = 3_451_902_999_741;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_2_amount),
        sender: "staker_2".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 withdraws tokens
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_1_amount),
        to: None
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Staker 1 withdraws dividend
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawDividends {
        to: None
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    let deposit_amount_2 = 23_810_833;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_2, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 restakes
    let stake_3_amount: u128 = 12_558_800;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_3_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Check that staker 1 stake has a calculated dividend and the correct
    // number of staked tokens.
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_1".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.token_amount.u128(), stake_3_amount);
    assert_eq!(stake_state.dividend.u128(), 0);
}

#[test]
fn withdraw_tokens_fails_for_new_address() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 withdraws tokens
    let stake_withdrawal_1: u128 = 3_009_852;
    let env = mock_env();
    let info = mock_info("staker_2", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_withdrawal_1),
        to: None
    });
    assert!(matches!(res, Err(ContractError::InsufficientTokens { .. })));
}

#[test]
fn withdraw_tokens_fails_if_too_few() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 withdraws tokens
    let stake_withdrawal_1: u128 = 10_009_852;
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_withdrawal_1),
        to: None
    });
    assert!(matches!(res, Err(ContractError::InsufficientTokens { .. })));
}

#[test]
fn withdraw_tokens_fails_if_already_withdrawn() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 stakes tokens
    let stake_2_amount: u128 = 3_451_902_999_741;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_2_amount),
        sender: "staker_2".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 withdraws tokens
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_1_amount),
        to: None
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Staker 1 tries to withdraw more tokens
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawTokens {
        amount: Uint128::from(stake_1_amount),
        to: None
    });
    assert!(matches!(res, Err(ContractError::InsufficientTokens { .. })));
}

#[test]
fn withdraw_dividends() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 stakes tokens
    let stake_2_amount: u128 = 3_451_902_999_741;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_2_amount),
        sender: "staker_2".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_2 = 23_810_833;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_2, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 withdraws dividends
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawDividends {
        to: None,
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    let expected_dividend = 134_072;
    let expected_dividend_tax = 541;

    // Check send funds message
    let send_funds_msg = &res.messages[0];
    match send_funds_msg {
        SubMsg { msg: CosmosMsg::Bank(BankMsg::Send { to_address, amount }), .. } => {
            assert_eq!(to_address.as_str(), "staker_1");
            assert_eq!(amount, &coins(expected_dividend - expected_dividend_tax, ABC_COIN));
        },
        _ => panic!("Unexpected message type: {:?}", send_funds_msg),
    }

    // Check total staked amount
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::State {}).unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(state.total_staked.u128(), stake_1_amount + stake_2_amount);

    // Check that staker 1 stake has a calculated dividend and the correct
    // number of staked tokens.
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::StakeState {
        address: "staker_1".into(),
    }).unwrap();
    let stake_state: StakeStateResponse = from_binary(&res).unwrap();
    assert_eq!(stake_state.token_amount.u128(), stake_1_amount);
    assert_eq!(stake_state.multiplier, Decimal::from_str("0.014696156097130519").unwrap());
    assert_eq!(stake_state.dividend.u128(), 0);
}

#[test]
fn withdraw_dividends_to_address() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 withdraws dividends
    let env = mock_env();
    let info = mock_info("staker_1", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawDividends {
        to: Some("recipient".into()),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    let expected_dividend = 134_010;
    let expected_dividend_tax = 542;

    // Check send funds message
    let send_funds_msg = &res.messages[0];
    match send_funds_msg {
        SubMsg { msg: CosmosMsg::Bank(BankMsg::Send { to_address, amount }), .. } => {
            assert_eq!(to_address.as_str(), "recipient");
            assert_eq!(amount, &coins(expected_dividend - expected_dividend_tax, ABC_COIN));
        },
        _ => panic!("Unexpected message type: {:?}", send_funds_msg),
    }
}

#[test]
fn withdraw_dividends_fails_for_new_address() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 withdraws
    let env = mock_env();
    let info = mock_info("staker_2", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawDividends {
        to: None,
    });
    assert!(matches!(res, Err(ContractError::InsufficientFunds { .. })));
}

#[test]
fn withdraw_dividends_fails_for_withdraw_zero() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 1 stakes tokens
    let stake_1_amount: u128 = 9_122_993;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_1_amount),
        sender: "staker_1".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    let deposit_amount_1 = 134_010;
    let env = mock_env();
    let info = mock_info("auction", &coins(deposit_amount_1, ABC_COIN));
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Deposit {}).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 stakes tokens
    let stake_2_amount: u128 = 3_451_902_999_741;
    let env = mock_env();
    let info = mock_info("token_contract", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(Cw20ReceiveMsg {
        amount: Uint128::from(stake_2_amount),
        sender: "staker_2".into(),
        msg: to_binary(&ReceiveMsg::Stake { }).unwrap(),
    })).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Staker 2 withdraws dividends
    let env = mock_env();
    let info = mock_info("staker_2", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawDividends {
        to: None,
    });
    assert!(matches!(res, Err(ContractError::InsufficientFunds { .. })));
}
