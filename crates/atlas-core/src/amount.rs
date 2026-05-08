use num_bigint::{BigInt, Sign};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawAmount {
    value: BigInt,
    decimals: u8,
}

impl RawAmount {
    pub fn new(value: BigInt, decimals: u8) -> Result<Self, AmountError> {
        if value.sign() == Sign::Minus {
            return Err(AmountError::NegativeValue);
        }
        Ok(Self { value, decimals })
    }

    pub fn value(&self) -> &BigInt {
        &self.value
    }

    pub fn decimals(&self) -> u8 {
        self.decimals
    }

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

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AmountError {
    #[error("amount decimals mismatch: left={left} right={right}")]
    DecimalsMismatch { left: u8, right: u8 },
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
}
