//! Tests for `EvmReader` against a mocked alloy provider.

use alloy_primitives::U256;
use alloy_provider::mock::Asserter;
use alloy_provider::ProviderBuilder;
use alloy_rpc_types_eth::TransactionReceipt;
use atlas_core::asset::{AssetInstance, AssetStandard};
use atlas_core::error::ChainError;
use atlas_core::fee::TransactionStatus;
use atlas_core::id::{AddressRef, AssetInstanceId, AssetInstrumentId, NetworkId};
use atlas_core::service::ChainReader;
use atlas_evm::reader::EvmReader;
use std::str::FromStr;

const ADDR: &str = "0x39fa8c5f2793459d6622857e7d9fbb4bd91766d3";
const TOKEN: &str = "0xc083e9947cf02b8ffc7d3090ae9aea72df98fd47";

fn native_eth() -> AssetInstance {
    AssetInstance {
        id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
        instrument_id: AssetInstrumentId::from_str("eth.native").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        standard: AssetStandard::Native,
        decimals: 18,
        contract: None,
        capabilities: Vec::new(),
        metadata: Default::default(),
    }
}

fn erc20_token() -> AssetInstance {
    AssetInstance {
        id: AssetInstanceId::from_str(&format!("eip155:1/erc20:{}", TOKEN)).unwrap(),
        instrument_id: AssetInstrumentId::from_str("tok.test").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        standard: AssetStandard::Erc20,
        decimals: 6,
        contract: Some(TOKEN.to_string()),
        capabilities: Vec::new(),
        metadata: Default::default(),
    }
}

#[tokio::test]
async fn get_balance_native_returns_amount_from_get_balance() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    asserter.push_success(&U256::from(1_000_000_000_000_000_000u128));

    let reader = EvmReader::new(provider);
    let balance = reader
        .get_balance(&native_eth(), &AddressRef::from_str(ADDR).unwrap())
        .await
        .unwrap();
    assert_eq!(balance.value().to_string(), "1000000000000000000");
}

#[tokio::test]
async fn get_balance_erc20_returns_amount_from_call() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    let mut buf = [0u8; 32];
    buf[24..32].copy_from_slice(&500_000u64.to_be_bytes());
    asserter.push_success(&alloy_primitives::Bytes::from(buf.to_vec()));

    let reader = EvmReader::new(provider);
    let balance = reader
        .get_balance(&erc20_token(), &AddressRef::from_str(ADDR).unwrap())
        .await
        .unwrap();
    assert_eq!(balance.value().to_string(), "500000");
}

#[tokio::test]
async fn get_balance_erc20_rejects_malformed_response_size() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    asserter.push_success(&alloy_primitives::Bytes::from(vec![0u8; 16]));

    let reader = EvmReader::new(provider);
    let err = reader
        .get_balance(&erc20_token(), &AddressRef::from_str(ADDR).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ChainError::Rpc(atlas_core::error::RpcError::MalformedResponse(_))
    ));
}

#[tokio::test]
async fn get_balance_spl_returns_standard_not_supported() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let reader = EvmReader::new(provider);

    let spl = AssetInstance {
        id: AssetInstanceId::from_str(
            "solana:101/spl:Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        )
        .unwrap(),
        instrument_id: AssetInstrumentId::from_str("usdc.circle").unwrap(),
        network: NetworkId::from_str("solana:101").unwrap(),
        standard: AssetStandard::Spl,
        decimals: 6,
        contract: Some("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string()),
        capabilities: Vec::new(),
        metadata: Default::default(),
    };
    let err = reader
        .get_balance(&spl, &AddressRef::from_str(ADDR).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ChainError::StandardNotSupported {
            standard: AssetStandard::Spl,
            ..
        }
    ));
}

#[tokio::test]
async fn get_balance_erc20_missing_contract_errors() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let reader = EvmReader::new(provider);
    let mut bad = erc20_token();
    bad.contract = None;
    let err = reader
        .get_balance(&bad, &AddressRef::from_str(ADDR).unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, ChainError::TransactionBuildFailed(_)));
}

#[tokio::test]
async fn get_balance_invalid_address_errors() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let reader = EvmReader::new(provider);
    let err = reader
        .get_balance(
            &native_eth(),
            &AddressRef::from_str("not-an-address").unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ChainError::InvalidAddress(_)));
}

#[tokio::test]
async fn get_nonce_returns_provider_value() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    asserter.push_success(&alloy_primitives::U64::from(42u64));

    let reader = EvmReader::new(provider);
    let nonce = reader
        .get_nonce(
            &NetworkId::from_str("eip155:1").unwrap(),
            &AddressRef::from_str(ADDR).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(nonce, 42);
}

#[tokio::test]
async fn get_nonce_invalid_address_errors() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let reader = EvmReader::new(provider);
    let err = reader
        .get_nonce(
            &NetworkId::from_str("eip155:1").unwrap(),
            &AddressRef::from_str("not-an-address").unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ChainError::InvalidAddress(_)));
}

#[tokio::test]
async fn get_transaction_status_confirmed_when_status_true() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    let receipt: TransactionReceipt = serde_json::from_str(
        r#"{
            "transactionHash": "0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f",
            "transactionIndex": "0x0",
            "blockHash": "0x8e38b4dbf6b11fcc3b9dee84fb7986e29ca0a02cecd8977c161ff7333329681e",
            "blockNumber": "0xf4240",
            "cumulativeGasUsed": "0x5208",
            "gasUsed": "0x5208",
            "contractAddress": null,
            "status": "0x1",
            "from": "0x39fa8c5f2793459d6622857e7d9fbb4bd91766d3",
            "to": "0xc083e9947cf02b8ffc7d3090ae9aea72df98fd47",
            "type": "0x2",
            "effectiveGasPrice": "0x12bfb19e60",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        }"#,
    )
    .unwrap();
    asserter.push_success(&Some(receipt));

    let hash = "0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f";
    let reader = EvmReader::new(provider);
    let status = reader
        .get_transaction_status(&NetworkId::from_str("eip155:1").unwrap(), hash)
        .await
        .unwrap();
    match status {
        TransactionStatus::Confirmed { block_number, .. } => {
            assert_eq!(block_number, 0xf4240);
        }
        other => panic!("expected Confirmed, got {other:?}"),
    }
}

#[tokio::test]
async fn get_transaction_status_failed_when_status_false() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());

    let receipt: TransactionReceipt = serde_json::from_str(
        r#"{
            "transactionHash": "0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f",
            "transactionIndex": "0x0",
            "blockHash": "0x8e38b4dbf6b11fcc3b9dee84fb7986e29ca0a02cecd8977c161ff7333329681e",
            "blockNumber": "0x10",
            "cumulativeGasUsed": "0x5208",
            "gasUsed": "0x5208",
            "contractAddress": null,
            "status": "0x0",
            "from": "0x39fa8c5f2793459d6622857e7d9fbb4bd91766d3",
            "to": "0xc083e9947cf02b8ffc7d3090ae9aea72df98fd47",
            "type": "0x2",
            "effectiveGasPrice": "0x1",
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        }"#,
    )
    .unwrap();
    asserter.push_success(&Some(receipt));

    let reader = EvmReader::new(provider);
    let status = reader
        .get_transaction_status(
            &NetworkId::from_str("eip155:1").unwrap(),
            "0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f",
        )
        .await
        .unwrap();
    assert!(matches!(status, TransactionStatus::Failed { .. }));
}

#[tokio::test]
async fn get_transaction_status_pending_when_no_receipt_but_in_mempool() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    asserter.push_success(&Option::<TransactionReceipt>::None);
    let pending_tx: alloy_rpc_types_eth::Transaction = serde_json::from_str(
        r#"{
            "hash":"0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f",
            "nonce":"0x1",
            "blockHash":null,
            "blockNumber":null,
            "transactionIndex":null,
            "from":"0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266",
            "to":"0x5fbdb2315678afecb367f032d93f642f64180aa3",
            "value":"0x0",
            "gasPrice":"0x3a29f0f8",
            "gas":"0x1c9c380",
            "maxFeePerGas":"0xba43b7400",
            "maxPriorityFeePerGas":"0x5f5e100",
            "input":"0x",
            "r":"0xd309309a59a49021281cb6bb41d164c96eab4e50f0c1bd24c03ca336e7bc2bb7",
            "s":"0x28a7f089143d0a1355ebeb2a1b9f0e5ad9eca4303021c1400d61bc23c9ac5319",
            "v":"0x0",
            "yParity":"0x0",
            "chainId":"0x1",
            "accessList":[],
            "type":"0x2"
        }"#,
    )
    .unwrap();
    asserter.push_success(&Some(pending_tx));

    let reader = EvmReader::new(provider);
    let status = reader
        .get_transaction_status(
            &NetworkId::from_str("eip155:1").unwrap(),
            "0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f",
        )
        .await
        .unwrap();
    assert!(matches!(status, TransactionStatus::Pending { .. }));
}

#[tokio::test]
async fn get_transaction_status_not_found_when_no_receipt_no_mempool() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter.clone());
    asserter.push_success(&Option::<TransactionReceipt>::None);
    asserter.push_success(&Option::<alloy_rpc_types_eth::Transaction>::None);

    let reader = EvmReader::new(provider);
    let status = reader
        .get_transaction_status(
            &NetworkId::from_str("eip155:1").unwrap(),
            "0x0101010101010101010101010101010101010101010101010101010101010101",
        )
        .await
        .unwrap();
    assert!(matches!(status, TransactionStatus::NotFound { .. }));
}

#[tokio::test]
async fn get_transaction_status_invalid_hash_returns_build_failed() {
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .connect_mocked_client(asserter);
    let reader = EvmReader::new(provider);
    let err = reader
        .get_transaction_status(&NetworkId::from_str("eip155:1").unwrap(), "not-a-hash")
        .await
        .unwrap_err();
    assert!(matches!(err, ChainError::TransactionBuildFailed(_)));
}
