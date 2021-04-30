use cosmwasm_std::{
    Api, Coin, Extern, Querier, StdResult, Storage, Uint128
};
use terra_cosmwasm::TerraQuerier;

const DECIMAL_FRACTION: Uint128 = Uint128(1_000_000_000_000_000_000u128);

/// Calculate tax that is subtracted from the sent amount
///
/// Source: terraswap
pub fn calculate_tax<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    denom: &str,
    amount: Uint128,
) -> StdResult<Uint128> {
    let terra_querier = TerraQuerier::new(&deps.querier);
    let tax_rate = terra_querier.query_tax_rate()?.rate;
    let tax_cap = terra_querier.query_tax_cap(denom)?.cap;
    Ok(std::cmp::min(
        (
            amount -
            amount.multiply_ratio(
                DECIMAL_FRACTION,
                DECIMAL_FRACTION * tax_rate + DECIMAL_FRACTION,
            )
        )?,
        tax_cap,
    ))
}

/// Calculate tax to be sent in addition in order for recipient to receive amount
///
/// Source: terraswap
pub fn calculate_added_tax<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    denom: &str,
    amount: Uint128,
) -> StdResult<Uint128> {
    let terra_querier = TerraQuerier::new(&deps.querier);
    let tax_rate = terra_querier.query_tax_rate()?.rate;
    let tax_cap = terra_querier.query_tax_cap(denom)?.cap;
    Ok(std::cmp::min(
        (
            amount -
            amount.multiply_ratio(
                DECIMAL_FRACTION,
                DECIMAL_FRACTION * tax_rate + DECIMAL_FRACTION,
            )
        )?,
        tax_cap,
    ))
}


/// Return Coin after deducting tax.
///
/// This is useful when sending a fixed amount to figure out how much to put in
/// the send message for the amount plus taxes to sum to the fixed amount.
/// Source: terraswap
pub fn deduct_coin_tax<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    coin: Coin,
) -> StdResult<Coin> {
    if coin.denom == "uluna" {
        Ok(coin)
    } else {
        let amount = deduct_tax(deps, &coin.denom, coin.amount)?;
        Ok(Coin {
            denom: coin.denom,
            amount,
        })
    }
}

/// Return amount after deducting tax.
///
/// This is useful when sending a fixed amount to figure out how much to put in
/// the send message for the amount plus taxes to sum to the fixed amount.
/// Source: terraswap
pub fn deduct_tax<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    denom: &str,
    amount: Uint128,
) -> StdResult<Uint128> {
    amount - calculate_tax(deps, denom, amount)?
}
