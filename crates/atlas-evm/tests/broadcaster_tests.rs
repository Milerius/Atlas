//! Tests for `EvmBroadcaster` against a mocked alloy provider.

use alloy_primitives::B256;
use alloy_provider::mock::Asserter;
use alloy_provider::ProviderBuilder;
use atlas_core::error::ChainError;
use atlas_core::id::NetworkId;
use atlas_core::service::ChainBroadcaster;
use atlas_core::transaction::SignedTransaction;
use atlas_evm::broadcaster::EvmBroadcaster;
use std::str::FromStr;

#[tokio::test]
async fn broadcast_returns_tx_hash_from_provider() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    // eth_sendRawTransaction returns the tx hash.
    let expected = B256::repeat_byte(0xab);
    asserter.push_success(&expected);

    let broadcaster = EvmBroadcaster::new(provider);
    let signed = SignedTransaction {
        network: NetworkId::from_str("eip155:1").unwrap(),
        raw: vec![0x02, 0xde, 0xad, 0xbe, 0xef],
    };
    let result = broadcaster.broadcast(signed).await.unwrap();
    assert_eq!(result.tx_hash, format!("{:#x}", expected));
}

#[tokio::test]
async fn broadcast_rejects_empty_raw_immediately() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let broadcaster = EvmBroadcaster::new(provider);
    let signed = SignedTransaction {
        network: NetworkId::from_str("eip155:1").unwrap(),
        raw: Vec::new(),
    };
    let err = broadcaster.broadcast(signed).await.unwrap_err();
    assert!(matches!(err, ChainError::BroadcastFailed(_)));
}
