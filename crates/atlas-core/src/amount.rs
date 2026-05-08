//! Big-integer raw amounts with a decimal scale.
//!
//! Atlas never uses `f64` / `f32` for money, balances, fees, or quantities.
//! [`RawAmount`] wraps `num_bigint::BigInt` (arbitrary precision) plus a
//! `u8` decimal scale, which is enough to represent any chain-native base
//! unit — wei (18), satoshi (8), token units, lamports (9), etc. — without
//! precision loss.
//!
//! Display strings (e.g. "1.5 USDC") are derived values produced by higher
//! layers from the raw value + decimals; the raw amount is the
//! authoritative form.

use num_bigint::{BigInt, Sign};
use serde::{Deserialize, Serialize};

/// A non-negative on-chain amount in raw base units, paired with the
/// instrument's decimal scale.
///
/// Construct with [`RawAmount::new`]; negative values are rejected.
/// Arithmetic uses [`RawAmount::checked_add`] which requires matching
/// decimals — different-scale amounts are different units and cannot be
/// summed without explicit conversion at a higher layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawAmount {
    value: BigInt,
    decimals: u8,
}

impl RawAmount {
    /// Construct a new raw amount.
    ///
    /// Returns [`AmountError::NegativeValue`] if `value` is negative.
    /// Zero and any positive `BigInt` are accepted.
    pub fn new(value: BigInt, decimals: u8) -> Result<Self, AmountError> {
        if value.sign() == Sign::Minus {
            return Err(AmountError::NegativeValue);
        }
        Ok(Self { value, decimals })
    }

    /// Borrow the raw integer value (e.g. wei, lamports).
    pub fn value(&self) -> &BigInt {
        &self.value
    }

    /// The instrument's decimal scale (e.g. 18 for ETH, 6 for USDC).
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// Add two amounts that share the same decimal scale.
    ///
    /// Returns [`AmountError::DecimalsMismatch`] when scales differ; this
    /// is a hard error rather than a coercion because mixing scales
    /// silently is how you lose user funds.
    pub fn checked_add(&self, rhs: &Self) -> Result<Self, AmountError> {
        if self.decimals != rhs.decimals {
            return Err(AmountError::DecimalsMismatch {
                left: self.decimals,
                right: rhs.decimals,
            });
        }
        // Sum of two non-negative values is non-negative; bypass the
        // sign check to avoid the unreachable error path.
        Ok(Self {
            value: &self.value + &rhs.value,
            decimals: self.decimals,
        })
    }
}

/// Errors returned by [`RawAmount`] construction and arithmetic.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AmountError {
    /// `checked_add` was called with two amounts whose decimal scales
    /// differ. The two scales are reported in the variant for diagnosis.
    #[error("amount decimals mismatch: left={left} right={right}")]
    DecimalsMismatch {
        /// Decimal scale of the left-hand operand.
        left: u8,
        /// Decimal scale of the right-hand operand.
        right: u8,
    },
    /// `RawAmount::new` was called with a negative `BigInt`. Atlas
    /// amounts are non-negative by construction.
    #[error("amount value must be non-negative")]
    NegativeValue,
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn raw_amount_preserves_large_integer() {
        let amount = RawAmount::new(
            BigInt::parse_bytes(b"1000000000000000000000000000000", 10).unwrap(),
            18,
        )
        .unwrap();
        assert_eq!(amount.decimals(), 18);
        assert_eq!(
            amount.value().to_string(),
            "1000000000000000000000000000000"
        );
    }

    #[test]
    fn checked_add_rejects_decimal_mismatch() {
        let a = RawAmount::new(BigInt::from(1), 6).unwrap();
        let b = RawAmount::new(BigInt::from(1), 18).unwrap();
        assert_eq!(
            a.checked_add(&b).unwrap_err().to_string(),
            "amount decimals mismatch: left=6 right=18"
        );
    }

    #[test]
    fn raw_amount_rejects_negative_value() {
        assert_eq!(
            RawAmount::new(BigInt::from(-1), 18).unwrap_err(),
            AmountError::NegativeValue,
        );
    }

    #[test]
    fn checked_add_sums_matching_decimals() {
        let a = RawAmount::new(BigInt::from(2u64), 6).unwrap();
        let b = RawAmount::new(BigInt::from(3u64), 6).unwrap();
        let sum = a.checked_add(&b).unwrap();
        assert_eq!(sum.value().to_string(), "5");
        assert_eq!(sum.decimals(), 6);
    }

    #[test]
    fn raw_amount_accepts_zero() {
        let amount = RawAmount::new(BigInt::from(0u64), 18).unwrap();
        assert_eq!(amount.value().to_string(), "0");
    }
}
