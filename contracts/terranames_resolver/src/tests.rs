use cosmwasm_std::{from_binary, Addr, Uint128};
use cosmwasm_std::testing::{mock_env, mock_info};

use terranames::auction::NameStateResponse;
use terranames::resolver::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ResolveNameResponse,
};

use crate::contract::{execute, instantiate, query};
use crate::errors::ContractError;
use crate::mock_querier::mock_dependencies;

fn default_init() -> InstantiateMsg {
    InstantiateMsg {
        auction_contract: "auction".into(),
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

    // it worked, let's query the state
    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(config.auction_contract.as_str(), "auction");
}

#[test]
fn set_value_to_string() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    deps.querier.auction_querier.response = Some(NameStateResponse {
        owner: Addr::unchecked("owner"),
        controller: Some(Addr::unchecked("controller")),
        rate: Uint128::from(100u64),
        begin_block: 100_000,
        begin_deposit: Uint128::from(1000u64),

        previous_owner: None,

        counter_delay_end: 110000,
        transition_delay_end: 130000,
        bid_delay_end: 2000000,
        expire_block: Some(10_100_000),
    });

    let mut env = mock_env();
    env.block.height = 123456;
    let info = mock_info("controller", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::SetNameValue {
        name: "example".to_string(),
        value: Some("test_value".to_string()),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::ResolveName {
        name: "example".to_string(),
    }).unwrap();
    let resolved: ResolveNameResponse = from_binary(&res).unwrap();
    assert_eq!(resolved.value, Some("test_value".into()));
    assert_eq!(resolved.expire_block, Some(10_100_000));
}

#[test]
fn set_value_to_none() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    deps.querier.auction_querier.response = Some(NameStateResponse {
        owner: Addr::unchecked("owner"),
        controller: Some(Addr::unchecked("controller")),
        rate: Uint128::from(100u64),
        begin_block: 100_000,
        begin_deposit: Uint128::from(1000u64),

        previous_owner: None,

        counter_delay_end: 110000,
        transition_delay_end: 130000,
        bid_delay_end: 2000000,
        expire_block: Some(10_100_000),
    });

    let mut env = mock_env();
    env.block.height = 123456;
    let info = mock_info("controller", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::SetNameValue {
        name: "example".to_string(),
        value: None,
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::ResolveName {
        name: "example".to_string(),
    }).unwrap();
    let resolved: ResolveNameResponse = from_binary(&res).unwrap();
    assert_eq!(resolved.value, None);
    assert_eq!(resolved.expire_block, Some(10_100_000));
}

#[test]
fn set_value_for_zero_bid() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    deps.querier.auction_querier.response = Some(NameStateResponse {
        owner: Addr::unchecked("owner"),
        controller: Some(Addr::unchecked("controller")),
        rate: Uint128::zero(),
        begin_block: 100_000,
        begin_deposit: Uint128::zero(),

        previous_owner: None,

        counter_delay_end: 110000,
        transition_delay_end: 130000,
        bid_delay_end: 100_000,
        expire_block: None,
    });

    let mut env = mock_env();
    env.block.height = 123456;
    let info = mock_info("controller", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::SetNameValue {
        name: "example".to_string(),
        value: Some("test_value".into()),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let env = mock_env();
    let res = query(deps.as_ref(), env, QueryMsg::ResolveName {
        name: "example".to_string(),
    }).unwrap();
    let resolved: ResolveNameResponse = from_binary(&res).unwrap();
    assert_eq!(resolved.value, Some("test_value".into()));
    assert_eq!(resolved.expire_block, None);
}

#[test]
fn set_value_as_other_fails() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    deps.querier.auction_querier.response = Some(NameStateResponse {
        owner: Addr::unchecked("owner"),
        controller: Some(Addr::unchecked("controller")),
        rate: Uint128::zero(),
        begin_block: 100_000,
        begin_deposit: Uint128::zero(),

        previous_owner: None,

        counter_delay_end: 110000,
        transition_delay_end: 130000,
        bid_delay_end: 100_000,
        expire_block: None,
    });

    // Fails when called as other sender
    let mut env = mock_env();
    env.block.height = 123456;
    let info = mock_info("other", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::SetNameValue {
        name: "example".to_string(),
        value: Some("test_value".into()),
    });
    assert!(matches!(res, Err(ContractError::Unauthorized { .. })));

    // Even if called as owner
    let mut env = mock_env();
    env.block.height = 123456;
    let info = mock_info("owner", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::SetNameValue {
        name: "example".to_string(),
        value: Some("test_value".into()),
    });
    assert!(matches!(res, Err(ContractError::Unauthorized { .. })));
}

#[test]
fn set_value_when_controller_is_none_fails() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    deps.querier.auction_querier.response = Some(NameStateResponse {
        owner: Addr::unchecked("owner"),
        controller: None,
        rate: Uint128::zero(),
        begin_block: 100_000,
        begin_deposit: Uint128::zero(),

        previous_owner: None,

        counter_delay_end: 110_000,
        transition_delay_end: 130_000,
        bid_delay_end: 100_000,
        expire_block: None,
    });

    // Fails when called as any sender
    let mut env = mock_env();
    env.block.height = 123456;
    let info = mock_info("owner", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::SetNameValue {
        name: "example".to_string(),
        value: Some("test_value".into()),
    });
    assert!(matches!(res, Err(ContractError::Unauthorized { .. })));
}

#[test]
fn set_value_when_expired_fails() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    deps.querier.auction_querier.response = Some(NameStateResponse {
        owner: Addr::unchecked("owner"),
        controller: Some(Addr::unchecked("controller")),
        rate: Uint128::from(100u64),
        begin_block: 100_000,
        begin_deposit: Uint128::from(1000u64),

        previous_owner: None,

        counter_delay_end: 110_000,
        transition_delay_end: 130_000,
        bid_delay_end: 2000000,
        expire_block: Some(10_100_000),
    });

    // Fails when called after expiration
    let mut env = mock_env();
    env.block.height = 10_100_000;
    let info = mock_info("controller", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::SetNameValue {
        name: "example".to_string(),
        value: Some("test_value".into()),
    });
    assert!(matches!(res, Err(ContractError::NameExpired { .. })));
}
