use cosmwasm_std::{
    coins, from_binary, Api, CosmosMsg, Env, Extern, HandleResult, HumanAddr,
    Querier, Storage, Uint128, WasmMsg,
};
use cosmwasm_std::testing::mock_env;

use terranames::auction::{
    AllNameStatesResponse, ConfigResponse, HandleMsg, InitMsg,
    NameStateResponse, QueryMsg,
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

/// Builder for creating a single bid
struct Bid<'a> {
    name: &'a str,
    bidder: &'a str,
    block_height: u64,
    rate: u128,
    deposit: u128,
}

impl<'a> Bid<'a> {
    fn on(name: &'a str, bidder: &'a str, block_height: u64) -> Bid<'a> {
        Bid {
            name,
            bidder,
            block_height,
            rate: 0,
            deposit: 0,
        }
    }

    fn rate(self, rate: u128) -> Bid<'a> {
        Self {
            rate,
            ..self
        }
    }

    fn deposit(self, deposit: u128) -> Bid<'a> {
        Self {
            deposit,
            ..self
        }
    }

    fn handle<S: Storage, A: Api, Q: Querier>(
        self,
        deps: &mut Extern<S, A, Q>,
    ) -> HandleResult {
        let mut env = mock_env(self.bidder, &coins(self.deposit, ABC_COIN));
        env.block.height = self.block_height;

        handle(deps, env, HandleMsg::BidName {
            name: self.name.to_string(),
            rate: Uint128::from(self.rate),
        })
    }
}

/// Helper trait for modifying Env
trait EnvBuilder {
    fn at_block_height(self, block_height: u64) -> Self;
}

impl EnvBuilder for Env {
    /// Set block height for Env
    fn at_block_height(mut self, block_height: u64) -> Self {
        self.block.height = block_height;
        self
    }
}

/// Helper for asserting NameStateResponse
#[must_use]
struct NameStateAsserter<'a> {
    name: &'a str,

    owner: Option<&'a str>,
    controller: Option<Option<&'a str>>,

    rate: Option<u128>,
    begin_block: Option<u64>,
    begin_deposit: Option<u128>,

    previous_owner: Option<Option<&'a str>>,

    counter_delay_end: Option<u64>,
    transition_delay_end: Option<u64>,
    bid_delay_end: Option<u64>,
    expire_block: Option<Option<u64>>,
}

impl<'a> NameStateAsserter<'a> {
    /// Create NameStateAsserter for asserting state of name
    fn new(name: &'a str) -> Self {
        Self {
            name,
            owner: None,
            controller: None,
            rate: None,
            begin_block: None,
            begin_deposit: None,
            previous_owner: None,
            counter_delay_end: None,
            transition_delay_end: None,
            bid_delay_end: None,
            expire_block: None,
        }
    }

    /// Set owner to assert
    fn owner(self, owner: &'a str) -> Self {
        Self {
            owner: Some(owner),
            ..self
        }
    }

    /// Set controller to assert
    fn controller(self, controller: Option<&'a str>) -> Self {
        Self {
            controller: Some(controller),
            ..self
        }
    }

    /// Set rate to assert
    fn rate(self, rate: u128) -> Self {
        Self {
            rate: Some(rate),
            ..self
        }
    }

    /// Set begin block to assert
    fn begin_block(self, begin_block: u64) -> Self {
        Self {
            begin_block: Some(begin_block),
            ..self
        }
    }

    /// Set begin deposit to assert
    fn begin_deposit(self, begin_deposit: u128) -> Self {
        Self {
            begin_deposit: Some(begin_deposit),
            ..self
        }
    }

    /// Set previous owner to assert
    fn previous_owner(self, previous_owner: Option<&'a str>) -> Self {
        Self {
            previous_owner: Some(previous_owner),
            ..self
        }
    }

    /// Set counter delay end to assert
    fn counter_delay_end(self, counter_delay_end: u64) -> Self {
        Self {
            counter_delay_end: Some(counter_delay_end),
            ..self
        }
    }

    /// Set transition delay end to assert
    fn transition_delay_end(self, transition_delay_end: u64) -> Self {
        Self {
            transition_delay_end: Some(transition_delay_end),
            ..self
        }
    }

    /// Set bid delay end to assert
    fn bid_delay_end(self, bid_delay_end: u64) -> Self {
        Self {
            bid_delay_end: Some(bid_delay_end),
            ..self
        }
    }

    /// Set expire block to assert
    fn expire_block(self, expire_block: Option<u64>) -> Self {
        Self {
            expire_block: Some(expire_block),
            ..self
        }
    }

    /// Assert name state properties
    fn assert<S: Storage, A: Api, Q: Querier>(self, deps: &Extern<S, A, Q>) {
        let res = query(deps, QueryMsg::GetNameState {
            name: self.name.to_string(),
        }).unwrap();
        let name_state: NameStateResponse = from_binary(&res).unwrap();

        if let Some(owner) = self.owner {
            assert_eq!(name_state.owner.as_str(), owner, "owner does not match");
        }
        if let Some(controller) = self.controller {
            assert_eq!(name_state.controller, controller.map(|c| c.into()), "controller does not match");
        }
        if let Some(rate) = self.rate {
            assert_eq!(name_state.rate.u128(), rate, "rate does not match");
        }
        if let Some(begin_block) = self.begin_block {
            assert_eq!(name_state.begin_block, begin_block, "begin_block does not match");
        }
        if let Some(begin_deposit) = self.begin_deposit {
            assert_eq!(name_state.begin_deposit.u128(), begin_deposit, "begin_deposit does not match");
        }
        if let Some(previous_owner) = self.previous_owner {
            assert_eq!(name_state.previous_owner, previous_owner.map(|c| c.into()), "previous_owner does not match");
        }
        if let Some(counter_delay_end) = self.counter_delay_end {
            assert_eq!(name_state.counter_delay_end, counter_delay_end, "counter_delay_end does not match");
        }
        if let Some(transition_delay_end) = self.transition_delay_end {
            assert_eq!(name_state.transition_delay_end, transition_delay_end, "transition_delay_end does not match");
        }
        if let Some(bid_delay_end) = self.bid_delay_end {
            assert_eq!(name_state.bid_delay_end, bid_delay_end, "bid_delay_end does not match");
        }
        if let Some(expire_block) = self.expire_block {
            assert_eq!(name_state.expire_block, expire_block, "expire_block does not match");
        }
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

    let bid_block = 1234;
    let res = Bid::on("example", "bidder", bid_block)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("bidder")
        .controller(None)
        .rate(0)
        .begin_block(bid_block)
        .begin_deposit(0)
        .counter_delay_end(1234 + 86400)
        .transition_delay_end(1234)
        .bid_delay_end(1234)
        .expire_block(None)
        .assert(&deps);
}

#[test]
fn initial_non_zero_bid() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_230_000;
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(194_513)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // The tax needed to be withheld from 1_230_000 at the rate of 0.405%.
    // Note that this is not simply 1_230_000 * 0.405% because amount plus tax
    // has to sum to 1_230_000.
    let tax_amount = 4962;

    // Assert funds message sent to collector
    let send_to_collector_msg = &res.messages[0];
    match send_to_collector_msg {
        CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, send }) => {
            assert_eq!(contract_addr.as_str(), "collector");
            assert_eq!(send, &coins(deposit_amount - tax_amount, ABC_COIN));

            let msg: CollectorHandleMsg = from_binary(&msg).unwrap();
            match msg {
                CollectorHandleMsg::AcceptFunds(AcceptFunds { source_addr }) => {
                    assert_eq!(source_addr.as_str(), "bidder");
                },
                _ => panic!("Unexpected message: {:?}", msg),
            }
        },
        _ => panic!("Unexpected message type: {:?}", send_to_collector_msg),
    }

    NameStateAsserter::new("example")
        .owner("bidder")
        .controller(None)
        .rate(194_513)
        .begin_block(bid_block)
        .begin_deposit(1_230_000)
        .counter_delay_end(1234 + 86400)
        .transition_delay_end(1234)
        .bid_delay_end(1234 + 86400 + 2254114)
        .expire_block(Some(1234 + 6323484))
        .assert(&deps);
}

#[test]
fn initial_bid_outside_of_allowed_block_range() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_230_000;

    // High rate results in too few blocks leased
    let res = Bid::on("example_1", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(545_669)
        .handle(&mut deps);
    assert_eq!(res.is_err(), true);

    // Lower rate is successful
    Bid::on("example_1", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(545_668)
        .handle(&mut deps)
        .unwrap();

    // Low rate results in too many blocks leased
    let res = Bid::on("example_2", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(54566)
        .handle(&mut deps);
    assert_eq!(res.is_err(), true);

    // Higher rate is successful
    Bid::on("example_2", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(54567)
        .handle(&mut deps)
        .unwrap();
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
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Bid on the name as the current owner. Not allowed.
    let bid_2_block: u64 = 2000;
    let deposit_amount = 2_000;
    let res = Bid::on("example", "bidder_1", bid_2_block)
        .deposit(deposit_amount)
        .rate(246)
        .handle(&mut deps);
    assert_eq!(res.is_err(), true);

    // Bid on the name as the current owner after expiry. This is allowed.
    let bid_2_block: u64 = bid_1_block + 8130081;
    let deposit_amount = 2_000;
    let res = Bid::on("example", "bidder_1", bid_2_block)
        .deposit(deposit_amount)
        .rate(246)
        .handle(&mut deps)
        .unwrap();
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
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 0);

    // Bid within counter delay. For a zero rate name this should not matter as
    // a counter bid can be posted any time.
    let bid_2_block: u64 = 2000;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    NameStateAsserter::new("example")
        .owner("bidder_2")
        .controller(None)
        .rate(123)
        .begin_block(bid_2_block)
        .begin_deposit(1_000)
        .counter_delay_end(bid_2_block + 86400)
        .transition_delay_end(bid_2_block + 86400 + 259200)
        .bid_delay_end(bid_2_block + 86400 + 2254114)
        .expire_block(Some(bid_2_block + 8130081))
        .assert(&deps);
}

#[test]
fn bid_on_existing_zero_rate_name_after_counter_delay() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block: u64 = 1234;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 0);

    // Bid after counter delay. For a zero rate name this should not matter as
    // a counter bid can be posted any time.
    let bid_2_block: u64 = bid_1_block + 86400;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    NameStateAsserter::new("example")
        .owner("bidder_2")
        .controller(None)
        .rate(123)
        .begin_block(bid_2_block)
        .begin_deposit(1_000)
        .counter_delay_end(bid_2_block + 86400)
        .transition_delay_end(bid_2_block + 86400 + 259200)
        .bid_delay_end(bid_2_block + 86400 + 2254114)
        .expire_block(Some(bid_2_block + 8130081))
        .assert(&deps);
}

#[test]
fn bid_three_bidders() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Initial bid
    let bid_1_block = 1234;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // First counter following the bid delay
    let bid_2_block = bid_1_block + 86400 + 2254114;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(124)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Countered by third bidder
    let bid_3_block = bid_2_block + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_3", bid_3_block)
        .deposit(deposit_amount)
        .rate(125)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    NameStateAsserter::new("example")
        .owner("bidder_3")
        .previous_owner(Some("bidder_1"))
        .rate(125)
        .begin_block(bid_3_block)
        .begin_deposit(1_000)
        .counter_delay_end(bid_3_block + 86400)
        .transition_delay_end(bid_3_block + 86400 + 259200)
        .bid_delay_end(bid_3_block + 86400 + 2254114)
        .expire_block(Some(bid_3_block + 8000000))
        .assert(&deps);
}

// Bid by A, starts ownership. Then bid by B, then by C, then counter bid by
// A. A should continue as owner and this should not trigger a transition.
#[test]
fn bid_is_counter_bid_then_countered() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Initial bid
    let bid_1_block = 1234;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // First counter following the bid delay
    let bid_2_block = bid_1_block + 86400 + 2254114;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(124)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Second counter
    let bid_3_block = bid_2_block + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_3", bid_3_block)
        .deposit(deposit_amount)
        .rate(125)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Countered by initial owner
    let bid_4_block = bid_3_block + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_4_block)
        .deposit(deposit_amount)
        .rate(200)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    NameStateAsserter::new("example")
        .owner("bidder_1")
        .rate(200)
        .begin_block(bid_4_block)
        .begin_deposit(1_000)
        .counter_delay_end(bid_4_block + 86400)
        .transition_delay_end(bid_4_block)
        .bid_delay_end(bid_4_block + 86400 + 2254114)
        .expire_block(Some(bid_4_block + 5000000))
        .assert(&deps);
}

// Bid on expired name.
#[test]
fn bid_on_expired_name() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Initial bid
    let bid_1_block = 1234;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Bid after expiration
    let bid_2_block = bid_1_block + 8130081 + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(110)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Transition delay should be based on when the name expired.
    NameStateAsserter::new("example")
        .owner("bidder_2")
        .previous_owner(None)
        .rate(110)
        .begin_block(bid_2_block)
        .begin_deposit(1_000)
        .counter_delay_end(bid_2_block + 86400)
        .transition_delay_end(bid_1_block + 8130081 + 86400 + 259200)
        .bid_delay_end(bid_2_block + 86400 + 2254114)
        .expire_block(Some(bid_2_block + 9090909))
        .assert(&deps);
}

// Bid on name that expired during a transition
#[test]
fn bid_on_expired_name_in_transition() {
    let mut deps = mock_dependencies(20, &[]);

    // Change min lease blocks so the name can expire during a transition.
    let mut msg = default_init();
    msg.min_lease_blocks = msg.counter_delay_blocks;
    msg.bid_delay_blocks = 10000;
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Initial bid
    let bid_1_block = 1234;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Second bid
    let bid_2_block = bid_1_block + 86400 + 10000;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(130)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Request rate change to let it expire during transition
    let set_rate_block = bid_2_block + 86400;
    let env = mock_env("bidder_2", &[]).at_block_height(set_rate_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(10000u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Bid after expiration
    let bid_3_block = set_rate_block + 98800;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_3", bid_3_block)
        .deposit(deposit_amount)
        .rate(125)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // TODO Transition delay could be based on the beginning of the transition
    // before the name expired but is currently based on when it expired.
    NameStateAsserter::new("example")
        .owner("bidder_3")
        .previous_owner(None)
        .rate(125)
        .begin_block(bid_3_block)
        .begin_deposit(1_000)
        .counter_delay_end(bid_3_block + 86400)
        .transition_delay_end(bid_3_block + 86400 + 259200)
        .bid_delay_end(bid_3_block + 86400 + 10000)
        .expire_block(Some(bid_3_block + 8000000))
        .assert(&deps);
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
    let res = Bid::on("example", "bidder", bid_block)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 0);

    // Funding a zero rate name is not possible.
    let fund_block = 5000;
    let env = mock_env("funder", &coins(1_000_000, ABC_COIN))
        .at_block_height(fund_block);
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
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Funding an owned name is possible up to the max block limit.
    let fund_block = 4_000_000;
    let deposit_amount: u128 = 2_000;
    let tax_amount = 9;

    let env = mock_env("funder", &coins(deposit_amount, ABC_COIN))
        .at_block_height(fund_block);
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
                CollectorHandleMsg::AcceptFunds(AcceptFunds { source_addr }) => {
                    assert_eq!(source_addr.as_str(), "funder");
                },
            }
        },
        _ => panic!("Unexpected message type"),
    }

    NameStateAsserter::new("example")
        .owner("bidder")
        .rate(123)
        .begin_block(bid_block)
        .begin_deposit(3_000)
        .counter_delay_end(bid_block + 86400)
        .transition_delay_end(1234)
        .bid_delay_end(1234 + 86400 + 2254114)
        .expire_block(Some(1234 + 24390243))
        .assert(&deps);
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
    let res = Bid::on("example", "bidder_1", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Bidder 2 submits a bid while funder is preparing to fund bidder 1.
    let bid_block = 1235;
    let deposit_amount: u128 = 2_000;
    let res = Bid::on("example", "bidder_2", bid_block)
        .deposit(deposit_amount)
        .rate(246)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Funder submits funding simultaneously but bidder_2 transaction happens
    // first.
    let fund_block = 1236;
    let deposit_amount: u128 = 1_000;
    let env = mock_env("funder", &coins(deposit_amount, ABC_COIN))
        .at_block_height(fund_block);
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
    let res = Bid::on("example", "bidder_1", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Funder submits funding request without adding funds or with the wrong
    // coin.
    let fund_block = 1236;
    let deposit_amount: u128 = 1_000;
    let env = mock_env("funder", &coins(deposit_amount, NOT_ABC_COIN))
        .at_block_height(fund_block);
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
    let res = Bid::on("example", "bidder_1", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Funder submits funding request adding too much funds pushing the lease
    // over the max limit.
    let fund_block = 1236;
    let deposit_amount: u128 = 1_773;
    let env = mock_env("funder", &coins(deposit_amount, ABC_COIN))
        .at_block_height(fund_block);
    let res = handle(&mut deps, env, HandleMsg::FundName {
        name: "example".to_string(),
        owner: HumanAddr::from("bidder_1"),
    });
    assert_eq!(res.is_err(), true);

    // Reduce the funding under the limit should result in success.
    let deposit_amount: u128 = 1_772;
    let env = mock_env("funder", &coins(deposit_amount, ABC_COIN))
        .at_block_height(fund_block);
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
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to decrease the charged rate
    let rate_change_block = 100_000;
    let env = mock_env("bidder", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(98u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("bidder")
        .previous_owner(Some("bidder"))
        .rate(98)
        .begin_block(rate_change_block)
        .begin_deposit(987)
        .counter_delay_end(rate_change_block + 86400)
        .transition_delay_end(rate_change_block)
        .bid_delay_end(rate_change_block + 86400 + 2254114)
        .expire_block(Some(rate_change_block + 10071428))
        .assert(&deps);
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
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to increase the charged rate
    let rate_change_block = 100_000;
    let env = mock_env("bidder", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("bidder")
        .previous_owner(Some("bidder"))
        .rate(246)
        .begin_block(rate_change_block)
        .begin_deposit(987)
        .counter_delay_end(rate_change_block + 86400)
        .transition_delay_end(rate_change_block)
        .bid_delay_end(rate_change_block + 86400 + 2254114)
        .expire_block(Some(rate_change_block + 4012195))
        .assert(&deps);
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
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // First bidder submits requests to change the charged rate
    let rate_change_block = 2400;
    let env = mock_env("bidder_1", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    });
    assert_eq!(res.is_err(), true);

    let bid_2_block = 2500;
    let deposit_amount: u128 = 1_001;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(124)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Second bidder submits requests to change the charged rate
    let rate_change_block = 80000;
    let env = mock_env("bidder_2", &[]).at_block_height(rate_change_block);
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
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    let bid_2_block = 2400;
    let deposit_amount: u128 = 1_001;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(124)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // First bidder submits requests to change the charged rate
    let rate_change_block = 200_000;
    let env = mock_env("bidder_1", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(246u64),
    });
    assert_eq!(res.is_err(), true);
}

#[test]
fn set_rate_outside_of_lower_bound_fails() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to increase the charged rate
    let rate_change_block = 100_000;
    let env = mock_env("bidder", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(438u64),
    });
    assert_eq!(res.is_err(), true);

    // Success with lower rate
    let env = mock_env("bidder", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(437u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);
}

#[test]
fn set_rate_outside_of_upper_bound_fails() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_block = 1234;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to decrease the charged rate
    let rate_change_block = 100_000;
    let env = mock_env("bidder", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(43u64),
    });
    assert_eq!(res.is_err(), true);

    // Success with higher rate
    let env = mock_env("bidder", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(44u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);
}

// Rate change by A. This should trigger a counter delay of allowed bidding
// and a transition when a counter bid wins.
#[test]
fn set_rate_allows_bidding_do_transition() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block = 1234;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to decrease the charged rate
    let rate_change_block = 100_000;
    let env = mock_env("bidder_1", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(120u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    let bid_2_block = rate_change_block + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(121)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    NameStateAsserter::new("example")
        .owner("bidder_2")
        .previous_owner(Some("bidder_1"))
        .rate(121)
        .begin_block(bid_2_block)
        .begin_deposit(1000)
        .counter_delay_end(bid_2_block + 86400)
        .transition_delay_end(bid_2_block + 86400 + 259200)
        .bid_delay_end(bid_2_block + 86400 + 2254114)
        .expire_block(Some(bid_2_block + 8264462))
        .assert(&deps);
}

// Rate change by A. This should trigger a counter delay of allowed bidding
// and no transition when the counter bid does not win.
#[test]
fn set_rate_allows_bidding_no_transition() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block = 1234;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to decrease the charged rate
    let rate_change_block = 100_000;
    let env = mock_env("bidder_1", &[]).at_block_height(rate_change_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(120u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Counter bid by other bidder
    let bid_2_block = rate_change_block + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(121)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Countered by original owner
    let bid_3_block = bid_2_block + 1000;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_3_block)
        .deposit(deposit_amount)
        .rate(122)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    NameStateAsserter::new("example")
        .owner("bidder_1")
        .previous_owner(Some("bidder_1"))
        .rate(122)
        .begin_block(bid_3_block)
        .begin_deposit(1000)
        .counter_delay_end(bid_3_block + 86400)
        .transition_delay_end(bid_3_block)
        .bid_delay_end(bid_3_block + 86400 + 2254114)
        .expire_block(Some(bid_3_block + 8196721))
        .assert(&deps);
}

// Rate change by A. This should trigger a counter delay of allowed bidding
// and a continuation of the existing transition when the counter does not win.
#[test]
fn set_rate_allows_bidding_continued_transition() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block = 1234;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner submits requests to decrease the charged rate
    let rate_change_1_block = 100_000;
    let env = mock_env("bidder_1", &[]).at_block_height(rate_change_1_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(120u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Counter bid by other bidder
    let bid_2_block = rate_change_1_block + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(121)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // New owner requests change to decrease the charged rate during transition
    let rate_change_2_block = bid_2_block + 100_000;
    let env = mock_env("bidder_2", &[]).at_block_height(rate_change_2_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameRate {
        name: "example".to_string(),
        rate: Uint128::from(120u64),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    // Counter bid by third bidder
    let bid_3_block = rate_change_2_block + 100;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_3", bid_3_block)
        .deposit(deposit_amount)
        .rate(121)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Countered by second owner
    let bid_4_block = bid_3_block + 1000;
    let deposit_amount = 1_000;
    let res = Bid::on("example", "bidder_2", bid_4_block)
        .deposit(deposit_amount)
        .rate(122)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    NameStateAsserter::new("example")
        .owner("bidder_2")
        .previous_owner(Some("bidder_2"))
        .rate(122)
        .begin_block(bid_4_block)
        .begin_deposit(1000)
        .counter_delay_end(bid_4_block + 86400)
        .transition_delay_end(bid_2_block + 86400 + 259200)
        .bid_delay_end(bid_4_block + 86400 + 2254114)
        .expire_block(Some(bid_4_block + 8196721))
        .assert(&deps);
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
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Ownership transferred
    let transfer_block = 100_000;
    let env = mock_env("bidder", &[]).at_block_height(transfer_block);
    let res = handle(&mut deps, env, HandleMsg::TransferNameOwner {
        name: "example".to_string(),
        to: HumanAddr::from("receiver"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("receiver")
        .begin_block(bid_block)
        .begin_deposit(deposit_amount)
        .counter_delay_end(bid_block + 86400)
        .transition_delay_end(bid_block)
        .bid_delay_end(bid_block + 86400 + 2254114)
        .expire_block(Some(bid_block + 8130081))
        .assert(&deps);
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
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Another bid occurs
    let bid_2_block = 2342748;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(124)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Original owner can transfer their expiring ownership
    let transfer_block = 2342749;
    let env = mock_env("bidder_1", &[]).at_block_height(transfer_block);
    let res = handle(&mut deps, env, HandleMsg::TransferNameOwner {
        name: "example".to_string(),
        to: HumanAddr::from("receiver_1"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("bidder_2")
        .begin_block(bid_2_block)
        .begin_deposit(deposit_amount)
        .counter_delay_end(bid_2_block + 86400)
        .transition_delay_end(bid_2_block + 86400 + 259200)
        .bid_delay_end(bid_2_block + 86400 + 2254114)
        .expire_block(Some(bid_2_block + 8064516))
        .assert(&deps);

    // Highest bid owner can also transfer their ownership of the bid and
    // potential future ownership of the name.
    let env = mock_env("bidder_2", &[]).at_block_height(transfer_block);
    let res = handle(&mut deps, env, HandleMsg::TransferNameOwner {
        name: "example".to_string(),
        to: HumanAddr::from("receiver_2"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("receiver_2")
        .begin_block(bid_2_block)
        .begin_deposit(deposit_amount)
        .previous_owner(Some("receiver_1"))
        .assert(&deps);
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
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Ownership transfer fails
    let transfer_block = 100_000;
    let env = mock_env("other", &[]).at_block_height(transfer_block);
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
    let res = Bid::on("example", "bidder", bid_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Owner cannot set controller before end of counter delay
    let set_controller_block = bid_block + 86400 - 1;
    let env = mock_env("bidder", &[]).at_block_height(set_controller_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller"),
    });
    assert_eq!(res.is_err(), true);

    // Owner can set controller after end of counter delay
    let set_controller_block = bid_block + 86400;
    let env = mock_env("bidder", &[]).at_block_height(set_controller_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("bidder")
        .controller(Some("controller"))
        .assert(&deps);
}

#[test]
fn set_controller_during_counter_delay() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bid_1_block = 1234;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(123)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Another bid occurs
    let bid_2_block = 2342748;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("example", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(124)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 2);

    // Original owner can set controller
    let transfer_block = 2342749;
    let env = mock_env("bidder_1", &[]).at_block_height(transfer_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller_1"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("bidder_2")
        .controller(Some("controller_1"))
        .assert(&deps);

    // Highest bid owner cannot set controller before end of counter delay
    let set_controller_block = 2342750;
    let env = mock_env("bidder_2", &[]).at_block_height(set_controller_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller_2"),
    });
    assert_eq!(res.is_err(), true);

    // After the counter delay ends, the highest bidder can set the controller
    let set_controller_block = 2432750;
    let env = mock_env("bidder_2", &[]).at_block_height(set_controller_block);
    let res = handle(&mut deps, env, HandleMsg::SetNameController {
        name: "example".to_string(),
        controller: HumanAddr::from("controller_2"),
    }).unwrap();
    assert_eq!(res.messages.len(), 0);

    NameStateAsserter::new("example")
        .owner("bidder_2")
        .controller(Some("controller_2"))
        .assert(&deps);
}

#[test]
fn query_all_name_states() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &[]);

    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // First bid
    let bid_1_block = 1234;
    let deposit_amount: u128 = 5_670;
    let res = Bid::on("example", "bidder_1", bid_1_block)
        .deposit(deposit_amount)
        .rate(600)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Second bid
    let bid_2_block = 2342748;
    let deposit_amount: u128 = 1_000;
    let res = Bid::on("other", "bidder_2", bid_2_block)
        .deposit(deposit_amount)
        .rate(124)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    // Third bid
    let bid_3_block = 2367901;
    let deposit_amount: u128 = 1_200_000;
    let res = Bid::on("abc-def", "bidder_1", bid_3_block)
        .deposit(deposit_amount)
        .rate(140_000)
        .handle(&mut deps)
        .unwrap();
    assert_eq!(res.messages.len(), 1);

    let res = query(&deps, QueryMsg::GetAllNameStates {
        start_after: None,
        limit: Some(2),
    }).unwrap();
    let state: AllNameStatesResponse = from_binary(&res).unwrap();
    assert_eq!(state.names.len(), 2);

    assert_eq!(state.names[0].name, "abc-def");
    assert_eq!(state.names[0].state.rate.u128(), 140_000);
    assert_eq!(state.names[1].name, "example");
    assert_eq!(state.names[1].state.rate.u128(), 600);

    // Query for second page
    let res = query(&deps, QueryMsg::GetAllNameStates {
        start_after: Some("example".into()),
        limit: Some(2),
    }).unwrap();
    let state: AllNameStatesResponse = from_binary(&res).unwrap();
    assert_eq!(state.names.len(), 1);

    assert_eq!(state.names[0].name, "other");
    assert_eq!(state.names[0].state.rate.u128(), 124);
}

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
