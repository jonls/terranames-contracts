use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env,
    Extern, HandleResponse, HandleResult, MigrateResponse, MigrateResult,
    InitResponse, InitResult, Querier, QueryRequest, StdError, StdResult,
    Storage, Uint128, WasmMsg, WasmQuery,
};

use cw20::{Cw20Contract, Cw20HandleMsg, Cw20ReceiveMsg};
use terranames::root_collector::{
    ConfigResponse, HandleMsg, InitMsg, MigrateMsg, ReceiveMsg, StateResponse,
    QueryMsg,
};
use terranames::terra::{calculate_added_tax, calculate_tax, deduct_tax};
use terranames::utils::FractionInv;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    HandleMsg as PairHandleMsg, QueryMsg as PairQueryMsg,
    PoolResponse,
};
use terraswap::querier::query_pair_info;

use crate::state::{
    read_config, read_state, store_config, store_state, Config, State,
};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> InitResult {
    if msg.min_token_price.is_zero() {
        return Err(StdError::generic_err("min_token_price must be non-zero"));
    }

    let terranames_token = deps.api.canonical_address(&msg.terranames_token)?;
    let terraswap_factory = deps.api.canonical_address(&msg.terraswap_factory)?;

    // Query for the swap contract
    let pair = query_pair_info(
        deps, &deps.api.human_address(&terraswap_factory)?, &[
            AssetInfo::Token {
                contract_addr: deps.api.human_address(&terranames_token)?,
            },
            AssetInfo::NativeToken {
                denom: msg.stable_denom.clone(),
            },
        ]
    )?;

    let config = Config {
        terranames_token: deps.api.canonical_address(&msg.terranames_token)?,
        terraswap_pair: deps.api.canonical_address(&pair.contract_addr)?,
        stable_denom: msg.stable_denom,
        min_token_price: msg.min_token_price,
    };

    store_config(&mut deps.storage, &config)?;

    let state = State {
        initial_token_pool: Uint128::zero(),
    };

    store_state(&mut deps.storage, &state)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> HandleResult {
    match msg {
        HandleMsg::ConsumeExcessStable {} => {
            handle_consume_excess_stable(deps, env)
        },
        HandleMsg::ConsumeExcessTokens {} => {
            handle_consume_excess_tokens(deps, env)
        },
        HandleMsg::Receive(msg) => {
            handle_receive(deps, env, msg)
        },
    }
}

fn handle_consume_excess_stable<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let mut state = read_state(&deps.storage)?;

    // Query for current stable coin balance
    let stable_balance = deps.querier.query_balance(
        env.contract.address.clone(),
        &config.stable_denom,
    )?.amount;
    if stable_balance.is_zero() {
        return Err(StdError::generic_err("No stable coin funds exist to consume"));
    }

    // Query for the current token balance
    let token_balance = Cw20Contract(
        deps.api.human_address(&config.terranames_token)?,
    ).balance(
        &deps.querier,
        env.contract.address,
    )?;

    let mut messages = vec![];

    let mut stable_to_send = stable_balance;
    let tokens_to_release_left = std::cmp::min(token_balance, state.initial_token_pool);
    if !tokens_to_release_left.is_zero() {
        // Query for the swap pool exchange rate
        let pair_pool = deps.querier.query::<PoolResponse>(&QueryRequest::Wasm(
            WasmQuery::Smart {
                contract_addr: deps.api.human_address(&config.terraswap_pair)?,
                msg: to_binary(
                    &PairQueryMsg::Pool {},
                )?,
            }
        ))?;

        let (pool_tokens, pool_stables) = match (&pair_pool.assets[0].info, &pair_pool.assets[1].info) {
            (&AssetInfo::NativeToken { .. }, &AssetInfo::Token { .. }) => {
                (pair_pool.assets[1].amount, pair_pool.assets[0].amount)
            },
            (&AssetInfo::Token { .. }, &AssetInfo::NativeToken { .. }) => {
                (pair_pool.assets[0].amount, pair_pool.assets[1].amount)
            },
            _ => {
                return Err(StdError::generic_err("Unexpected pool data"));
            },
        };

        let max_provide_stable_tax = calculate_tax(deps, &config.stable_denom, stable_to_send)?;
        let max_provide_stable = (stable_to_send - max_provide_stable_tax)?;
        let max_tokens_per_stable = config.min_token_price.inv()
            .ok_or(StdError::generic_err("Invalid token price"))?;

        let stable_price_in_tokens = if !pool_stables.is_zero() {
            std::cmp::min(
                Decimal::from_ratio(pool_tokens, pool_stables),
                max_tokens_per_stable,
            )
        } else {
            max_tokens_per_stable
        };
        let max_provide_tokens = max_provide_stable * stable_price_in_tokens;

        // Reduce stablecoins provided if max threshold is hit
        let (provide_tokens, provide_stable, provide_stable_tax) = if max_provide_tokens > tokens_to_release_left {
            let min_stables_per_token = config.min_token_price;
            let token_price_in_stables = if !pool_tokens.is_zero() {
                std::cmp::max(
                    Decimal::from_ratio(pool_stables, pool_tokens),
                    min_stables_per_token,
                )
            } else {
                min_stables_per_token
            };
            let reduced_provide_stable = tokens_to_release_left * token_price_in_stables;
            (tokens_to_release_left, reduced_provide_stable, calculate_added_tax(deps, &config.stable_denom, reduced_provide_stable)?)
        } else {
            (max_provide_tokens, max_provide_stable, max_provide_stable_tax)
        };

        // Allow swap pair to withdraw the tokens
        messages.push(
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: deps.api.human_address(&config.terranames_token)?,
                    msg: to_binary(
                        &Cw20HandleMsg::IncreaseAllowance {
                            spender: deps.api.human_address(&config.terraswap_pair)?,
                            amount: provide_tokens,
                            expires: None,
                        },
                    )?,
                    send: vec![],
                }
            ),
        );

        // Provide tokens and stablecoins to liquidity pool
        messages.push(
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: deps.api.human_address(&config.terraswap_pair)?,
                    msg: to_binary(
                        &PairHandleMsg::ProvideLiquidity {
                            assets: [
                                Asset {
                                    info: AssetInfo::Token {
                                        contract_addr: deps.api.human_address(&config.terranames_token)?,
                                    },
                                    amount: provide_tokens,
                                },
                                Asset {
                                    info: AssetInfo::NativeToken {
                                        denom: config.stable_denom.clone(),
                                    },
                                    amount: provide_stable,
                                },
                            ],
                            slippage_tolerance: None,
                        },
                    )?,
                    send: vec![
                        Coin {
                            denom: config.stable_denom.clone(),
                            amount: provide_stable,
                        },
                    ],
                },
            ),
        );

        // Calculate how much of the deposit is left
        stable_to_send = Uint128::from(
            stable_to_send.u128().saturating_sub(provide_stable.u128()).saturating_sub(provide_stable_tax.u128())
        );
        // Calculate how much of the initial token pool is left
        state.initial_token_pool = Uint128::from(
            state.initial_token_pool.u128().saturating_sub(provide_tokens.u128())
        );
        store_state(&mut deps.storage, &state)?;
    }

    // Use remaining funds to buy back tokens
    if !stable_to_send.is_zero() {
        let remaining_after_tax = deduct_tax(deps, &config.stable_denom, stable_to_send)?;
        messages.push(
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: deps.api.human_address(&config.terraswap_pair)?,
                    msg: to_binary(
                        &PairHandleMsg::Swap {
                            offer_asset: Asset {
                                info: AssetInfo::NativeToken {
                                    denom: config.stable_denom.clone(),
                                },
                                amount: remaining_after_tax,
                            },
                            to: None,
                            belief_price: None,
                            max_spread: None,
                        },
                    )?,
                    send: vec![
                        Coin {
                            denom: config.stable_denom.clone(),
                            amount: remaining_after_tax,
                        },
                    ],
                },
            ),
        );
    }

    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "consume_excess_stable"),
            log("initial_token_pool", state.initial_token_pool),
        ],
        data: None,
    })
}

fn handle_consume_excess_tokens<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let state = read_state(&deps.storage)?;

    // Query for the current token balance
    let token_balance = Cw20Contract(
        deps.api.human_address(&config.terranames_token)?,
    ).balance(
        &deps.querier,
        env.contract.address,
    )?;

    let tokens_to_burn = Uint128::from(
        token_balance.u128().saturating_sub(state.initial_token_pool.u128())
    );
    if tokens_to_burn.is_zero() {
        return Err(StdError::generic_err("No tokens exist to consume"));
    }

    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: deps.api.human_address(&config.terranames_token)?,
                    msg: to_binary(
                        &Cw20HandleMsg::Burn {
                            amount: token_balance,
                        },
                    )?,
                    send: vec![],
                },
            )
        ],
        log: vec![
            log("action", "consume_excess_tokens"),
            log("tokens", token_balance),
        ],
        data: None,
    })
}

fn handle_receive<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    wrapper: Cw20ReceiveMsg,
) -> HandleResult {
    let msg: ReceiveMsg = match wrapper.msg {
        Some(bin) => from_binary(&bin),
        None => Err(StdError::generic_err("No message in cw20 receive")),
    }?;

    let config = read_config(&deps.storage)?;
    let mut state = read_state(&deps.storage)?;

    if deps.api.canonical_address(&env.message.sender)? != config.terranames_token {
        return Err(StdError::unauthorized());
    }

    match msg {
        ReceiveMsg::AcceptInitialTokens {} => {
            state.initial_token_pool += wrapper.amount;
            store_state(&mut deps.storage, &state)?;

            Ok(HandleResponse {
                messages: vec![],
                log: vec![
                    log("action", "receive"),
                    log("receive_type", "accept_initial_tokens"),
                    log("initial_token_pool", state.initial_token_pool),
                ],
                data: None,
            })
        },
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            to_binary(&query_config(deps)?)
        },
        QueryMsg::State {} => {
            to_binary(&query_state(deps)?)
        },
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config = read_config(&deps.storage)?;
    Ok(ConfigResponse {
        terranames_token: deps.api.human_address(&config.terranames_token)?,
        terraswap_pair: deps.api.human_address(&config.terraswap_pair)?,
        stable_denom: config.stable_denom,
        min_token_price: config.min_token_price,
    })
}

fn query_state<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<StateResponse> {
    let state = read_state(&deps.storage)?;
    Ok(StateResponse {
        initial_token_pool: state.initial_token_pool,
    })
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    Ok(MigrateResponse::default())
}
