//! Tests for `EvmFeeEstimator` against a mocked alloy provider.

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

const MIN_PRIORITY_FEE_WEI: u128 = 1_000_000;
const MAX_PRIORITY_FEE_WEI: u128 = 200_000_000;
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
async fn eip1559_fee_uses_median_priority_clamped_to_min() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    // Median of [1, 2, 3] = 2 wei, far below MIN_PRIORITY_FEE_WEI (1_000_000).
    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![100_000_000_000u128, 100_000_000_000u128],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![1u128], vec![2u128], vec![3u128]]),
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
            assert_eq!(max_priority_fee_per_gas, BigInt::from(MIN_PRIORITY_FEE_WEI));
        }
        other => panic!("expected EIP-1559, got {other:?}"),
    }
}

#[tokio::test]
async fn eip1559_fee_uses_median_priority_clamped_to_max() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    // Median (10 GWEI) is far above MAX (0.2 GWEI).
    let huge: u128 = 10_000_000_000;
    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![100u128, 100u128],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![huge], vec![huge], vec![huge]]),
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
            assert_eq!(max_priority_fee_per_gas, BigInt::from(MAX_PRIORITY_FEE_WEI));
        }
        other => panic!("expected EIP-1559, got {other:?}"),
    }
}

#[tokio::test]
async fn eip1559_fee_max_fee_is_base_times_multiplier_plus_priority() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    let base_fee: u128 = 50_000_000_000;
    // Pick a priority value well within [MIN, MAX] so it isn't clamped.
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
            // base_fee_per_gas.last() is the next-block base. multiplier=2.
            let expected = BigInt::from(base_fee * 2 + priority);
            assert_eq!(max_fee_per_gas, expected);
            assert_eq!(max_priority_fee_per_gas, BigInt::from(priority));
            assert_eq!(gas_limit, 21_000);
            assert_eq!(l1_fee_wei, None);
        }
        other => panic!("expected EIP-1559, got {other:?}"),
    }
}

#[tokio::test]
async fn eip1559_fee_falls_back_to_legacy_when_fee_history_empty() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    // Empty base_fee_per_gas triggers the FeeEstimationFailed error in
    // eip1559_fee, which then falls back to legacy_fee.
    asserter.push_success(&FeeHistory::default());
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
