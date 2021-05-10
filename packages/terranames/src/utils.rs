use cosmwasm_std::{Decimal, Uint128};

/// Helper trait to expose inverse and denominator constant
pub trait FractionInv {
    const FRACTIONAL: u128;

    fn inv(&self) -> Option<Self> where Self: Sized;
}

impl FractionInv for Decimal {
    const FRACTIONAL: u128 = 1_000_000_000_000_000_000;

    fn inv(&self) -> Option<Self> {
        if self.is_zero() {
            None
        } else {
            Some(
                Decimal::from_ratio(
                    Decimal::FRACTIONAL,
                    Uint128::from(Decimal::FRACTIONAL) * *self,
                )
            )
        }
    }
}
