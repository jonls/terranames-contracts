use cosmwasm_std::{Coin, QuerierWrapper, StdResult, Uint128};
use terra_cosmwasm::TerraQuerier;

const DECIMAL_FRACTION: Uint128 = Uint128::new(1_000_000_000_000_000_000u128);

/// Calculate tax that is subtracted from the sent amount
///
/// Source: terraswap
pub fn calculate_tax(
    querier: &QuerierWrapper,
    denom: &str,
    amount: Uint128,
) -> StdResult<Uint128> {
    let terra_querier = TerraQuerier::new(querier);
    let tax_rate = terra_querier.query_tax_rate()?.rate;
    let tax_cap = terra_querier.query_tax_cap(denom)?.cap;
    Ok(std::cmp::min(
        // a * (1 - (1 / (t + 1)))
        amount.checked_sub(
            amount.multiply_ratio(
                DECIMAL_FRACTION,
                DECIMAL_FRACTION * tax_rate + DECIMAL_FRACTION,
            ),
        )?,
        tax_cap,
    ))
}

/// Calculate tax to be sent in addition in order for recipient to receive amount
///
/// Source: terraswap
pub fn calculate_added_tax(
    querier: &QuerierWrapper,
    denom: &str,
    amount: Uint128,
) -> StdResult<Uint128> {
    let terra_querier = TerraQuerier::new(querier);
    let tax_rate = terra_querier.query_tax_rate()?.rate;
    let tax_cap = terra_querier.query_tax_cap(denom)?.cap;
    Ok(std::cmp::min(amount * tax_rate, tax_cap))
}

/// Return Coin after deducting tax.
///
/// This is useful when sending a fixed amount to figure out how much to put in
/// the send message for the amount plus taxes to sum to the fixed amount.
/// Source: terraswap
pub fn deduct_coin_tax(
    querier: &QuerierWrapper,
    coin: Coin,
) -> StdResult<Coin> {
    if coin.denom == "uluna" {
        Ok(coin)
    } else {
        let amount = deduct_tax(querier, &coin.denom, coin.amount)?;
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
pub fn deduct_tax(
    querier: &QuerierWrapper,
    denom: &str,
    amount: Uint128,
) -> StdResult<Uint128> {
    let tax = calculate_tax(querier, denom, amount)?;
    Ok(amount.checked_sub(tax)?)
}
