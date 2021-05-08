use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, HandleResult,
    InitResponse, InitResult, Querier, StdError, StdResult, Storage,
};

use terranames::querier::query_name_state;
use terranames::resolver::{
    ConfigResponse, InitMsg, HandleMsg, QueryMsg, ResolveNameResponse,
};

use crate::state::{
    read_config, read_name_value, store_config, store_name_value, Config,
};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> InitResult {
    let auction_contract = deps.api.canonical_address(&msg.auction_contract)?;

    let state = Config {
        auction_contract,
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
        HandleMsg::SetNameValue { name, value } => {
            handle_set_value(deps, env, name, value)
        },
    }
}

fn handle_set_value<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    name: String,
    value: Option<String>,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let name_state = query_name_state(
        &deps.querier,
        &deps.api.human_address(&config.auction_contract)?,
        &name,
    )?;

    // ensure name controller permission
    if let Some(controller) = name_state.controller {
        if env.message.sender != controller {
            return Err(StdError::unauthorized());
        }
    } else {
        return Err(StdError::unauthorized());
    }

    if let Some(expire_block) = name_state.expire_block {
        if env.block.height >= expire_block {
            return Err(StdError::generic_err("Name is expired"));
        }
    }

    store_name_value(&mut deps.storage, &name, value.clone())?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "set_value"),
            if let Some(ref value) = value {
                log("value", value)
            } else {
                log("value_deleted", "")
            },
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
        QueryMsg::ResolveName { name } => {
            to_binary(&query_resolve(deps, name)?)
        },
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config = read_config(&deps.storage)?;

    Ok(ConfigResponse {
        auction_contract: deps.api.human_address(&config.auction_contract)?,
    })
}

fn query_resolve<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    name: String,
) -> StdResult<ResolveNameResponse> {
    let config = read_config(&deps.storage)?;
    let name_state = query_name_state(
        &deps.querier,
        &deps.api.human_address(&config.auction_contract)?,
        &name,
    )?;
    let name_value = read_name_value(&deps.storage, &name)?;

    Ok(ResolveNameResponse {
        value: name_value,
        expire_block: name_state.expire_block,
    })
}
