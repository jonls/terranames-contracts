use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, BankMsg, Coin, CosmosMsg, Deps,
    DepsMut, Env, MessageInfo, QuerierWrapper, QueryResponse, Response,
    StdResult, Uint128, WasmMsg,
};

use terranames::auction::{
    deposit_from_seconds_ceil, deposit_from_seconds_floor, ConfigResponse,
    AllNameStatesResponse, ExecuteMsg, InstantiateMsg, MigrateMsg,
    NameStateItem, NameStateResponse, QueryMsg,
};
use terranames::root_collector::{
    ExecuteMsg as RootCollectorExecuteMsg,
};
use terranames::terra::deduct_coin_tax;
use terranames::utils::Timestamp;

use crate::errors::{
    BidDepositTooLow, BidInvalidInterval, BidRateTooLow, ClosedForBids,
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
fn send_to_collector_msg(
    querier: &QuerierWrapper,
    _env: &Env,
    config: &Config,
    _source_addr: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(
        WasmMsg::Execute {
            contract_addr: config.collector_addr.to_string(),
            msg: to_binary(&RootCollectorExecuteMsg::Deposit {})?,
            funds: vec![
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> ContractResult<Response> {
    let collector_addr = deps.api.addr_validate(&msg.collector_addr)?;

    if !(msg.min_lease_secs <= msg.max_lease_secs) {
        return InvalidConfig.fail();
    }

    let state = Config {
        collector_addr,
        stable_denom: msg.stable_denom,
        min_lease_secs: msg.min_lease_secs,
        max_lease_secs: msg.max_lease_secs,
        counter_delay_secs: msg.counter_delay_secs,
        transition_delay_secs: msg.transition_delay_secs,
        bid_delay_secs: msg.bid_delay_secs,
    };

    store_config(deps.storage, &state)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
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
        let owner_status = name_state.owner_status(&config, env.block.time.into());
        match owner_status {
            OwnerStatus::Valid { owner, transition_reference_time } |
            OwnerStatus::TransitionDelay { owner, transition_reference_time } => {
                execute_bid_existing(
                    deps, env, info, name, rate, config, name_state, Some(owner),
                    transition_reference_time,
                )
            },
            OwnerStatus::CounterDelay { name_owner: owner, transition_reference_time, .. } => {
                execute_bid_existing(
                    deps, env, info, name, rate, config, name_state, owner,
                    transition_reference_time,
                )
            },
            OwnerStatus::Expired { expire_time, .. } => {
                execute_bid_new(
                    deps, env, info, name, rate, expire_time,
                )
            },
        }
    } else {
        execute_bid_new(deps, env, info, name, rate, Timestamp::zero())
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
    transition_reference_time: Timestamp,
) -> ContractResult<Response> {
    if info.sender == name_state.owner {
        return Unauthorized.fail();
    }

    let seconds_spent_since_bid = match name_state.seconds_spent_since_bid(env.block.time.into()) {
        Some(seconds_spent) => seconds_spent,
        None => panic!("Invalid block time"),
    };

    if seconds_spent_since_bid >= config.counter_delay_secs &&
            seconds_spent_since_bid < config.counter_delay_secs + config.bid_delay_secs &&
            !name_state.rate.is_zero() {
        return ClosedForBids.fail();
    }

    if rate <= name_state.rate {
        return BidRateTooLow {
            rate: name_state.rate,
        }.fail();
    }

    let msg_deposit = get_sent_funds(&info, &config.stable_denom);
    let deposit_spent = deposit_from_seconds_ceil(seconds_spent_since_bid, name_state.rate);
    let deposit_left = name_state.begin_deposit.saturating_sub(deposit_spent);

    // TODO Consider adding a small delta that could be given back to the previous
    // bidder to cover tx fees.
    if msg_deposit <= deposit_left {
        return BidDepositTooLow {
            deposit: deposit_left,
        }.fail();
    }

    // TODO Consider allowing the existing owner a slightly higher max deposit
    // at the same rate. The increase in deposit could be equal to the rate
    // increase times the length of the previous lease plus a small penalty.
    let min_deposit = deposit_from_seconds_ceil(config.min_lease_secs, rate);
    let max_deposit = deposit_from_seconds_floor(config.max_lease_secs, rate);
    if msg_deposit < min_deposit || msg_deposit > max_deposit {
        return BidInvalidInterval.fail();
    }

    let previous_bidder = name_state.owner;

    name_state.previous_owner = owner.clone();
    name_state.previous_transition_reference_time = transition_reference_time;
    name_state.owner = info.sender.clone();
    name_state.rate = rate;
    name_state.begin_time = env.block.time.into();
    name_state.begin_deposit = msg_deposit;

    // Only update transition reference time if ownership is assigned to a new
    // owner.
    if Some(name_state.owner.clone()) != owner {
        name_state.transition_reference_time = env.block.time.into();
    } else {
        name_state.transition_reference_time = name_state.previous_transition_reference_time;
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
        )?,
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

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes)
    )
}

fn execute_bid_new(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    rate: Uint128,
    transition_reference_time: Timestamp,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let msg_deposit = get_sent_funds(&info, &config.stable_denom);
    let begin_time = env.block.time.into();

    let min_deposit = deposit_from_seconds_ceil(config.min_lease_secs, rate);
    let max_deposit = deposit_from_seconds_floor(config.max_lease_secs, rate);
    if msg_deposit < min_deposit || msg_deposit > max_deposit {
        return BidInvalidInterval.fail();
    }

    let name_state = NameState {
        owner: info.sender.clone(),
        controller: None,
        transition_reference_time,

        begin_time,
        begin_deposit: msg_deposit,
        rate,

        previous_owner: None,
        previous_transition_reference_time: Timestamp::zero(),
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
            )?,
        );
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "bid")
        .add_attribute("owner", info.sender)
        .add_attribute("rate", rate)
        .add_attribute("deposit", msg_deposit)
    )
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
    let max_deposit = name_state.max_allowed_deposit(&config, env.block.time.into());
    if combined_deposit > max_deposit {
        return BidInvalidInterval.fail();
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
        )?,
    );

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "fund")
        .add_attribute("deposit", combined_deposit)
    )
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
    let owner_status = name_state.owner_status(&config, env.block.time.into());

    if !owner_status.can_set_rate(&sender_canonical) {
        return Unauthorized.fail();
    }

    // Always round up spent deposit to avoid charging too little.
    let seconds_spent = Timestamp::from(env.block.time).checked_sub(name_state.begin_time)?;
    let spent_deposit = deposit_from_seconds_ceil(seconds_spent, name_state.rate);
    let new_deposit = name_state.begin_deposit.saturating_sub(spent_deposit); // TODO <-- add test for this: last block spends slightly more than total deposit

    let min_deposit = deposit_from_seconds_ceil(config.min_lease_secs, rate);
    let max_deposit = deposit_from_seconds_floor(config.max_lease_secs, rate);

    if new_deposit < min_deposit || new_deposit > max_deposit {
        return BidInvalidInterval.fail();
    }

    name_state.rate = rate;
    name_state.begin_time = env.block.time.into();
    name_state.begin_deposit = new_deposit;
    name_state.previous_owner = Some(name_state.owner.clone());
    name_state.previous_transition_reference_time = name_state.transition_reference_time;
    store_name_state(deps.storage, &name, &name_state)?;

    Ok(Response::new()
        .add_attribute("action", "set_rate")
        .add_attribute("rate", rate)
        .add_attribute("deposit", new_deposit)
    )
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
    let owner_status = name_state.owner_status(&config, env.block.time.into());

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

    Ok(Response::new()
        .add_attribute("action", "transfer_owner")
        .add_attribute("owner", new_owner)
    )
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
    let owner_status = name_state.owner_status(&config, env.block.time.into());

    if !owner_status.can_set_controller(&sender_canonical) {
        return Unauthorized.fail();
    }

    name_state.controller = Some(controller.clone());
    store_name_state(deps.storage, &name, &name_state)?;

    Ok(Response::new()
        .add_attribute("action", "set_controller")
        .add_attribute("controller", controller)
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    deps: Deps,
    env: Env,
    msg: QueryMsg,
) -> ContractResult<QueryResponse> {
    match msg {
        QueryMsg::Config {} => {
            Ok(to_binary(&query_config(deps)?)?)
        },
        QueryMsg::GetNameState { name } => {
            Ok(to_binary(&query_name_state(deps, env, name)?)?)
        },
        QueryMsg::GetAllNameStates { start_after, limit } => {
            Ok(to_binary(&query_all_name_states(deps, env, start_after, limit)?)?)
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
        min_lease_secs: config.min_lease_secs,
        max_lease_secs: config.max_lease_secs,
        counter_delay_secs: config.counter_delay_secs,
        transition_delay_secs: config.transition_delay_secs,
        bid_delay_secs: config.bid_delay_secs,
    })
}

fn create_name_state_response(
    config: &Config,
    current_time: Timestamp,
    name_state: &NameState,
) -> NameStateResponse {
    let counter_delay_end = name_state.counter_delay_end(config);
    let transition_delay_end = name_state.transition_delay_end(config);
    let bid_delay_end = name_state.bid_delay_end(config);
    let expire_time = name_state.expire_time();

    let owner_status = name_state.owner_status(&config, current_time);
    let current_deposit = name_state.current_deposit(current_time);

    let (name_owner, bid_owner) = match owner_status {
        OwnerStatus::Expired { .. } =>
            (None, None),
        OwnerStatus::CounterDelay { name_owner, bid_owner, .. } =>
            (name_owner, Some(bid_owner)),
        OwnerStatus::Valid { owner, .. } | OwnerStatus::TransitionDelay { owner, ..} =>
            (Some(owner.clone()), Some(owner)),
    };

    NameStateResponse {
        name_owner,
        bid_owner,
        controller: name_state.controller.clone(),
        rate: name_state.rate,
        begin_time: name_state.begin_time,
        begin_deposit: name_state.begin_deposit,
        current_deposit,
        counter_delay_end,
        transition_delay_end,
        bid_delay_end,
        expire_time,
    }
}

fn query_name_state(
    deps: Deps,
    env: Env,
    name: String,
) -> ContractResult<NameStateResponse> {
    let config = read_config(deps.storage)?;
    let name_state = read_name_state(deps.storage, &name)?;

    Ok(create_name_state_response(&config, env.block.time.into(), &name_state))
}

fn query_all_name_states(
    deps: Deps,
    env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> ContractResult<AllNameStatesResponse> {
    let config = read_config(deps.storage)?;
    let name_states = collect_name_states(
        deps.storage,
        start_after.as_deref(),
        limit,
    )?;

    let names: Vec<NameStateItem> = name_states.into_iter().map(|(name, name_state)| {
        let state = create_name_state_response(
            &config, env.block.time.into(), &name_state,
        );

        Ok(NameStateItem {
            name,
            state,
        })
    }).collect::<StdResult<Vec<_>>>()?;

    Ok(AllNameStatesResponse {
        names,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    _deps: DepsMut,
    _env: Env,
    _msg: MigrateMsg,
) -> ContractResult<Response> {
    Ok(Response::default())
}
