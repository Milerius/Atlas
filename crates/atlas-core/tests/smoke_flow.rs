use atlas_core::{
    amount::RawAmount,
    id::{AccountRef, AssetInstanceId, NetworkId, SignerId},
    registry::{AssetRegistryDocument, ChainRegistryDocument, Registry},
    service::{ChainService, MockEvmService},
    signing::{MockSigner, SignerProvider},
    transaction::TransferIntent,
};
use num_bigint::BigInt;
use std::str::FromStr;

fn registry() -> Registry {
    let chain_doc: ChainRegistryDocument =
        serde_json::from_str(include_str!("fixtures/chain_registry.valid.json")).unwrap();
    let asset_doc: AssetRegistryDocument =
        serde_json::from_str(include_str!("fixtures/asset_registry.valid.json")).unwrap();
    Registry::from_documents(chain_doc, asset_doc).unwrap()
}

#[tokio::test]
async fn base_usdc_transfer_smoke_flow_uses_exact_asset_instance() {
    let registry = registry();
    let asset = registry
        .asset_instance("eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")
        .unwrap();
    let network = registry.network(asset.network.as_str()).unwrap();

    let service = MockEvmService;
    let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
    let intent = TransferIntent {
        asset_instance_id: AssetInstanceId::from_str(asset.id.as_str()).unwrap(),
        to: "0x0000000000000000000000000000000000000001".to_string(),
        amount: RawAmount::new(BigInt::from(100_000_000u64), asset.decimals),
    };

    let unsigned = service
        .prepare_transfer(
            AccountRef::from_str("account-1").unwrap(),
            NetworkId::from_str(network.id.as_str()).unwrap(),
            intent,
        )
        .await
        .unwrap();
    let request = service.signing_request(&unsigned).unwrap();
    let response = signer.sign(request).await.unwrap();
    let signed = service
        .assemble_signed_transaction(unsigned, response)
        .unwrap();
    let broadcast = service.broadcast(signed).await.unwrap();

    assert_eq!(broadcast.tx_hash, "0xmock");
}
