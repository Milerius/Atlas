//! End-to-end smoke test: real EvmCodec + real LocalKeySigner + real
//! signature recovery. The Provider isn't called for codec-only flow, so we
//! exercise the codec + signer round-trip directly without RPC.

use alloy_consensus::transaction::SignerRecoverable;
use alloy_consensus::TxEnvelope;
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::Address;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::ChainCodec;
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::TransferIntent;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use std::str::FromStr;

const TEST_KEY: [u8; 32] = [
    0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e, 0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32, 0x06,
    0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06,
];

#[tokio::test]
async fn build_sign_assemble_recovers_to_signer_address() {
    let codec = EvmCodec;
    let signer = LocalKeySigner::from_bytes(SignerId::from_str("test").unwrap(), TEST_KEY).unwrap();
    let expected_sender: Address = signer.address().parse().unwrap();

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str(&signer.address()).unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000003").unwrap(),
            amount: RawAmount::new(BigInt::from(123_456u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 5,
        fee: EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        },
        standard: AssetStandard::Native,
        contract: None,
    };

    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let request = codec.signing_request(&unsigned).unwrap();
    let response = signer.sign(request).await.unwrap();
    let signed = codec.assemble_signed(unsigned, response).unwrap();

    let envelope: TxEnvelope = TxEnvelope::decode_2718(&mut &signed.raw[..]).unwrap();
    let recovered = envelope.recover_signer().unwrap();
    assert_eq!(recovered, expected_sender);
}
