//! End-to-end smoke flow through the in-tree mock EVM service.
//!
//! Exercises: load the official registry → look up a concrete USDC
//! `AssetInstance` on Base → drive the full `ChainService::transfer`
//! pipeline against `MockEvmChainService` → assert the deterministic
//! `0xmock` broadcast hash.
//!
//! Mirrors the shape a real wallet integration would take, just with the
//! mock standing in for the real codec/reader/fee/broadcaster stack.

use atlas_core::amount::RawAmount;
use atlas_core::id::{AccountRef, AddressRef, SignerId};
use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
use atlas_core::service::ChainService;
use atlas_core::signing::MockSigner;
use atlas_core::transaction::TransferIntent;
use atlas_evm::mock::MockEvmChainService;
use num_bigint::BigInt;
use std::str::FromStr;

fn registry() -> Registry {
    let chain_doc: ChainRegistryDocument = serde_json::from_str(CHAIN_REGISTRY_JSON).unwrap();
    let asset_doc: AssetRegistryDocument = serde_json::from_str(ASSET_REGISTRY_JSON).unwrap();
    Registry::from_documents(chain_doc, asset_doc).unwrap()
}

#[tokio::test]
async fn base_usdc_transfer_smoke_flow_uses_exact_asset_instance() {
    let registry = registry();
    let asset = registry
        .asset_instance("eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")
        .unwrap();

    let service = MockEvmChainService;
    let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
    let intent = TransferIntent {
        asset_instance_id: asset.id.clone(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(100_000_000u64), asset.decimals).unwrap(),
    };

    let broadcast = service
        .transfer(intent, AccountRef::from_str("account-1").unwrap(), &signer)
        .await
        .unwrap();

    assert_eq!(broadcast.tx_hash, "0xmock");
}
