use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Coin, CosmosMsg, Decimal,
    Deps, DepsMut, Env, Fraction, MessageInfo, QuerierWrapper, QueryResponse,
    Response, StdResult, Uint128, WasmMsg,
};

use cw20::{
    BalanceResponse as Cw20BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg,
    Cw20ReceiveMsg,
};
use terranames::root_collector::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, ReceiveMsg,
    StateResponse, QueryMsg,
};
use terranames::terra::{calculate_added_tax, calculate_tax, deduct_tax};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    ExecuteMsg as PairHandleMsg, QueryMsg as PairQueryMsg,
    PoolResponse,
};
use terraswap::querier::query_pair_info;

use crate::errors::{ContractError, InvalidConfig, Unauthorized, Unfunded};
use crate::state::{
    read_config, read_state, store_config, store_state, Config, State,
};

type ContractResult<T> = Result<T, ContractError>;

// TODO cw20 package balance query helper seems to be broken in cosmwasm 0.14?
fn query_token_balance(
    querier: &QuerierWrapper,
    token_contract: &Addr,
    address: &Addr,
) -> StdResult<Uint128> {
    let query_response: Cw20BalanceResponse = querier.query_wasm_smart(
        token_contract,
        &Cw20QueryMsg::Balance {
            address: address.into(),
        },
    )?;
    Ok(query_response.balance)
}

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> ContractResult<Response> {
    if msg.min_token_price.is_zero() {
        return InvalidConfig.fail();
    }

    let terranames_token = deps.api.addr_validate(&msg.terranames_token)?;
    let terraswap_factory = deps.api.addr_validate(&msg.terraswap_factory)?;

    // Query for the swap contract
    let pair = query_pair_info(
        &deps.querier,
        terraswap_factory,
        &[
            AssetInfo::Token {
                contract_addr: terranames_token.clone(),
            },
            AssetInfo::NativeToken {
                denom: msg.stable_denom.clone(),
            },
        ]
    )?;

    let config = Config {
        terranames_token: terranames_token,
        terraswap_pair: pair.contract_addr,
        stable_denom: msg.stable_denom,
        min_token_price: msg.min_token_price,
    };

    store_config(deps.storage, &config)?;

    let state = State {
        initial_token_pool: Uint128::zero(),
    };

    store_state(deps.storage, &state)?;

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
        ExecuteMsg::ConsumeExcessStable {} => {
            execute_consume_excess_stable(deps, env, info)
        },
        ExecuteMsg::ConsumeExcessTokens {} => {
            execute_consume_excess_tokens(deps, env, info)
        },
        ExecuteMsg::Receive(msg) => {
            execute_receive(deps, env, info, msg)
        },
    }
}

fn execute_consume_excess_stable(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    // Query for current stable coin balance
    let stable_balance = deps.querier.query_balance(
        env.contract.address.clone(),
        &config.stable_denom,
    )?.amount;
    if stable_balance.is_zero() {
        return Unfunded.fail();
    }

    // Query for the current token balance
    let token_balance = query_token_balance(
        &deps.querier,
        &config.terranames_token,
        &env.contract.address,
    )?;

    let mut messages = vec![];

    let mut stable_to_send = stable_balance;
    let tokens_to_release_left = std::cmp::min(token_balance, state.initial_token_pool);
    if !tokens_to_release_left.is_zero() {
        // Query for the swap pool exchange rate
        let pair_pool: PoolResponse = deps.querier.query_wasm_smart(
            &config.terraswap_pair,
            &PairQueryMsg::Pool {},
        )?;

        let (pool_tokens, pool_stables) = match (&pair_pool.assets[0].info, &pair_pool.assets[1].info) {
            (&AssetInfo::NativeToken { .. }, &AssetInfo::Token { .. }) => {
                (pair_pool.assets[1].amount, pair_pool.assets[0].amount)
            },
            (&AssetInfo::Token { .. }, &AssetInfo::NativeToken { .. }) => {
                (pair_pool.assets[0].amount, pair_pool.assets[1].amount)
            },
            _ => {
                panic!("Unexpected pool data");
            },
        };

        let max_provide_stable_tax = calculate_tax(&deps.querier, &config.stable_denom, stable_to_send)?;
        let max_provide_stable = stable_to_send.checked_sub(max_provide_stable_tax)?;
        let max_tokens_per_stable = config.min_token_price.inv().expect("Invalid token price");

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
            (tokens_to_release_left, reduced_provide_stable, calculate_added_tax(&deps.querier, &config.stable_denom, reduced_provide_stable)?)
        } else {
            (max_provide_tokens, max_provide_stable, max_provide_stable_tax)
        };

        // Allow swap pair to withdraw the tokens
        messages.push(
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: config.terranames_token.to_string(),
                    msg: to_binary(
                        &Cw20ExecuteMsg::IncreaseAllowance {
                            spender: config.terraswap_pair.to_string(),
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
                    contract_addr: config.terraswap_pair.to_string(),
                    msg: to_binary(
                        &PairHandleMsg::ProvideLiquidity {
                            assets: [
                                Asset {
                                    info: AssetInfo::Token {
                                        contract_addr: config.terranames_token.into(),
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
        store_state(deps.storage, &state)?;
    }

    // Use remaining funds to buy back tokens
    if !stable_to_send.is_zero() {
        let remaining_after_tax = deduct_tax(&deps.querier, &config.stable_denom, stable_to_send)?;
        messages.push(
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: config.terraswap_pair.into(),
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

    Ok(Response {
        messages,
        attributes: vec![
            attr("action", "consume_excess_stable"),
            attr("initial_token_pool", state.initial_token_pool),
        ],
        data: None,
        submessages: vec![],
    })
}

fn execute_consume_excess_tokens(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let state = read_state(deps.storage)?;

    // Query for the current token balance
    let token_balance = query_token_balance(
        &deps.querier,
        &config.terranames_token,
        &env.contract.address,
    )?;

    let tokens_to_burn = Uint128::from(
        token_balance.u128().saturating_sub(state.initial_token_pool.u128())
    );
    if tokens_to_burn.is_zero() {
        return Unfunded.fail();
    }

    Ok(Response {
        messages: vec![
            CosmosMsg::Wasm(
                WasmMsg::Execute {
                    contract_addr: config.terranames_token.into(),
                    msg: to_binary(
                        &Cw20ExecuteMsg::Burn {
                            amount: token_balance,
                        },
                    )?,
                    send: vec![],
                },
            )
        ],
        attributes: vec![
            attr("action", "consume_excess_tokens"),
            attr("tokens", token_balance),
        ],
        data: None,
        submessages: vec![],
    })
}

fn execute_receive(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> ContractResult<Response> {
    let msg: ReceiveMsg = from_binary(&wrapper.msg)?;

    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    if info.sender != config.terranames_token {
        return Unauthorized.fail();
    }

    match msg {
        ReceiveMsg::AcceptInitialTokens {} => {
            state.initial_token_pool += wrapper.amount;
            store_state(deps.storage, &state)?;

            Ok(Response {
                messages: vec![],
                attributes: vec![
                    attr("action", "receive"),
                    attr("receive_type", "accept_initial_tokens"),
                    attr("initial_token_pool", state.initial_token_pool),
                ],
                data: None,
                submessages: vec![],
            })
        },
    }
}

#[entry_point]
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
    }
}

fn query_config(
    deps: Deps,
    _env: Env,
) -> ContractResult<ConfigResponse> {
    let config = read_config(deps.storage)?;
    Ok(ConfigResponse {
        terranames_token: config.terranames_token.into(),
        terraswap_pair: config.terraswap_pair.into(),
        stable_denom: config.stable_denom,
        min_token_price: config.min_token_price,
    })
}

fn query_state(
    deps: Deps,
    _env: Env,
) -> ContractResult<StateResponse> {
    let state = read_state(deps.storage)?;
    Ok(StateResponse {
        initial_token_pool: state.initial_token_pool,
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
