use cosmwasm_std::{
    log, to_binary, Api, BankMsg, Binary, CanonicalAddr, Coin, CosmosMsg,
    Env, Extern, HandleResponse, HandleResult, HumanAddr, InitResponse,
    InitResult, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use terranames::auction::{
    deposit_from_blocks_ceil, deposit_from_blocks_floor, ConfigResponse,
    HandleMsg, InitMsg, NameStateResponse, QueryMsg,
};
use terranames::collector::{
    AcceptFunds, HandleMsg as CollectorHandleMsg,
};
use terranames::terra::deduct_coin_tax;

use crate::state::{
    read_config, read_name_state, read_option_name_state, store_config,
    store_name_state, Config, NameState, OwnerStatus,
};

/// Return the funds of type denom attached in the request.
fn get_sent_funds(env: &Env, denom: &str) -> Uint128 {
    env.message
        .sent_funds
        .iter()
        .find(|c| c.denom == denom)
        .map(|c| c.amount)
        .unwrap_or_else(Uint128::zero)
}

/// Create message for refund deposits
fn refund_deposit_msg<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
    config: &Config,
    to: &HumanAddr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Bank(
        BankMsg::Send {
            from_address: env.contract.address.clone(),
            to_address: to.clone(),
            amount: vec![
                deduct_coin_tax(
                    &deps,
                    Coin {
                        denom: config.stable_denom.clone(),
                        amount,
                    },
                )?
            ],
        }
    ))
}

/// Create message for sending deposits to fund
fn send_to_collector_msg<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
    config: &Config,
    source_addr: &HumanAddr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(
        WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.collector_addr)?,
            msg: to_binary(
                &CollectorHandleMsg::AcceptFunds(AcceptFunds {
                    source_addr: source_addr.clone(),
                }),
            )?,
            send: vec![
                deduct_coin_tax(
                    &deps,
                    Coin {
                        denom: config.stable_denom.clone(),
                        amount,
                    },
                )?
            ],
        }
    ))
}


pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> InitResult {
    let collector_addr = deps.api.canonical_address(&msg.collector_addr)?;

    if !(msg.min_lease_blocks <= msg.max_lease_blocks) {
        return Err(StdError::generic_err("Invalid min/max lease blocks"));
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

    store_config(&mut deps.storage, &state)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> HandleResult {
    match msg {
        HandleMsg::BidName { name, rate } => {
            handle_bid(deps, env, name, rate)
        },
        HandleMsg::FundName { name, owner } => {
            handle_fund(deps, env, name, owner)
        },
        HandleMsg::SetNameRate { name, rate } => {
            handle_set_rate(deps, env, name, rate)
        },
        HandleMsg::TransferNameOwner { name, to } => {
            handle_transfer_owner(deps, env, name, to)
        },
        HandleMsg::SetNameController { name, controller } => {
            handle_set_controller(deps, env, name, controller)
        },
    }
}

fn handle_bid<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    rate: Uint128,
) -> HandleResult {
    if let Some(name_state) = read_option_name_state(&deps.storage, &name)? {
        let config = read_config(&deps.storage)?;
        let owner_status = name_state.owner_status(&config, env.block.height);
        match owner_status {
            OwnerStatus::Valid { owner, transition_reference_block } |
            OwnerStatus::CounterDelay { name_owner: owner, transition_reference_block, .. } |
            OwnerStatus::TransitionDelay { owner, transition_reference_block } => {
                handle_bid_existing(
                    deps, env, name, rate, config, name_state, owner,
                    transition_reference_block,
                )
            },
            OwnerStatus::Expired { expire_block, .. } => {
                handle_bid_new(
                    deps, env, name, rate, expire_block,
                )
            },
        }
    } else {
        handle_bid_new(deps, env, name, rate, 0)
    }
}

fn handle_bid_existing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    rate: Uint128,
    config: Config,
    mut name_state: NameState,
    owner: CanonicalAddr,
    transition_reference_block: u64,
) -> HandleResult {
    let sender_canonical = deps.api.canonical_address(&env.message.sender)?;
    // TODO rethink how this works when the bid delay is over but there are no
    // counter bids? Currently this means that the owner cannot lock in
    // ownership for another bid delay period because they cannot even bump the
    // bid up slightly to trigger a new begin_block (maybe they can with rate
    // change?).
    if sender_canonical == name_state.owner {
        return Err(StdError::generic_err("Cannot bid as current owner"));
    }

    let blocks_spent_since_bid = match name_state.blocks_spent_since_bid(env.block.height) {
        Some(blocks_spent) => blocks_spent,
        None => return Err(StdError::generic_err("Invalid block height"))
    };

    if blocks_spent_since_bid >= config.counter_delay_blocks &&
            blocks_spent_since_bid < config.counter_delay_blocks + config.bid_delay_blocks &&
            !name_state.rate.is_zero() {
        return Err(StdError::generic_err("Unable to bid in the current block"));
    }

    if rate <= name_state.rate {
        return Err(StdError::generic_err(format!(
            "Bid rate must be greater than {} {}",
            name_state.rate, config.stable_denom,
        )));
    }

    let msg_deposit = get_sent_funds(&env, &config.stable_denom);
    let deposit_spent = deposit_from_blocks_ceil(blocks_spent_since_bid, name_state.rate);
    let deposit_left = (name_state.begin_deposit - deposit_spent).unwrap_or_else(|_| Uint128::zero());

    if msg_deposit <= deposit_left {
        return Err(StdError::generic_err(format!(
            "Deposit must be greater than {} {}",
            deposit_left, config.stable_denom,
        )))
    }

    // TODO I'm uncertain if these limits are necessary though they may be good
    // to have as very wide ranges to avoid unpredictable edge cases near the
    // edges. The lower limit probably needs to be at least
    // counter_delay_blocks to avoid an attack where a name is bid on with a
    // very short time to expiry then the rate resets after expiry.
    let min_deposit = deposit_from_blocks_ceil(config.min_lease_blocks, rate);
    let max_deposit = deposit_from_blocks_floor(config.max_lease_blocks, rate);
    if msg_deposit < min_deposit || msg_deposit > max_deposit {
        return Err(StdError::generic_err(
            "Deposit is outside of the allowed range",
        ));
    }

    let previous_bidder = name_state.owner;

    name_state.previous_owner = owner;
    name_state.previous_transition_reference_block = transition_reference_block;
    name_state.owner = sender_canonical;
    name_state.rate = rate;
    name_state.begin_block = env.block.height;
    name_state.begin_deposit = msg_deposit;

    // Only update transition reference block if ownership is assigned to a new
    // owner.
    if name_state.owner != name_state.previous_owner {
        name_state.transition_reference_block = env.block.height;
    } else {
        name_state.transition_reference_block = name_state.previous_transition_reference_block;
    }

    store_name_state(&mut deps.storage, &name, &name_state)?;

    let mut messages = vec![];

    // Refund previous owner
    if !deposit_left.is_zero() {
        messages.push(
            refund_deposit_msg(
                deps,
                &env,
                &config,
                &deps.api.human_address(&previous_bidder)?,
                deposit_left,
            )?,
        );
    }

    // Send excess deposit to collector
    let excess_deposit = (msg_deposit - deposit_left)?;
    messages.push(
        send_to_collector_msg(
            deps,
            &env,
            &config,
            &env.message.sender,
            excess_deposit,
        )?
    );

    let mut logs = vec![
        log("action", "bid"),
        log("owner", env.message.sender),
        log("rate", rate),
        log("deposit", msg_deposit),
        log("refund", deposit_left),
    ];

    if name_state.previous_owner != CanonicalAddr::default() {
        logs.push(
            log("previous_owner", deps.api.human_address(&name_state.previous_owner)?),
        );
    }

    Ok(HandleResponse {
        messages,
        log: logs,
        data: None,
    })
}

fn handle_bid_new<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    rate: Uint128,
    transition_reference_block: u64,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let msg_deposit = get_sent_funds(&env, &config.stable_denom);
    let begin_block = env.block.height;

    let min_deposit = deposit_from_blocks_ceil(config.min_lease_blocks, rate);
    let max_deposit = deposit_from_blocks_floor(config.max_lease_blocks, rate);
    if msg_deposit < min_deposit || msg_deposit > max_deposit {
        return Err(StdError::generic_err(
            "Deposit is outside of the allowed range",
        ));
    }

    let sender_canonical = deps.api.canonical_address(&env.message.sender)?;
    let name_state = NameState {
        owner: sender_canonical.clone(),
        controller: CanonicalAddr::default(),
        transition_reference_block,

        begin_block: begin_block,
        begin_deposit: msg_deposit,
        rate: rate,

        previous_owner: CanonicalAddr::default(),
        previous_transition_reference_block: 0,
    };
    store_name_state(&mut deps.storage, &name, &name_state)?;

    let mut messages = vec![];

    // Send deposit to fund
    if !msg_deposit.is_zero() {
        messages.push(
            send_to_collector_msg(
                deps,
                &env,
                &config,
                &env.message.sender,
                msg_deposit,
            )?
        );
    }

    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "bid"),
            log("owner", env.message.sender),
            log("rate", rate),
            log("deposit", msg_deposit),
        ],
        data: None,
    })
}

fn handle_fund<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    owner: HumanAddr,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let msg_deposit = get_sent_funds(&env, &config.stable_denom);
    let mut name_state = read_name_state(&deps.storage, &name)?;

    if msg_deposit.is_zero() {
        return Err(StdError::generic_err("Fund deposit is zero"));
    }

    let owner_canonical = deps.api.canonical_address(&owner)?;
    if name_state.owner != owner_canonical {
        return Err(StdError::generic_err("Owner does not match expectation"));
    }

    let combined_deposit = msg_deposit + name_state.begin_deposit;
    let max_deposit = name_state.max_allowed_deposit(&config, env.block.height);
    if combined_deposit > max_deposit {
        return Err(StdError::generic_err(format!(
            "Deposit outside of allowed range, max allowed additional: {} {}",
            (max_deposit - name_state.begin_deposit).unwrap_or_else(|_| Uint128::zero()),
            config.stable_denom,
        )));
    }

    name_state.begin_deposit = combined_deposit;
    store_name_state(&mut deps.storage, &name, &name_state)?;

    let mut messages = vec![];

    // Send deposit to fund
    messages.push(
        send_to_collector_msg(
            deps,
            &env,
            &config,
            &env.message.sender,
            msg_deposit,
        )?
    );

    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "fund"),
            log("deposit", combined_deposit),
        ],
        data: None,
    })
}

fn handle_set_rate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    rate: Uint128,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let mut name_state = read_name_state(&deps.storage, &name)?;
    let sender_canonical = deps.api.canonical_address(&env.message.sender)?;
    let owner_status = name_state.owner_status(&config, env.block.height);

    if !owner_status.can_set_rate(&sender_canonical) {
        return Err(StdError::unauthorized());
    }

    // Always round up spent deposit to avoid charging too little.
    let blocks_spent = env.block.height - name_state.begin_block;
    let spent_deposit = deposit_from_blocks_ceil(blocks_spent, name_state.rate);
    let new_deposit = (
        name_state.begin_deposit - spent_deposit
    ).unwrap_or_else(|_| Uint128::zero()); // TODO <-- add test for this: last block spends slightly more than total deposit

    let min_deposit = deposit_from_blocks_ceil(config.min_lease_blocks, rate);
    let max_deposit = deposit_from_blocks_floor(config.max_lease_blocks, rate);

    if new_deposit < min_deposit || new_deposit > max_deposit {
        return Err(StdError::generic_err(
            "Rate results in a lease outside of the allowed block range"
        ));
    }

    name_state.rate = rate;
    name_state.begin_block = env.block.height;
    name_state.begin_deposit = new_deposit;
    name_state.previous_owner = name_state.owner.clone();
    name_state.previous_transition_reference_block = name_state.transition_reference_block;
    store_name_state(&mut deps.storage, &name, &name_state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "set_rate"),
            log("rate", rate),
            log("deposit", new_deposit),
        ],
        data: None,
    })
}

fn handle_transfer_owner<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    to: HumanAddr,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let mut name_state = read_name_state(&deps.storage, &name)?;
    let sender_canonical = deps.api.canonical_address(&env.message.sender)?;
    let owner_status = name_state.owner_status(&config, env.block.height);

    let new_owner = deps.api.canonical_address(&to)?;

    if owner_status.can_transfer_name_owner(&sender_canonical) {
        match owner_status {
            // In the counter-delay state, the current owner is determined by
            // previous_owner since owner is the current highest bid holder.
            OwnerStatus::CounterDelay { .. } => {
                name_state.previous_owner = new_owner.clone();
            },
            _ => {
                name_state.owner = new_owner.clone();
            }
        }
    } else if owner_status.can_transfer_bid_owner(&sender_canonical) {
        // This lets the current highest bid holder transfer their bid.
        name_state.owner = new_owner.clone();
    } else {
        return Err(StdError::unauthorized());
    }

    store_name_state(&mut deps.storage, &name, &name_state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "transfer_owner"),
            log("owner", new_owner),
        ],
        data: None,
    })
}

fn handle_set_controller<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    controller: HumanAddr,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let mut name_state = read_name_state(&deps.storage, &name)?;
    let sender_canonical = deps.api.canonical_address(&env.message.sender)?;
    let owner_status = name_state.owner_status(&config, env.block.height);

    if !owner_status.can_set_controller(&sender_canonical) {
        return Err(StdError::unauthorized());
    }

    name_state.controller = deps.api.canonical_address(&controller)?;
    store_name_state(&mut deps.storage, &name, &name_state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "set_controller"),
            log("controller", controller),
        ],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            to_binary(&query_config(deps)?)
        },
        QueryMsg::GetNameState { name } => {
            to_binary(&query_name_state(deps, name)?)
        },
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config = read_config(&deps.storage)?;

    Ok(ConfigResponse {
        collector_addr: deps.api.human_address(&config.collector_addr)?,
        stable_denom: config.stable_denom,
        min_lease_blocks: config.min_lease_blocks,
        max_lease_blocks: config.max_lease_blocks,
        counter_delay_blocks: config.counter_delay_blocks,
        transition_delay_blocks: config.transition_delay_blocks,
        bid_delay_blocks: config.bid_delay_blocks,
    })
}

fn query_name_state<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    name: String,
) -> StdResult<NameStateResponse> {
    let config = read_config(&deps.storage)?;
    let name_state = read_name_state(&deps.storage, &name)?;

    let previous_owner = if name_state.previous_owner != CanonicalAddr::default() {
        Some(deps.api.human_address(&name_state.previous_owner)?)
    } else {
        None
    };

    let controller = if name_state.controller != CanonicalAddr::default() {
        Some(deps.api.human_address(&name_state.controller)?)
    } else {
        None
    };

    Ok(NameStateResponse {
        owner: deps.api.human_address(&name_state.owner)?,
        controller,
        rate: name_state.rate,
        begin_block: name_state.begin_block,
        begin_deposit: name_state.begin_deposit,
        previous_owner,
        counter_delay_end: name_state.counter_delay_end(&config),
        transition_delay_end: name_state.transition_delay_end(&config),
        bid_delay_end: name_state.bid_delay_end(&config),
        expire_block: name_state.expire_block(),
    })
}
