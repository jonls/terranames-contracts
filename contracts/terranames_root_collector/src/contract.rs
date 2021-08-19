use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, BankMsg, Coin, CosmosMsg,
    Decimal, Deps, DepsMut, Env, MessageInfo, QuerierWrapper, QueryResponse,
    Response, StdResult, Uint128, WasmMsg,
};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use terranames::root_collector::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, ReceiveMsg,
    StakeStateResponse, StateResponse, QueryMsg,
};
use terranames::terra::deduct_coin_tax;

use crate::errors::{
    ContractError, InsufficientFunds, InsufficientTokens, Unauthorized,
};
use crate::state::{
    read_config, read_option_stake_state, read_stake_state, read_state,
    store_config, store_stake_state, store_state, Config, StakeState, State,
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

/// Create message for dividend deposits
fn send_dividend_msg(
    querier: &QuerierWrapper,
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

/// Create message for sending tokens
fn send_tokens_msg(
    config: &Config,
    recipient: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(
        WasmMsg::Execute {
            contract_addr: config.base_token.clone().into(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient.into(),
                amount,
            })?,
            funds: vec![],
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
    let config = Config {
        base_token: deps.api.addr_validate(&msg.base_token)?,
        stable_denom: msg.stable_denom,
        unstake_delay: msg.unstake_delay,
    };

    store_config(deps.storage, &config)?;

    let state = State {
        multiplier: Decimal::zero(),
        total_staked: Uint128::zero(),
        residual: Uint128::zero(),
    };

    store_state(deps.storage, &state)?;

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
        ExecuteMsg::Deposit {} => {
            execute_deposit(deps, env, info)
        },
        ExecuteMsg::UnstakeTokens { amount } => {
            execute_unstake_tokens(deps, env, info, amount)
        },
        ExecuteMsg::WithdrawTokens { amount, to } => {
            let to_addr = to.map(|to| deps.api.addr_validate(&to)).transpose()?;
            execute_withdraw_tokens(deps, env, info, amount, to_addr)
        },
        ExecuteMsg::WithdrawDividends { to } => {
            let to_addr = to.map(|to| deps.api.addr_validate(&to)).transpose()?;
            execute_withdraw_dividends(deps, env, info, to_addr)
        },
        ExecuteMsg::Receive(msg) => {
            execute_receive(deps, env, info, msg)
        },
    }
}

fn execute_deposit(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    let deposit = get_sent_funds(&info, &config.stable_denom) + state.residual;
    let (deposit_per_stake, residual) = if !state.total_staked.is_zero() {
        let deposit_per_stake = Decimal::from_ratio(deposit, state.total_staked);
        let residual = deposit.checked_sub(deposit_per_stake * state.total_staked)?;
        (deposit_per_stake, residual)
    } else {
        (Decimal::zero(), deposit)
    };

    state.multiplier = state.multiplier + deposit_per_stake;
    state.residual = residual;

    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "deposit")
        .add_attribute("amount", deposit)
        .add_attribute("multiplier", state.multiplier.to_string())
        .add_attribute("residual", state.residual)
    )
}

fn execute_unstake_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    let opt_stake_state = read_option_stake_state(deps.storage, &info.sender)?;
    let mut stake_state = if let Some(stake_state) = opt_stake_state {
        stake_state
    } else {
        return InsufficientTokens.fail();
    };

    if stake_state.staked_amount < amount {
        return InsufficientTokens.fail();
    }

    stake_state.update_dividend(state.multiplier);
    stake_state.update_unstaked_amount(env.block.time.into(), config.unstake_delay);

    stake_state.staked_amount = stake_state.staked_amount.checked_sub(amount)?;
    stake_state.unstaking_amount += amount;
    stake_state.unstaking_begin_time = Some(env.block.time.into());
    store_stake_state(deps.storage, &info.sender, &stake_state)?;

    state.total_staked = state.total_staked.checked_sub(amount)?;
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "unstake_tokens")
        .add_attribute("staked_amount", stake_state.staked_amount)
    )
}

fn execute_withdraw_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    to: Option<Addr>,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;

    let opt_stake_state = read_option_stake_state(deps.storage, &info.sender)?;
    let mut stake_state = if let Some(stake_state) = opt_stake_state {
        stake_state
    } else {
        return InsufficientTokens.fail();
    };

    stake_state.update_unstaked_amount(env.block.time.into(), config.unstake_delay);

    if stake_state.unstaked_amount < amount {
        return InsufficientTokens.fail();
    }

    let mut messages = vec![];

    stake_state.unstaked_amount = stake_state.unstaked_amount.checked_sub(amount)?;
    store_stake_state(deps.storage, &info.sender, &stake_state)?;

    messages.push(
        send_tokens_msg(
            &config,
            &to.unwrap_or(info.sender),
            amount,
        )?,
    );

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "withdraw_tokens")
        .add_attribute("unstaked_amount", stake_state.unstaked_amount)
    )
}

fn execute_withdraw_dividends(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    to: Option<Addr>,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let state = read_state(deps.storage)?;

    let opt_stake_state = read_option_stake_state(deps.storage, &info.sender)?;
    let mut stake_state = if let Some(stake_state) = opt_stake_state {
        stake_state
    } else {
        return InsufficientFunds.fail();
    };

    stake_state.update_dividend(state.multiplier);
    if stake_state.dividend.is_zero() {
        return InsufficientFunds.fail();
    }

    let mut messages = vec![];
    messages.push(
        send_dividend_msg(
            &deps.querier,
            &config,
            &to.unwrap_or(info.sender.clone()),
            stake_state.dividend,
        )?,
    );

    stake_state.dividend = Uint128::zero();
    store_stake_state(deps.storage, &info.sender, &stake_state)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "withdraw_dividends")
    )
}

fn execute_receive(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> ContractResult<Response> {
    let msg: ReceiveMsg = from_binary(&wrapper.msg)?;

    let config = read_config(deps.storage)?;

    if info.sender != config.base_token {
        return Unauthorized.fail();
    }

    match msg {
        ReceiveMsg::Stake {} => {
            execute_receive_stake(deps, wrapper)
        },
    }
}

fn execute_receive_stake(
    deps: DepsMut,
    wrapper: Cw20ReceiveMsg,
) -> ContractResult<Response> {
    let mut state = read_state(deps.storage)?;

    let token_sender = deps.api.addr_validate(&wrapper.sender)?;
    let opt_stake_state = read_option_stake_state(deps.storage, &token_sender)?;

    let stake_state = if let Some(mut stake_state) = opt_stake_state {
        stake_state.update_dividend(state.multiplier);
        stake_state.staked_amount += wrapper.amount;
        stake_state
    } else {
        StakeState {
            multiplier: state.multiplier,
            staked_amount: wrapper.amount,
            unstaking_amount: Uint128::zero(),
            unstaked_amount: Uint128::zero(),
            unstaking_begin_time: None,
            dividend: Uint128::zero(),
        }
    };
    store_stake_state(deps.storage, &token_sender, &stake_state)?;

    state.total_staked += wrapper.amount;
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "receive")
        .add_attribute("receive_type", "stake")
        .add_attribute("staked_amount", stake_state.staked_amount)
        .add_attribute("multiplier", stake_state.multiplier.to_string())
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
            Ok(to_binary(&query_config(deps, env)?)?)
        },
        QueryMsg::State {} => {
            Ok(to_binary(&query_state(deps, env)?)?)
        },
        QueryMsg::StakeState { address } => {
            Ok(to_binary(&query_stake_state(
                deps,
                env,
                deps.api.addr_validate(&address)?,
            )?)?)
        },
    }
}

fn query_config(
    deps: Deps,
    _env: Env,
) -> ContractResult<ConfigResponse> {
    let config = read_config(deps.storage)?;
    Ok(ConfigResponse {
        base_token: config.base_token.into(),
        stable_denom: config.stable_denom,
        unstake_delay: config.unstake_delay,
    })
}

fn query_state(
    deps: Deps,
    _env: Env,
) -> ContractResult<StateResponse> {
    let state = read_state(deps.storage)?;
    Ok(StateResponse {
        multiplier: state.multiplier,
        total_staked: state.total_staked,
        residual: state.residual,
    })
}

fn query_stake_state(
    deps: Deps,
    env: Env,
    address: Addr,
) -> ContractResult<StakeStateResponse> {
    let config = read_config(deps.storage)?;
    let state = read_state(deps.storage)?;
    let stake_state = read_stake_state(deps.storage, &address)?;

    let (unstaking_amount, unstaked_amount) = stake_state.unstaking_unstaked_amount(
        env.block.time.into(), config.unstake_delay,
    );
    let unstake_time = stake_state.unstaking_begin_time.map(|t| t + config.unstake_delay);

    let dividend = stake_state.dividend(state.multiplier);

    Ok(StakeStateResponse {
        staked_amount: stake_state.staked_amount,
        unstaking_amount,
        unstake_time,
        unstaked_amount,
        multiplier: stake_state.multiplier,
        dividend,
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
