use cosmwasm_std::Decimal;

/// Helper trait to expose denominator constant
pub trait FractionDenom {
    const FRACTIONAL: u128;
}

impl FractionDenom for Decimal {
    const FRACTIONAL: u128 = 1_000_000_000_000_000_000;
}
