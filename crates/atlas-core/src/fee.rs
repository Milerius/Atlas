//! Per-chain fee shapes.
//!
//! [`Fee`] is the chain-agnostic top-level enum used by higher product layers
//! that aggregate fees across networks. Each chain family's
//! [`crate::service::FeeEstimator::Fee`] associated type points at the
//! appropriate sibling enum (e.g. [`EvmFee`]).

use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

/// Chain-agnostic fee envelope returned by higher product layers that
/// aggregate fees across networks. Each chain family's
/// `crate::service::FeeEstimator::Fee` associated type is the appropriate
/// inner variant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fee {
    /// EVM fee (legacy gas price or EIP-1559 priority-fee model).
    Evm(EvmFee),
}

/// EVM-specific fee parameters. Returned by an EVM `FeeEstimator` impl.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EvmFee {
    /// Pre-EIP-1559 model: a single `gas_price` paid for every gas unit.
    Legacy {
        /// Wei per gas unit.
        gas_price: BigInt,
        /// Gas units the transaction is allowed to consume.
        gas_limit: u64,
    },
    /// EIP-1559 priority-fee model with optional OP-Stack L1 data fee.
    Eip1559 {
        /// Maximum total wei per gas unit (base fee + tip ceiling).
        max_fee_per_gas: BigInt,
        /// Tip in wei per gas unit paid to the validator.
        max_priority_fee_per_gas: BigInt,
        /// Gas units the transaction is allowed to consume on L2.
        gas_limit: u64,
        /// L1 data fee charged on OP-Stack L2s; `None` for plain L1s.
        l1_fee_wei: Option<BigInt>,
    },
}

impl EvmFee {
    /// Total worst-case fee in wei: `gas_limit * effective_price + l1_fee`.
    /// `l1_fee` is non-zero only on OP-Stack networks.
    pub fn max_fee_wei(&self) -> BigInt {
        match self {
            EvmFee::Legacy {
                gas_price,
                gas_limit,
            } => gas_price * BigInt::from(*gas_limit),
            EvmFee::Eip1559 {
                max_fee_per_gas,
                gas_limit,
                l1_fee_wei,
                ..
            } => {
                let l2 = max_fee_per_gas * BigInt::from(*gas_limit);
                match l1_fee_wei {
                    Some(l1) => l2 + l1,
                    None => l2,
                }
            }
        }
    }
}

/// Lifecycle of a broadcast transaction. Returned by
/// [`crate::service::ChainReader::get_transaction_status`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TransactionStatus {
    /// In the mempool but not yet included in a block.
    Pending { hash: String },
    /// Included in a block and executed successfully.
    Confirmed {
        hash: String,
        block_number: u64,
        gas_used: BigInt,
    },
    /// Included in a block but reverted.
    Failed { hash: String, reason: String },
    /// Hash not found in the mempool or any known block.
    NotFound { hash: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_max_fee_is_gas_price_times_gas_limit() {
        let fee = EvmFee::Legacy {
            gas_price: BigInt::from(20_000_000_000u64),
            gas_limit: 21_000,
        };
        assert_eq!(
            fee.max_fee_wei(),
            BigInt::from(20_000_000_000u64) * BigInt::from(21_000u64)
        );
    }

    #[test]
    fn eip1559_max_fee_no_l1() {
        let fee = EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 50_000,
            l1_fee_wei: None,
        };
        assert_eq!(
            fee.max_fee_wei(),
            BigInt::from(30_000_000_000u64) * BigInt::from(50_000u64)
        );
    }

    #[test]
    fn eip1559_max_fee_includes_l1() {
        let fee = EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 50_000,
            l1_fee_wei: Some(BigInt::from(123u64)),
        };
        let expected =
            BigInt::from(30_000_000_000u64) * BigInt::from(50_000u64) + BigInt::from(123u64);
        assert_eq!(fee.max_fee_wei(), expected);
    }

    #[test]
    fn fee_serde_round_trip_uses_snake_case_external_tag() {
        let fee = Fee::Evm(EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 21_000,
        });
        let json = serde_json::to_string(&fee).unwrap();
        // External tag, snake_case: {"evm": {...}}
        assert!(json.starts_with(r#"{"evm":"#), "got: {json}");
        let decoded: Fee = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, fee);
    }

    #[test]
    fn evm_fee_serde_round_trip_uses_internal_kind_tag() {
        let fee = EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(1u64),
            max_priority_fee_per_gas: BigInt::from(1u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        };
        let json = serde_json::to_string(&fee).unwrap();
        assert!(json.contains(r#""kind":"eip1559""#), "got: {json}");
        let decoded: EvmFee = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, fee);
    }

    #[test]
    fn transaction_status_serde_round_trip_uses_internal_kind_tag() {
        let status = TransactionStatus::Confirmed {
            hash: "0xabc".to_string(),
            block_number: 42,
            gas_used: BigInt::from(21_000u64),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains(r#""kind":"confirmed""#), "got: {json}");
        let decoded: TransactionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, status);
    }
}
