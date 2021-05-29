use cosmwasm_std::{
    attr, entry_point, to_binary, Deps, DepsMut, Env, MessageInfo,
    QueryResponse, Response, StdResult,
};

use terranames::querier::query_name_state;
use terranames::resolver::{
    ConfigResponse, InstantiateMsg, ExecuteMsg, MigrateMsg, QueryMsg,
    ResolveNameResponse,
};

use crate::errors::{ContractError, NameExpired, Unauthorized};
use crate::state::{
    read_config, read_name_value, store_config, store_name_value, Config,
};

type ContractResult<T> = Result<T, ContractError>;

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> ContractResult<Response> {
    let auction_contract = deps.api.addr_validate(&msg.auction_contract)?;

    let state = Config {
        auction_contract,
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
        ExecuteMsg::SetNameValue { name, value } => {
            execute_set_value(deps, env, info, name, value)
        },
    }
}

fn execute_set_value(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    name: String,
    value: Option<String>,
) -> ContractResult<Response> {
    let config = read_config(deps.storage)?;
    let name_state = query_name_state(
        &deps.querier,
        &config.auction_contract,
        &name,
    )?;

    // ensure name controller permission
    if let Some(controller) = name_state.controller {
        if info.sender != controller {
            return Unauthorized.fail();
        }
    } else {
        return Unauthorized.fail();
    }

    if let Some(expire_block) = name_state.expire_block {
        if env.block.height >= expire_block {
            return NameExpired.fail();
        }
    }

    store_name_value(deps.storage, &name, value.clone())?;

    Ok(Response {
        messages: vec![],
        attributes: vec![
            attr("action", "set_value"),
            if let Some(ref value) = value {
                attr("value", value)
            } else {
                attr("value_deleted", "")
            },
        ],
        data: None,
        submessages: vec![],
    })
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
        QueryMsg::ResolveName { name } => {
            Ok(to_binary(&query_resolve(deps, env, name)?)?)
        },
    }
}

fn query_config(
    deps: Deps,
    _env: Env,
) -> ContractResult<ConfigResponse> {
    let config = read_config(deps.storage)?;

    Ok(ConfigResponse {
        auction_contract: config.auction_contract.into(),
    })
}

fn query_resolve(
    deps: Deps,
    _env: Env,
    name: String,
) -> ContractResult<ResolveNameResponse> {
    let config = read_config(deps.storage)?;
    let name_state = query_name_state(
        &deps.querier,
        &config.auction_contract,
        &name,
    )?;
    let name_value = read_name_value(deps.storage, &name)?;

    Ok(ResolveNameResponse {
        value: name_value,
        expire_block: name_state.expire_block,
    })
}

#[entry_point]
pub fn migrate(
    _deps: DepsMut,
    _env: Env,
    _msg: MigrateMsg,
) -> StdResult<Response> {
    Ok(Response::default())
}
