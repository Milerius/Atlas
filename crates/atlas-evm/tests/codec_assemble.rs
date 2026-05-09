use alloy_consensus::transaction::SignerRecoverable;
use alloy_consensus::TxEnvelope;
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::Address;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::chain::Curve;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::ChainCodec;
use atlas_core::signing::{SigningPayloadKind, SigningResponse};
use atlas_core::transaction::TransferIntent;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use num_bigint::BigInt;
use std::str::FromStr;

#[test]
fn signing_request_payload_is_keccak256_of_payload() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
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

    assert_eq!(request.curve, Curve::Secp256k1);
    assert_eq!(request.payload_kind, SigningPayloadKind::TransactionDigest);
    assert_eq!(request.payload.len(), 32);
    let direct = alloy_primitives::keccak256(&unsigned.payload);
    assert_eq!(request.payload.as_slice(), direct.as_slice());
}

#[test]
fn assemble_signed_round_trip_recovers_sender_for_eip1559() {
    let codec = EvmCodec;
    // Deterministic test key.
    let key_bytes: [u8; 32] = [
        0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e, 0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32,
        0x06, 0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c,
        0x32, 0x06,
    ];
    let signer = PrivateKeySigner::from_bytes(&key_bytes.into()).unwrap();
    let expected_sender: Address = signer.address();

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            amount: RawAmount::new(BigInt::from(1_000u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
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

    // Sign the digest directly.
    let digest = alloy_primitives::B256::from_slice(&request.payload);
    let signature = signer.sign_hash_sync(&digest).unwrap();
    let mut sig_bytes = Vec::with_capacity(65);
    sig_bytes.extend_from_slice(&signature.r().to_be_bytes::<32>());
    sig_bytes.extend_from_slice(&signature.s().to_be_bytes::<32>());
    // v() returns bool in alloy-primitives 1.x (parity flag)
    sig_bytes.push(if signature.v() { 1 } else { 0 });

    let response = atlas_core::signing::SigningResponse::SignatureOnly {
        signer: SignerId::from_str("test").unwrap(),
        signature: sig_bytes,
        public_key: vec![], // unused by assemble_signed
    };

    let signed = codec.assemble_signed(unsigned, response).unwrap();

    // Decode the signed envelope and recover the sender.
    let envelope: TxEnvelope = TxEnvelope::decode_2718(&mut &signed.raw[..]).unwrap();
    let recovered = envelope.recover_signer().unwrap();
    assert_eq!(recovered, expected_sender);
}

#[test]
fn assemble_signed_passes_through_signed_transaction_variant() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let response = SigningResponse::SignedTransaction {
        signer: SignerId::from_str("test").unwrap(),
        raw: vec![0xde, 0xad, 0xbe, 0xef],
    };
    let signed = codec.assemble_signed(unsigned, response).unwrap();
    assert_eq!(signed.raw, vec![0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn assemble_signed_rejects_submitted_transaction_variant() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let response = SigningResponse::SubmittedTransaction {
        signer: SignerId::from_str("test").unwrap(),
        tx_hash: "0xabc".to_string(),
    };
    let err = codec.assemble_signed(unsigned, response).unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::ChainError::TransactionBuildFailed(_)
    ));
}
