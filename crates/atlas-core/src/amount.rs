use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawAmount {
    value: BigInt,
    decimals: u8,
}

impl RawAmount {
    pub fn new(value: BigInt, decimals: u8) -> Self {
        Self { value, decimals }
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
        Ok(Self::new(&self.value + &rhs.value, self.decimals))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AmountError {
    #[error("amount decimals mismatch: left={left} right={right}")]
    DecimalsMismatch { left: u8, right: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    #[test]
    fn raw_amount_preserves_large_integer() {
        let amount = RawAmount::new(BigInt::parse_bytes(b"1000000000000000000000000000000", 10).unwrap(), 18);
        assert_eq!(amount.decimals(), 18);
        assert_eq!(amount.value().to_string(), "1000000000000000000000000000000");
    }

    #[test]
    fn checked_add_rejects_decimal_mismatch() {
        let a = RawAmount::new(BigInt::from(1), 6);
        let b = RawAmount::new(BigInt::from(1), 18);
        assert_eq!(
            a.checked_add(&b).unwrap_err().to_string(),
            "amount decimals mismatch: left=6 right=18"
        );
    }
}
