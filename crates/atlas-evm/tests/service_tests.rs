//! Tests for `EvmChainService::transfer` orchestration via a mocked provider.

use alloy_primitives::B256;
use alloy_provider::mock::Asserter;
use alloy_provider::ProviderBuilder;
use alloy_rpc_types_eth::FeeHistory;
use atlas_core::amount::RawAmount;
use atlas_core::error::ChainError;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::ChainService;
use atlas_core::transaction::TransferIntent;
use atlas_evm::service::EvmChainService;
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use std::str::FromStr;

const TEST_KEY: [u8; 32] = [
    0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e, 0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32, 0x06,
    0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06,
];

fn mk_signer() -> LocalKeySigner {
    LocalKeySigner::from_bytes(SignerId::from_str("test").unwrap(), TEST_KEY).unwrap()
}

#[tokio::test]
async fn transfer_rejects_intent_for_wrong_network() {
    // Service bound to eip155:1, intent targets eip155:8453.
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let signer = mk_signer();
    let service = EvmChainService::new(provider, NetworkId::from_str("eip155:1").unwrap(), 1, true);

    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:8453/native:eth").unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
    };
    let account = AccountRef::from_str(&signer.address()).unwrap();
    let err = service
        .transfer(intent, account, &signer)
        .await
        .unwrap_err();
    assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
}

#[tokio::test]
async fn transfer_rejects_unsupported_standard_path() {
    // Network matches but the rest of the CAIP path doesn't decode to a
    // known standard. parse_standard_from_instance returns
    // UnsupportedAssetInstance for any non-native/non-erc20 segment.
    let asserter = Asserter::new();
    // Pre-populate enough responses for nonce + fee to succeed before we
    // hit parse_standard_from_instance.
    asserter.push_success(&alloy_primitives::U64::from(0u64));
    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![100u128, 100u128],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![1_000_000u128]; 3]),
    });
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let signer = mk_signer();
    let service = EvmChainService::new(provider, NetworkId::from_str("eip155:1").unwrap(), 1, true);

    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:1/spl:something").unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
    };
    let account = AccountRef::from_str(&signer.address()).unwrap();
    let err = service
        .transfer(intent, account, &signer)
        .await
        .unwrap_err();
    assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
}

#[tokio::test]
async fn transfer_native_happy_path_broadcasts_tx_hash() {
    let asserter = Asserter::new();
    // Order of provider calls inside transfer():
    //   1. eth_getTransactionCount (nonce)
    //   2. eth_feeHistory (eip1559 fee path)
    //   3. eth_sendRawTransaction (broadcast)
    asserter.push_success(&alloy_primitives::U64::from(7u64));
    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![1_000_000_000u128, 1_000_000_000u128],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![5_000_000u128]; 3]),
    });
    let expected_hash = B256::repeat_byte(0xcd);
    asserter.push_success(&expected_hash);

    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let signer = mk_signer();
    let service = EvmChainService::new(provider, NetworkId::from_str("eip155:1").unwrap(), 1, true);

    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
        amount: RawAmount::new(BigInt::from(123_456u64), 18).unwrap(),
    };
    let account = AccountRef::from_str(&signer.address()).unwrap();
    let result = service.transfer(intent, account, &signer).await.unwrap();
    assert_eq!(result.tx_hash, format!("{:#x}", expected_hash));
}

#[tokio::test]
async fn transfer_erc20_happy_path_broadcasts_tx_hash() {
    let asserter = Asserter::new();
    asserter.push_success(&alloy_primitives::U64::from(0u64));
    asserter.push_success(&FeeHistory {
        base_fee_per_gas: vec![1_000_000_000u128, 1_000_000_000u128],
        gas_used_ratio: vec![0.5],
        base_fee_per_blob_gas: Vec::new(),
        blob_gas_used_ratio: Vec::new(),
        oldest_block: 1,
        reward: Some(vec![vec![5_000_000u128]; 3]),
    });
    let expected_hash = B256::repeat_byte(0xee);
    asserter.push_success(&expected_hash);

    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let signer = mk_signer();
    let service = EvmChainService::new(provider, NetworkId::from_str("eip155:1").unwrap(), 1, true);

    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str(
            "eip155:1/erc20:0xc083e9947cf02b8ffc7d3090ae9aea72df98fd47",
        )
        .unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000003").unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 6).unwrap(),
    };
    let account = AccountRef::from_str(&signer.address()).unwrap();
    let result = service.transfer(intent, account, &signer).await.unwrap();
    assert_eq!(result.tx_hash, format!("{:#x}", expected_hash));
}

#[tokio::test]
async fn transfer_invalid_account_returns_build_failed() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let signer = mk_signer();
    let service = EvmChainService::new(provider, NetworkId::from_str("eip155:1").unwrap(), 1, true);

    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
    };
    // An account whose value is the empty string would fail AccountRef::new.
    // Atlas IDs reject empty/whitespace; pick a non-empty but
    // syntactically-fine value that fails when parsed as an address inside
    // estimate_fee/get_nonce.
    let account = AccountRef::from_str("not-an-address").unwrap();
    let err = service
        .transfer(intent, account, &signer)
        .await
        .unwrap_err();
    // The address parsing inside reader/estimator/codec surfaces
    // InvalidAddress; the orchestrator forwards via tokio::try_join.
    assert!(matches!(err, ChainError::InvalidAddress(_)));
}
