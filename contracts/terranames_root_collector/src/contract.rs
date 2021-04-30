use cosmwasm_std::{
    log, to_binary, Api, Binary, Coin, CosmosMsg, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, InitResponse, InitResult, Querier, QueryRequest,
    StdError, StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use cw20::{Cw20Contract, Cw20HandleMsg};
use terranames::collector::{
    AcceptFunds,
    RootCollectorHandleMsg as HandleMsg, RootCollectorConfigResponse as ConfigResponse,
    RootCollectorInitMsg as InitMsg, RootCollectorQueryMsg as QueryMsg,
};
use terranames::terra::{calculate_added_tax, calculate_tax, deduct_tax};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    HandleMsg as PairHandleMsg, QueryMsg as PairQueryMsg,
    PoolResponse,
};
use terraswap::querier::query_pair_info;

use crate::state::{
    read_config, store_config, Config,
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

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> InitResult {
    if msg.init_token_price.is_zero() {
        return Err(StdError::generic_err("init_token_price must be non-zero"));
    }

    let terranames_token = deps.api.canonical_address(&msg.terranames_token)?;
    let terraswap_registry = deps.api.canonical_address(&msg.terraswap_registry)?;

    // Query for the swap contract
    let pair = query_pair_info(
        deps, &deps.api.human_address(&terraswap_registry)?, &[
            AssetInfo::Token {
                contract_addr: deps.api.human_address(&terranames_token)?,
            },
            AssetInfo::NativeToken {
                denom: msg.stable_denom.clone(),
            },
        ]
    )?;

    let state = Config {
        terranames_token: deps.api.canonical_address(&msg.terranames_token)?,
        terraswap_pair: deps.api.canonical_address(&pair.contract_addr)?,
        stable_denom: msg.stable_denom,
        init_token_price: msg.init_token_price,
        initial_tokens_left: msg.initial_tokens_left,
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
        HandleMsg::AcceptFunds(AcceptFunds { denom, source_addr }) => {
            handle_accept_funds(deps, env, denom, source_addr)
        },
        HandleMsg::BurnExcess {} => {
            handle_burn_excess(deps, env)
        },
    }
}

fn handle_accept_funds<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    denom: String,
    source_addr: HumanAddr,
) -> HandleResult {
    let mut config = read_config(&deps.storage)?;

    if denom != config.stable_denom {
        return Err(StdError::generic_err(
            format!(
                "Funds do not match fund denomination: {}",
                config.stable_denom,
            )
        ));
    }

    let msg_deposit = get_sent_funds(&env, &denom);
    if msg_deposit.is_zero() {
        return Err(StdError::generic_err(format!("Missing funds: {}", denom)));
    }

    let mut messages = vec![];

    let mut stable_to_send = msg_deposit;

    if !config.initial_tokens_left.is_zero() {
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

        let max_provide_stable_tax = calculate_tax(deps, &denom, stable_to_send)?;
        let max_provide_stable = (stable_to_send - max_provide_stable_tax)?;
        let max_provide_tokens = if !pool_stables.is_zero() {
            max_provide_stable.multiply_ratio(pool_tokens.u128(), pool_stables.u128())
        } else {
            Uint128::from(1u64).multiply_ratio(max_provide_stable, config.init_token_price)
        };

        // Reduce stablecoins provided if max threshold is hit
        let (provide_tokens, provide_stable, provide_stable_tax) = if max_provide_tokens > config.initial_tokens_left {
            let reduced_provide_stable = if !pool_tokens.is_zero() {
                config.initial_tokens_left.multiply_ratio(pool_stables.u128(), pool_tokens.u128())
            } else {
                config.initial_tokens_left.multiply_ratio(config.init_token_price, 1u64)
            };
            (config.initial_tokens_left, reduced_provide_stable, calculate_added_tax(deps, &denom, reduced_provide_stable)?)
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
                                        denom: denom.clone(),
                                    },
                                    amount: provide_stable,
                                },
                            ],
                            slippage_tolerance: None,
                        },
                    )?,
                    send: vec![
                        Coin {
                            denom: denom.clone(),
                            amount: provide_stable,
                        },
                    ],
                },
            ),
        );

        config.initial_tokens_left = (config.initial_tokens_left - provide_tokens)?;
        stable_to_send = ((stable_to_send - provide_stable)? - provide_stable_tax)?;

        store_config(&mut deps.storage, &config)?;
    }

    // Use remaining funds to buy back tokens
    if !stable_to_send.is_zero() {
        let remaining_after_tax = deduct_tax(deps, &denom, stable_to_send)?;
        messages.push(
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: deps.api.human_address(&config.terraswap_pair)?,
                    msg: to_binary(
                        &PairHandleMsg::Swap {
                            offer_asset: Asset {
                                info: AssetInfo::NativeToken {
                                    denom: denom.clone(),
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
                            denom: denom.clone(),
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
            log("action", "accept_funds"),
        ],
        data: None,
    })
}

fn handle_burn_excess<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> HandleResult {
    let config = read_config(&deps.storage)?;

    // Query for the current token balance
    let token_balance = Cw20Contract(
        deps.api.human_address(&config.terranames_token)?,
    ).balance(
        &deps.querier,
        env.contract.address,
    )?;

    let tokens_to_burn = (token_balance - config.initial_tokens_left)?;
    if tokens_to_burn.is_zero() {
        return Err(StdError::generic_err("No tokens to burn"));
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
            log("action", "burn_excess"),
            log("tokens", token_balance),
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
    }
}

pub fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config = read_config(&deps.storage)?;
    Ok(ConfigResponse {
        terranames_token: deps.api.human_address(&config.terranames_token)?,
        terraswap_pair: deps.api.human_address(&config.terraswap_pair)?,
        stable_denom: config.stable_denom,
        init_token_price: config.init_token_price,
        initial_tokens_left: config.initial_tokens_left,
    })
}
