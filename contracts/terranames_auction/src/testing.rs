use cosmwasm_std::{coins, from_binary, CosmosMsg, HumanAddr, Uint128, WasmMsg};
use cosmwasm_std::testing::mock_env;

use terranames::auction::{
    ConfigResponse, HandleMsg, InitMsg, NameStateResponse, QueryMsg,
};
use terranames::collector::{
    AcceptFunds, HandleMsg as CollectorHandleMsg,
};

use crate::contract::{handle, init, query};
use crate::mock_querier::mock_dependencies;

static ABC_COIN: &str = "uabc";
static NOT_ABC_COIN: &str = "uNOT";

fn default_init() -> InitMsg {
    InitMsg {
        collector_addr: HumanAddr("collector".to_string()),
        stable_denom: ABC_COIN.to_string(),
        min_lease_blocks: 2254114, // ~6 months
        max_lease_blocks: 22541140, // ~5 years
        counter_delay_blocks: 86400, // ~1 week
        transition_delay_blocks: 259200, // ~3 weeks
        bid_delay_blocks: 2254114, // ~6 months
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    // we can just call .unwrap() to assert this was a success
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(&deps, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(config.collector_addr.as_str(), "collector");
    assert_eq!(config.stable_denom.as_str(), ABC_COIN);
    assert_eq!(config.min_lease_blocks, 2254114);
    assert_eq!(config.max_lease_blocks, 22541140);
    assert_eq!(config.counter_delay_blocks, 86400);
    assert_eq!(config.transition_delay_blocks, 259200);
    assert_eq!(config.bid_delay_blocks, 2254114);
}

#[test]
fn initial_zero_bid() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block: u64 = 1234;
    let mut env = mock_env("bidder", &[]);
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::zero(),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner.as_str(), "bidder");
    assert_eq!(name_state.controller, None);

    assert_eq!(name_state.rate, Uint128::zero());
    assert_eq!(name_state.begin_block, bid_block);
    assert_eq!(name_state.begin_deposit, Uint128::zero());

    assert_eq!(name_state.counter_delay_end, 1234 + 86400);
    assert_eq!(name_state.transition_delay_end, 1234);
    assert_eq!(name_state.bid_delay_end, 1234);
    assert_eq!(name_state.expire_block, None);
}

#[test]
fn initial_non_zero_bid() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block: u64 = 1234;
    let deposit_amount: u128 = 1_230_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    // The tax needed to be withheld from 1_230_000 at the rate of 0.405%.
    // Note that this is not simply 1_230_000 * 0.405% because amount plus tax
    // has to sum to 1_230_000.
    let tax_amount = 4962;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(194_513u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Assert funds message sent to collector
    let send_to_collector_msg = &res.messages[0];
    match send_to_collector_msg {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "collector");
            assert_eq!(send, &coins(deposit_amount - tax_amount, ABC_COIN));

            let msg: CollectorHandleMsg = from_binary(&msg).unwrap();
            match msg {
                CollectorHandleMsg::AcceptFunds(AcceptFunds { denom, source_addr }) => {
                    assert_eq!(denom, ABC_COIN);
                    assert_eq!(source_addr.as_str(), "bidder");
                },
                _ => panic!("Unexpected message"),
            }
        },
        _ => panic!("Unexpected message type"),
    }

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner.as_str(), "bidder");
    assert_eq!(name_state.controller, None);

    assert_eq!(name_state.rate, Uint128::from(194_513u64));
    assert_eq!(name_state.begin_block, bid_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(1_230_000u64));

    assert_eq!(name_state.counter_delay_end, 1234 + 86400);
    assert_eq!(name_state.transition_delay_end, 1234);
    assert_eq!(name_state.bid_delay_end, 1234 + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(1234 + 6323484));
}

#[test]
fn initial_bid_outside_of_allowed_block_range() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let deposit_amount: u128 = 1_230_000;

    // High rate results in too few blocks leased
    let env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123_456_789u64),
    });
    assert_eq!(res.is_err(), true);

    // High rate results in too few blocks leased
    let env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123_456_789u64),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn bid_on_existing_name_as_owner() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block: u64 = 1234;
    let deposit_amount = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_1_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Bid on the name as the current owner. Not allowed.
    let bid_2_block: u64 = 2000;
    let deposit_amount: u128 = 2_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    });
    assert_eq!(res.is_err(), true);

    // Bid on the name as the current owner after expiry. This is allowed.
    let bid_2_block: u64 = bid_1_block + 8130081;
    let deposit_amount: u128 = 2_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);
}

#[test]
fn bid_on_existing_zero_rate_name_in_counter_delay() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block: u64 = 1234;
    let mut env = mock_env("bidder_1", &[]);
    env.block.height = bid_1_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::zero(),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Bid within counter delay. For a zero rate name this should not matter as
    // a counter bid can be posted any time.
    let bid_2_block: u64 = 2000;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_2", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner.as_str(), "bidder_2");
    assert_eq!(name_state.controller, None);

    assert_eq!(name_state.rate, Uint128::from(123u64));
    assert_eq!(name_state.begin_block, bid_2_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(1_000u64));

    assert_eq!(name_state.counter_delay_end, bid_2_block + 86400);
    assert_eq!(name_state.transition_delay_end, bid_2_block + 86400 + 259200);
    assert_eq!(name_state.bid_delay_end, bid_2_block + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(bid_2_block + 8130081));
}

#[test]
fn bid_on_existing_zero_rate_name_after_counter_delay() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block: u64 = 1234;
    let mut env = mock_env("bidder_1", &[]);
    env.block.height = bid_1_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::zero(),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Bid after counter delay. For a zero rate name this should not matter as
    // a counter bid can be posted any time.
    let bid_2_block: u64 = bid_1_block + 86400;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_2", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner.as_str(), "bidder_2");
    assert_eq!(name_state.controller, None);

    assert_eq!(name_state.rate, Uint128::from(123u64));
    assert_eq!(name_state.begin_block, bid_2_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(1_000u64));

    assert_eq!(name_state.counter_delay_end, bid_2_block + 86400);
    assert_eq!(name_state.transition_delay_end, bid_2_block + 86400 + 259200);
    assert_eq!(name_state.bid_delay_end, bid_2_block + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(bid_2_block + 8130081));
}

// TODO More bidding test cases needed here

#[test]
fn fund_unclaimed_name_fails() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let env = mock_env("funder", &coins(1_000_000, ABC_COIN));
    let res = handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("owner"),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn fund_zero_rate_name_fails() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let mut env = mock_env("bidder", &[]);
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::zero(),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Funding a zero rate name is not possible.
    let fund_block = 5000;
    let mut env = mock_env("funder", &coins(1_000_000, ABC_COIN));
    env.block.height = fund_block;
    let res = handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("bidder"),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn fund_name() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Funding an owned name is possible up to the max block limit.
    let fund_block = 4_000_000;
    let deposit_amount: u128 = 2_000;
    let tax_amount = 9;

    let mut env = mock_env("funder", &coins(deposit_amount, ABC_COIN));
    env.block.height = fund_block;
    let res = handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("bidder"),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Assert funds message sent to collector
    let send_to_collector_msg = &res.messages[0];
    match send_to_collector_msg {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "collector");
            assert_eq!(send, &coins(deposit_amount - tax_amount, ABC_COIN));

            let msg: CollectorHandleMsg = from_binary(&msg).unwrap();
            match msg {
                CollectorHandleMsg::AcceptFunds(AcceptFunds { denom, source_addr }) => {
                    assert_eq!(denom, ABC_COIN);
                    assert_eq!(source_addr.as_str(), "funder");
                },
                _ => panic!("Unexpected message"),
            }
        },
        _ => panic!("Unexpected message type"),
    }

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner, HumanAddr::from("bidder"));

    assert_eq!(name_state.rate, Uint128::from(123u64));
    assert_eq!(name_state.begin_block, bid_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(3_000u64));

    assert_eq!(name_state.counter_delay_end, bid_block + 86400);
    assert_eq!(name_state.transition_delay_end, 1234);
    assert_eq!(name_state.bid_delay_end, 1234 + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(1234 + 24390243));
}

#[test]
fn fund_name_fails_due_to_other_bid() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Bidder 2 submits a bid while funder is preparing to fund bidder 1.
    let bid_block = 1235;
    let deposit_amount: u128 = 2_000;
    let mut env = mock_env("bidder_2", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 2);

    // Funder submits funding simultaneously but bidder_2 transaction happens
    // first.
    let fund_block = 1236;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("funder", &coins(deposit_amount, ABC_COIN));
    env.block.height = fund_block;
    let res = handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("bidder_1"),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn fund_name_fails_with_zero_funds() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Funder submits funding request without adding funds or with the wrong
    // coin.
    let fund_block = 1236;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("funder", &coins(deposit_amount, NOT_ABC_COIN));
    env.block.height = fund_block;
    let res = handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("bidder_1"),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn fund_name_fails_with_too_much_funding() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Funder submits funding request adding too much funds pushing the lease
    // over the max limit.
    let fund_block = 1236;
    let deposit_amount: u128 = 1_773;
    let mut env = mock_env("funder", &coins(deposit_amount, ABC_COIN));
    env.block.height = fund_block;
    let res = handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("bidder_1"),
    });
    assert_eq!(res.is_err(), true);

    // Reduce the funding under the limit should result in success.
    let deposit_amount: u128 = 1_772;
    let mut env = mock_env("funder", &coins(deposit_amount, ABC_COIN));
    env.block.height = fund_block;
    handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("bidder_1"),
    }).unwrap();
}

#[test]
fn set_lower_rate() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to decrease the charged rate
    let rate_change_block = 100_000;
    let mut env = mock_env("bidder", &[]);
    env.block.height = rate_change_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(98u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner, HumanAddr::from("bidder"));

    assert_eq!(name_state.rate, Uint128::from(98u64));
    assert_eq!(name_state.begin_block, rate_change_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(987u64));

    assert_eq!(name_state.counter_delay_end, rate_change_block + 86400);
    assert_eq!(name_state.transition_delay_end, rate_change_block);
    assert_eq!(name_state.bid_delay_end, rate_change_block + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(rate_change_block + 10071428));
}

#[test]
fn set_higher_rate() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to increase the charged rate
    let rate_change_block = 100_000;
    let mut env = mock_env("bidder", &[]);
    env.block.height = rate_change_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner, HumanAddr::from("bidder"));

    assert_eq!(name_state.rate, Uint128::from(246u64));
    assert_eq!(name_state.begin_block, rate_change_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(987u64));

    assert_eq!(name_state.counter_delay_end, rate_change_block + 86400);
    assert_eq!(name_state.transition_delay_end, rate_change_block);
    assert_eq!(name_state.bid_delay_end, rate_change_block + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(rate_change_block + 4012195));
}

#[test]
fn set_rate_during_counter_delay_fails() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_1_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // First bidder submits requests to change the charged rate
    let rate_change_block = 2400;
    let mut env = mock_env("bidder_1", &[]);
    env.block.height = rate_change_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    });
    assert_eq!(res.is_err(), true);

    let bid_2_block = 2500;
    let deposit_amount: u128 = 1_001;
    let mut env = mock_env("bidder_2", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(124u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 2);

    // Second bidder submits requests to change the charged rate
    let rate_change_block = 80000;
    let mut env = mock_env("bidder_2", &[]);
    env.block.height = rate_change_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(98u64),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn set_rate_as_non_owner_fails() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_1_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    let bid_2_block = 2400;
    let deposit_amount: u128 = 1_001;
    let mut env = mock_env("bidder_2", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(124u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 2);

    // First bidder submits requests to change the charged rate
    let rate_change_block = 200_000;
    let mut env = mock_env("bidder_1", &[]);
    env.block.height = rate_change_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn set_rate_outside_of_bounds_fails() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to decrease the charged rate
    let rate_change_block = 100_000;
    let mut env = mock_env("bidder", &[]);
    env.block.height = rate_change_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(40u64),
    });
    assert_eq!(res.is_err(), true);

    // Owner submits requests to increase the charged rate
    let rate_change_block = 100_000;
    let mut env = mock_env("bidder", &[]);
    env.block.height = rate_change_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(500u64),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn transfer_owner() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Ownership transferred
    let transfer_block = 100_000;
    let mut env = mock_env("bidder", &[]);
    env.block.height = transfer_block;
    let res = handle(&mut deps, env, HandleMsg::TransferNameOwner {
        name: "example".to_string(),
        to: HumanAddr::from("receiver"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner, HumanAddr::from("receiver"));

    assert_eq!(name_state.begin_block, bid_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(deposit_amount));

    assert_eq!(name_state.counter_delay_end, bid_block + 86400);
    assert_eq!(name_state.transition_delay_end, bid_block);
    assert_eq!(name_state.bid_delay_end, bid_block + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(bid_block + 8130081));
}

#[test]
fn transfer_owner_during_counter_bid() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_1_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Another bid occurs
    let bid_2_block = 2342748;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_2", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(124u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 2);

    // Original owner can transfer their expiring ownership
    let transfer_block = 2342749;
    let mut env = mock_env("bidder_1", &[]);
    env.block.height = transfer_block;
    let res = handle(&mut deps, env, HandleMsg::TransferNameOwner {
        name: "example".to_string(),
        to: HumanAddr::from("receiver_1"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner, HumanAddr::from("bidder_2"));

    assert_eq!(name_state.begin_block, bid_2_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(deposit_amount));

    assert_eq!(name_state.counter_delay_end, bid_2_block + 86400);
    assert_eq!(name_state.transition_delay_end, bid_2_block + 86400 + 259200);
    assert_eq!(name_state.bid_delay_end, bid_2_block + 86400 + 2254114);
    assert_eq!(name_state.expire_block, Some(bid_2_block + 8064516));

    assert_eq!(name_state.previous_owner, Some(HumanAddr::from("receiver_1")));

    // Highest bid owner can also transfer their ownership of the bid and
    // potential future ownership of the name.
    let mut env = mock_env("bidder_2", &[]);
    env.block.height = transfer_block;
    let res = handle(&mut deps, env, HandleMsg::TransferNameOwner {
        name: "example".to_string(),
        to: HumanAddr::from("receiver_2"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner, HumanAddr::from("receiver_2"));

    assert_eq!(name_state.begin_block, bid_2_block);
    assert_eq!(name_state.begin_deposit, Uint128::from(deposit_amount));

    assert_eq!(name_state.previous_owner, Some(HumanAddr::from("receiver_1")));
}

#[test]
fn transfer_owner_fails_if_not_owner() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Ownership transfer fails
    let transfer_block = 100_000;
    let mut env = mock_env("other", &[]);
    env.block.height = transfer_block;
    let res = handle(&mut deps, env, HandleMsg::TransferNameOwner {
        name: "example".to_string(),
        to: HumanAddr::from("receiver"),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn set_controller_new_bid() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner cannot set controller before end of counter delay
    let set_controller_block = bid_block + 86400 - 1;
    let mut env = mock_env("bidder", &[]);
    env.block.height = set_controller_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller"),
    });
    assert_eq!(res.is_err(), true);

    // Owner can set controller after end of counter delay
    let set_controller_block = bid_block + 86400;
    let mut env = mock_env("bidder", &[]);
    env.block.height = set_controller_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner.as_str(), "bidder");
    assert_eq!(name_state.controller, Some(HumanAddr::from("controller")));
}

#[test]
fn set_controller_during_counter_delay() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_1", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(123u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 1);

    // Another bid occurs
    let bid_2_block = 2342748;
    let deposit_amount: u128 = 1_000;
    let mut env = mock_env("bidder_2", &coins(deposit_amount, ABC_COIN));
    env.block.height = bid_2_block;

    let res = handle(&mut deps, env, HandleMsg::BidName {
        name: "example".to_string(),
        rate: Uint128::from(124u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 2);

    // Original owner can set controller
    let transfer_block = 2342749;
    let mut env = mock_env("bidder_1", &[]);
    env.block.height = transfer_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller_1"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner.as_str(), "bidder_2");
    assert_eq!(name_state.controller, Some(HumanAddr::from("controller_1")));

    // Highest bid owner cannot set controller before end of counter delay
    let transfer_block = 2342750;
    let mut env = mock_env("bidder_2", &[]);
    env.block.height = transfer_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller_2"),
    });
    assert_eq!(res.is_err(), true);

    // After the counter delay ends, the highest bidder can set the controller
    let transfer_block = 2432750;
    let mut env = mock_env("bidder_2", &[]);
    env.block.height = transfer_block;
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller_2"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let res = query(&deps, QueryMsg::GetNameState {
        name: "example".to_string(),
    }).unwrap();
    let name_state: NameStateResponse = from_binary(&res).unwrap();
    assert_eq!(name_state.owner.as_str(), "bidder_2");
    assert_eq!(name_state.controller, Some(HumanAddr::from("controller_2")));
}

// Edge cases:
// - Bid by A, starts ownership. Then bid by B, then counter bid by A. A should
//   continue as owner and this should not trigger a transition.
// - Rate change by A. This should trigger a counter delay of allowed bidding
//   but not a transition unless a counter bid wins.
// - Name expires during transition. A new bid should not cancel the transition
//   period.
// - Funding an expired name should fail maybe? There may be some cases where
//   this is useful. Alternative is to bid on the expired name (this is more
//   efficient since you are only paying starting from the current block) but
//   if it has a different owner you need to then transfer it.
// - Not allowed: Bid on name, change rate (not allowed until after counter
//   delay).
// - Maybe should be allowed to lower the rate even if that causes less than
//   min lease blocks to be bought from current point in time as long as
//   min lease blocks were bought counted from the original begin deposit. Does
//   fund have the same issue?
// -
//
// Question:
// - Is max lease blocks a good idea? It is supposed to alleviate any weirdness
//   from people extending their lease 1000 years into the future. Since they
//   have to prepay the full lease, allowing it may not be any issue. It is
//   probably good to have the min lease to avoid a kind of denial of service
//   where someone can distrupt a name for a short period with a high deposit
//   and rate. Ideally the user with the longer-term interest in the name wins
//   out in most cases against short term disruption. Having a longer max lease
//   may help with this since the person with long-term interest can increase
//   the rate temporarily, then later spread the spend out over much longer
//   time when the short-term interest subsides. Though at some point the
//   interest in keeping names stable may play against the interest of using
//   names efficiently warranting keeping the max lease shorter than infinite
//   to allow better use of names without someone having to amass huge funds
//   to unseat an underutilized name.
// - It is unclear if set rate results in the right mechanics. The purpose is
//   to allow for the owner to decrease the spend rate when they no longer
//   perceive the name to be as valuable as before. If this results in no new
//   bids, they will at least hold on to the name longer and have a larger
//   refund if there is a bid later.
// - How to best handle new names? 1) A bids on new name, immediately becomes
//   owner, but if B bids during the counter delay B has to go through a
//   transition; 2) A bids on new name, does not become true owner until after
//   counter delay, but if B bids during the counter delay B also does not go
//   through transition. (probably option 2 is best)
