mod common;

use atlas_core::{
    amount::RawAmount,
    id::{AccountRef, AddressRef, SignerId},
    service::{ChainService, MockEvmService},
    signing::{MockSigner, SignerProvider},
    transaction::TransferIntent,
};
use common::registry;
use num_bigint::BigInt;
use std::str::FromStr;

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
        asset_instance_id: asset.id.clone(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(100_000_000u64), asset.decimals).unwrap(),
    };

    let unsigned = service
        .prepare_transfer(
            AccountRef::from_str("account-1").unwrap(),
            network.id.clone(),
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
