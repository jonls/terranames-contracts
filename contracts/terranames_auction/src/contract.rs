use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, BankMsg, Coin, CosmosMsg, Deps,
    DepsMut, Env, MessageInfo, QuerierWrapper, QueryResponse, StdResult,
    Response, Uint128,
};

use terranames::auction::{
    deposit_from_blocks_ceil, deposit_from_blocks_floor, ConfigResponse,
    AllNameStatesResponse, ExecuteMsg, InstantiateMsg, MigrateMsg,
    NameStateItem, NameStateResponse, QueryMsg,
};
use terranames::terra::deduct_coin_tax;

use crate::errors::{
    BidDepositTooLow, BidInvalidBlockCount, BidRateTooLow, ClosedForBids,
    ContractError, InvalidConfig, Unauthorized, UnexpectedState, Unfunded,
};
use crate::state::{
    collect_name_states, read_config, read_name_state, read_option_name_state,
    store_config, store_name_state, Config, NameState, OwnerStatus,
};

type ContractResult<T> = Result<T, ContractError>;

/// Return the funds of type denom attached in the request.
fn get_sent_funds(info: &MessageInfo, denom: &str) -> Uint128 {
    info.funds
        .iter()
        .find(|c| c.denom == denom)
        .map(|c| c.amount)
        .unwrap_or_else(Uint128::zero)
}

/// Create message for refund deposits
///
/// Idea: Store refunds in this contract instead of sending them back
/// immediately, in order to avoid repeated tax on transfers. Instead users can
/// use the refund balance in calls needing funds. Also need a separate call to
/// actually send the refund balance back.
fn refund_deposit_msg(
    querier: &QuerierWrapper,
    _env: &Env,
    config: &Config,
    to: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Bank(
        BankMsg::Send {
            to_address: to.into(),
            amount: vec![
                deduct_coin_tax(
                    querier,
                    Coin {
                        denom: config.stable_denom.clone(),
                        amount,
                    },
                )?
            ],
        }
    ))
}

/// Create message for sending deposits to collector
///
/// Currently just sends funds to the collector. In the future this could use
/// a WasmMsg to trigger some kind of accounting in the collector and/or giving
/// some priviledge to the sender (e.g. reward tokens or something like that).
fn send_to_collector_msg(
    querier: &QuerierWrapper,
    _env: &Env,
    config: &Config,
    _source_addr: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Bank(
        BankMsg::Send {
            to_address: config.collector_addr.to_string(),
            amount: vec![
                deduct_coin_tax(
                    querier,
                    Coin {
                        denom: config.stable_denom.clone(),
                        amount,
                    },
                )?
            ],
        }
    ))
}

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> ContractResult<Response> {
    let collector_addr = deps.api.addr_validate(&msg.collector_addr)?;

    if !(msg.min_lease_blocks <= msg.max_lease_blocks) {
        return InvalidConfig.fail();
    }

    let state = Config {
        collector_addr,
        stable_denom: msg.stable_denom,
        min_lease_blocks: msg.min_lease_blocks,
        max_lease_blocks: msg.max_lease_blocks,
        counter_delay_blocks: msg.counter_delay_blocks,
        transition_delay_blocks: msg.transition_delay_blocks,
        bid_delay_blocks: msg.bid_delay_blocks,
    };

    store_config(deps.storage, &state)?;

    Ok(Response::default())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> ContractResult<Response> {
    match msg {
        ExecuteMsg::BidName { name, rate } => {
            execute_bid(deps, env, info, name, rate)
        },
        ExecuteMsg::FundName { name, owner } => {
            let owner = deps.api.addr_validate(&owner)?;
            execute_fund(deps, env, info, name, owner)
        },
        ExecuteMsg::SetNameRate { name, rate } => {
            execute_set_rate(deps, env, info, name, rate)
        },
        ExecuteMsg::TransferNameOwner { name, to } => {
            let to = deps.api.addr_validate(&to)?;
            execute_transfer_owner(deps, env, info, name, to)
        },
        ExecuteMsg::SetNameController { name, controller } => {
            let controller = deps.api.addr_validate(&controller)?;
            execute_set_controller(deps, env, info, name, controller)
        },
    }
}

fn execute_bid(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    rate: Uint128,
) -> ContractResult<Response> {
    if let Some(name_state) = read_option_name_state(deps.storage, &name)? {
        let config = read_config(deps.storage)?;
        let owner_status = name_state.owner_status(&config, env.block.height);
        match owner_status {
            OwnerStatus::Valid { owner, transition_reference_block } |
            OwnerStatus::TransitionDelay { owner, transition_reference_block } => {
                execute_bid_existing(
                    deps, env, info, name, rate, config, name_state, Some(owner),
                    transition_reference_block,
                )
            },
            OwnerStatus::CounterDelay { name_owner: owner, transition_reference_block, .. } => {
                execute_bid_existing(
                    deps, env, info, name, rate, config, name_state, owner,
                    transition_reference_block,
                )
            },
            OwnerStatus::Expired { expire_block, .. } => {
                execute_bid_new(
                    deps, env, info, name, rate, expire_block,
                )
            },
        }
    } else {
        execute_bid_new(deps, env, info, name, rate, 0)
    }
}

fn execute_bid_existing(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    rate: Uint128,
    config: Config,
    mut name_state: NameState,
    owner: Option<Addr>,
    transition_reference_block: u64,
) -> ContractResult<Response> {
    if info.sender == name_state.owner {
        return Unauthorized.fail();
    }

    let blocks_spent_since_bid = match name_state.blocks_spent_since_bid(env.block.height) {
        Some(blocks_spent) => blocks_spent,
        None => panic!("Invalid block height"),
    };

    if blocks_spent_since_bid >= config.counter_delay_blocks &&
            blocks_spent_since_bid < config.counter_delay_blocks + config.bid_delay_blocks &&
            !name_state.rate.is_zero() {
        return ClosedForBids.fail();
    }

    if rate <= name_state.rate {
        return BidRateTooLow {
            rate: name_state.rate,
        }.fail();
    }

    let msg_deposit = get_sent_funds(&info, &config.stable_denom);
    let deposit_spent = deposit_from_blocks_ceil(blocks_spent_since_bid, name_state.rate);
    let deposit_left = name_state.begin_deposit.saturating_sub(deposit_spent);

    if msg_deposit <= deposit_left {
        return BidDepositTooLow {
            deposit: deposit_left,
        }.fail();
    }

    // TODO I'm uncertain if these limits are necessary though they may be good
    // to have as very wide ranges to avoid unpredictable edge cases near the
    // edges. The lower limit probably needs to be at least
    // counter_delay_blocks to avoid an attack where a name is bid on with a
    // very short time to expiry then the rate resets after expiry.
    let min_deposit = deposit_from_blocks_ceil(config.min_lease_blocks, rate);
    let max_deposit = deposit_from_blocks_floor(config.max_lease_blocks, rate);
    if msg_deposit < min_deposit || msg_deposit > max_deposit {
        return BidInvalidBlockCount.fail();
    }

    let previous_bidder = name_state.owner;

    name_state.previous_owner = owner.clone();
    name_state.previous_transition_reference_block = transition_reference_block;
    name_state.owner = info.sender.clone();
    name_state.rate = rate;
    name_state.begin_block = env.block.height;
    name_state.begin_deposit = msg_deposit;

    // Only update transition reference block if ownership is assigned to a new
    // owner.
    if Some(name_state.owner.clone()) != owner {
        name_state.transition_reference_block = env.block.height;
    } else {
        name_state.transition_reference_block = name_state.previous_transition_reference_block;
    }

    store_name_state(deps.storage, &name, &name_state)?;

    let mut messages = vec![];

    // Refund previous owner
    if !deposit_left.is_zero() {
        messages.push(
            refund_deposit_msg(
                &deps.querier,
                &env,
                &config,
                &previous_bidder,
                deposit_left,
            )?,
        );
    }

    // TODO query for the contract balance instead of using msg_deposit
    // in order to drain dust etc. out of the contract.

    // Send excess deposit to collector
    let excess_deposit = msg_deposit.checked_sub(deposit_left)?;
    messages.push(
        send_to_collector_msg(
            &deps.querier,
            &env,
            &config,
            &info.sender,
            excess_deposit,
        )?
    );

    let mut attributes = vec![
        attr("action", "bid"),
        attr("owner", info.sender),
        attr("rate", rate),
        attr("deposit", msg_deposit),
        attr("refund", deposit_left),
    ];

    if let Some(previous_owner) = name_state.previous_owner {
        attributes.push(
            attr("previous_owner", previous_owner),
        );
    }

    Ok(Response {
        messages,
        attributes,
        data: None,
        submessages: vec![],
    })
}

fn execute_bid_new(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    rate: Uint128,
    transition_reference_block: u64,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let msg_deposit = get_sent_funds(&info, &config.stable_denom);
    let begin_block = env.block.height;

    let min_deposit = deposit_from_blocks_ceil(config.min_lease_blocks, rate);
    let max_deposit = deposit_from_blocks_floor(config.max_lease_blocks, rate);
    if msg_deposit < min_deposit || msg_deposit > max_deposit {
        return BidInvalidBlockCount.fail();
    }

    let name_state = NameState {
        owner: info.sender.clone(),
        controller: None,
        transition_reference_block,

        begin_block: begin_block,
        begin_deposit: msg_deposit,
        rate: rate,

        previous_owner: None,
        previous_transition_reference_block: 0,
    };
    store_name_state(deps.storage, &name, &name_state)?;

    let mut messages = vec![];

    // Send deposit to fund
    if !msg_deposit.is_zero() {
        messages.push(
            send_to_collector_msg(
                &deps.querier,
                &env,
                &config,
                &info.sender,
                msg_deposit,
            )?
        );
    }

    Ok(Response {
        messages,
        attributes: vec![
            attr("action", "bid"),
            attr("owner", info.sender),
            attr("rate", rate),
            attr("deposit", msg_deposit),
        ],
        data: None,
        submessages: vec![],
    })
}

fn execute_fund(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    owner: Addr,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let msg_deposit = get_sent_funds(&info, &config.stable_denom);
    let mut name_state = read_name_state(deps.storage, &name)?;

    if msg_deposit.is_zero() {
        return Unfunded.fail();
    }

    let owner_canonical = owner;
    if name_state.owner != owner_canonical {
        return UnexpectedState.fail();
    }

    let combined_deposit = msg_deposit + name_state.begin_deposit;
    let max_deposit = name_state.max_allowed_deposit(&config, env.block.height);
    if combined_deposit > max_deposit {
        return BidInvalidBlockCount.fail();
    }

    name_state.begin_deposit = combined_deposit;
    store_name_state(deps.storage, &name, &name_state)?;

    let mut messages = vec![];

    // Send deposit to fund
    messages.push(
        send_to_collector_msg(
            &deps.querier,
            &env,
            &config,
            &info.sender,
            msg_deposit,
        )?
    );

    Ok(Response {
        messages,
        attributes: vec![
            attr("action", "fund"),
            attr("deposit", combined_deposit),
        ],
        data: None,
        submessages: vec![],
    })
}

fn execute_set_rate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    rate: Uint128,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let mut name_state = read_name_state(deps.storage, &name)?;
    let sender_canonical = info.sender;
    let owner_status = name_state.owner_status(&config, env.block.height);

    if !owner_status.can_set_rate(&sender_canonical) {
        return Unauthorized.fail();
    }

    // Always round up spent deposit to avoid charging too little.
    let blocks_spent = env.block.height - name_state.begin_block;
    let spent_deposit = deposit_from_blocks_ceil(blocks_spent, name_state.rate);
    let new_deposit = name_state.begin_deposit.saturating_sub(spent_deposit); // TODO <-- add test for this: last block spends slightly more than total deposit

    let min_deposit = deposit_from_blocks_ceil(config.min_lease_blocks, rate);
    let max_deposit = deposit_from_blocks_floor(config.max_lease_blocks, rate);

    if new_deposit < min_deposit || new_deposit > max_deposit {
        return BidInvalidBlockCount.fail();
    }

    name_state.rate = rate;
    name_state.begin_block = env.block.height;
    name_state.begin_deposit = new_deposit;
    name_state.previous_owner = Some(name_state.owner.clone());
    name_state.previous_transition_reference_block = name_state.transition_reference_block;
    store_name_state(deps.storage, &name, &name_state)?;

    Ok(Response {
        messages: vec![],
        attributes: vec![
            attr("action", "set_rate"),
            attr("rate", rate),
            attr("deposit", new_deposit),
        ],
        data: None,
        submessages: vec![],
    })
}

fn execute_transfer_owner(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    to: Addr,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let mut name_state = read_name_state(deps.storage, &name)?;
    let sender_canonical = info.sender;
    let owner_status = name_state.owner_status(&config, env.block.height);

    let new_owner = to;

    if owner_status.can_transfer_name_owner(&sender_canonical) {
        match owner_status {
            // In the counter-delay state, the current owner is determined by
            // previous_owner since owner is the current highest bid holder.
            OwnerStatus::CounterDelay { .. } => {
                name_state.previous_owner = Some(new_owner.clone());
            },
            _ => {
                name_state.owner = new_owner.clone();
            }
        }
    } else if owner_status.can_transfer_bid_owner(&sender_canonical) {
        // This lets the current highest bid holder transfer their bid.
        name_state.owner = new_owner.clone();
    } else {
        return Unauthorized.fail();
    }

    store_name_state(deps.storage, &name, &name_state)?;

    Ok(Response {
        messages: vec![],
        attributes: vec![
            attr("action", "transfer_owner"),
            attr("owner", new_owner),
        ],
        data: None,
        submessages: vec![],
    })
}

fn execute_set_controller(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    controller: Addr,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let mut name_state = read_name_state(deps.storage, &name)?;
    let sender_canonical = info.sender;
    let owner_status = name_state.owner_status(&config, env.block.height);

    if !owner_status.can_set_controller(&sender_canonical) {
        return Unauthorized.fail();
    }

    name_state.controller = Some(controller.clone());
    store_name_state(deps.storage, &name, &name_state)?;

    Ok(Response {
        messages: vec![],
        attributes: vec![
            attr("action", "set_controller"),
            attr("controller", controller),
        ],
        data: None,
        submessages: vec![],
    })
}

#[entry_point]
pub fn query(
    deps: Deps,
    _env: Env,
    msg: QueryMsg,
) -> ContractResult<QueryResponse> {
    match msg {
        QueryMsg::Config {} => {
            Ok(to_binary(&query_config(deps)?)?)
        },
        QueryMsg::GetNameState { name } => {
            Ok(to_binary(&query_name_state(deps, name)?)?)
        },
        QueryMsg::GetAllNameStates { start_after, limit } => {
            Ok(to_binary(&query_all_name_states(deps, start_after, limit)?)?)
        },
    }
}

fn query_config(
    deps: Deps,
) -> ContractResult<ConfigResponse> {
    let config = read_config(deps.storage)?;

    Ok(ConfigResponse {
        collector_addr: config.collector_addr,
        stable_denom: config.stable_denom,
        min_lease_blocks: config.min_lease_blocks,
        max_lease_blocks: config.max_lease_blocks,
        counter_delay_blocks: config.counter_delay_blocks,
        transition_delay_blocks: config.transition_delay_blocks,
        bid_delay_blocks: config.bid_delay_blocks,
    })
}

fn query_name_state(
    deps: Deps,
    name: String,
) -> ContractResult<NameStateResponse> {
    let config = read_config(deps.storage)?;
    let name_state = read_name_state(deps.storage, &name)?;

    let counter_delay_end = name_state.counter_delay_end(&config);
    let transition_delay_end = name_state.transition_delay_end(&config);
    let bid_delay_end = name_state.bid_delay_end(&config);
    let expire_block = name_state.expire_block();

    Ok(NameStateResponse {
        owner: name_state.owner,
        controller: name_state.controller,
        rate: name_state.rate,
        begin_block: name_state.begin_block,
        begin_deposit: name_state.begin_deposit,
        previous_owner: name_state.previous_owner,
        counter_delay_end,
        transition_delay_end,
        bid_delay_end,
        expire_block,
    })
}

fn query_all_name_states(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> ContractResult<AllNameStatesResponse> {
    let config = read_config(deps.storage)?;
    let name_states = collect_name_states(
        deps.storage,
        start_after.as_deref(),
        limit,
    )?;

    let names: StdResult<Vec<NameStateItem>> = name_states.into_iter().map(|(name, name_state)| {
        let counter_delay_end = name_state.counter_delay_end(&config);
        let transition_delay_end = name_state.transition_delay_end(&config);
        let bid_delay_end = name_state.bid_delay_end(&config);
        let expire_block = name_state.expire_block();

        Ok(NameStateItem {
            name,
            state: NameStateResponse {
                owner: name_state.owner,
                controller: name_state.controller,
                rate: name_state.rate,
                begin_block: name_state.begin_block,
                begin_deposit: name_state.begin_deposit,
                previous_owner: name_state.previous_owner,
                counter_delay_end,
                transition_delay_end,
                bid_delay_end,
                expire_block,
           }
        })
    }).collect();

    Ok(AllNameStatesResponse {
        names: names?,
    })
}

#[entry_point]
pub fn migrate(
    _deps: DepsMut,
    _env: Env,
    _msg: MigrateMsg,
) -> ContractResult<Response> {
    Ok(Response::default())
}
