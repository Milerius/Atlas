//! Tests for `EvmFeeEstimator` against a mocked alloy provider.
//!
//! Since the EIP-1559 path now delegates to
//! [`alloy_provider::Provider::estimate_eip1559_fees`] (modelled after
//! MetaMask's gas-fee controller), the tests assert the alloy contract
//! end-to-end rather than re-checking median/clamp arithmetic that lives
//! upstream.

use alloy_provider::mock::Asserter;
use alloy_provider::ProviderBuilder;
use alloy_rpc_types_eth::FeeHistory;
use atlas_core::amount::RawAmount;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AddressRef, AssetInstanceId};
use atlas_core::service::FeeEstimator;
use atlas_core::transaction::TransferIntent;
use atlas_evm::fee_estimator::EvmFeeEstimator;
use num_bigint::BigInt;
use std::str::FromStr;

const SENDER: &str = "0x39fa8c5f2793459d6622857e7d9fbb4bd91766d3";

fn native_intent() -> TransferIntent {
    TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
    }
}

fn erc20_intent() -> TransferIntent {
    TransferIntent {
        asset_instance_id: AssetInstanceId::from_str(
            "eip155:1/erc20:0xc083e9947cf02b8ffc7d3090ae9aea72df98fd47",
        )
        .unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 6).unwrap(),
    }
}

#[tokio::test]
async fn eip1559_fee_returns_alloy_default_for_typical_inputs() {
    // alloy's default estimator: max_priority = median(non_zero_rewards),
    // bumped up to a 1-wei floor; max_fee = base_fee * 2 + max_priority.
    // Median of [5, 5, 5, 5, 5] = 5; with base_fee = 1 gwei, the result is
    // max_priority = 5_000_000 wei, max_fee = 2_000_000_005 wei.
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    let base_fee: u128 = 1_000_000_000;
    let priority: u128 = 5_000_000;
    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![base_fee, base_fee],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![priority]; 5]),
    });

    let estimator = EvmFeeEstimator::new(provider, true);
    let fee = estimator
        .estimate_fee(&native_intent(), &AddressRef::from_str(SENDER).unwrap())
        .await
        .unwrap();
    match fee {
        EvmFee::Eip1559 {
            max_fee_per_gas,
            max_priority_fee_per_gas,
            gas_limit,
            l1_fee_wei,
        } => {
            assert_eq!(max_priority_fee_per_gas, BigInt::from(priority));
            // alloy: max_fee = base * 2 + priority
            assert_eq!(max_fee_per_gas, BigInt::from(base_fee * 2 + priority));
            assert_eq!(gas_limit, 21_000);
            assert_eq!(l1_fee_wei, None);
        }
        other => panic!("expected EIP-1559, got {other:?}"),
    }
}

#[tokio::test]
async fn eip1559_fee_uses_alloy_min_priority_when_rewards_are_zero() {
    // When `eth_feeHistory` returns all-zero priority-fee samples, alloy
    // falls back to a 1-wei minimum (its EIP1559_MIN_PRIORITY_FEE
    // constant). This pins that contract — Atlas no longer enforces its
    // own opinionated 0.001-gwei floor.
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![100_000_000_000u128, 100_000_000_000u128],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![0u128]; 3]),
    });

    let estimator = EvmFeeEstimator::new(provider, true);
    let fee = estimator
        .estimate_fee(&native_intent(), &AddressRef::from_str(SENDER).unwrap())
        .await
        .unwrap();
    match fee {
        EvmFee::Eip1559 {
            max_priority_fee_per_gas,
            ..
        } => {
            assert_eq!(max_priority_fee_per_gas, BigInt::from(1u64));
        }
        other => panic!("expected EIP-1559, got {other:?}"),
    }
}

#[tokio::test]
async fn eip1559_fee_falls_back_to_legacy_when_fee_history_returns_no_base_fee() {
    // alloy's `estimate_eip1559_fees` falls back to `eth_getBlockByNumber`
    // when feeHistory produces no usable base fee. We make both fail so
    // the outer match in our estimator drops to legacy_fee, which calls
    // `eth_gasPrice`.
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    asserter.push_success(&FeeHistory::default());
    // alloy's fallback fetches a block; making it fail flushes us into
    // the outer Err arm.
    asserter.push_failure_msg("no block available");
    // legacy_fee call.
    asserter.push_success(&alloy_primitives::U128::from(7_000_000_000u128));

    let estimator = EvmFeeEstimator::new(provider, true);
    let fee = estimator
        .estimate_fee(&native_intent(), &AddressRef::from_str(SENDER).unwrap())
        .await
        .unwrap();
    match fee {
        EvmFee::Legacy { gas_price, .. } => {
            assert_eq!(gas_price, BigInt::from(7_000_000_000u128));
        }
        other => panic!("expected Legacy fallback, got {other:?}"),
    }
}

#[tokio::test]
async fn legacy_fee_returns_provider_gas_price() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    asserter.push_success(&alloy_primitives::U128::from(123_456_789u128));

    let estimator = EvmFeeEstimator::new(provider, false);
    let fee = estimator
        .estimate_fee(&native_intent(), &AddressRef::from_str(SENDER).unwrap())
        .await
        .unwrap();
    match fee {
        EvmFee::Legacy {
            gas_price,
            gas_limit,
        } => {
            assert_eq!(gas_price, BigInt::from(123_456_789u128));
            assert_eq!(gas_limit, 21_000);
        }
        other => panic!("expected Legacy, got {other:?}"),
    }
}

#[tokio::test]
async fn gas_limit_is_higher_for_erc20_intent_than_native() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    // Two legacy estimates back-to-back (eip1559 = false).
    asserter.push_success(&alloy_primitives::U128::from(1u128));
    asserter.push_success(&alloy_primitives::U128::from(1u128));

    let estimator = EvmFeeEstimator::new(provider, false);
    let native = estimator
        .estimate_fee(&native_intent(), &AddressRef::from_str(SENDER).unwrap())
        .await
        .unwrap();
    let erc20 = estimator
        .estimate_fee(&erc20_intent(), &AddressRef::from_str(SENDER).unwrap())
        .await
        .unwrap();

    let n_gas = match native {
        EvmFee::Legacy { gas_limit, .. } => gas_limit,
        _ => unreachable!(),
    };
    let e_gas = match erc20 {
        EvmFee::Legacy { gas_limit, .. } => gas_limit,
        _ => unreachable!(),
    };
    assert!(e_gas > n_gas, "ERC-20 gas limit should exceed native");
}
