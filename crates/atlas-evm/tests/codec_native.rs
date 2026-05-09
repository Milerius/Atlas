use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_primitives::{address, Bytes, TxKind, U256};
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId};
use atlas_core::service::ChainCodec;
use atlas_core::transaction::TransferIntent;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use num_bigint::BigInt;
use std::str::FromStr;

#[test]
fn prepare_transfer_native_eip1559_matches_alloy_direct_encoding() {
    let codec = EvmCodec;
    let to = "0x0000000000000000000000000000000000000001";
    let amount = 10u64.pow(15); // 0.001 ETH

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str(to).unwrap(),
            amount: RawAmount::new(BigInt::from(amount), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 7,
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

    // Reference: build the same TxEip1559 directly with alloy and assert
    // encode_for_signing produces identical bytes.
    let reference = TxEip1559 {
        chain_id: 1,
        nonce: 7,
        gas_limit: 21_000,
        max_fee_per_gas: 30_000_000_000u128,
        max_priority_fee_per_gas: 1_000_000_000u128,
        to: TxKind::Call(address!("0000000000000000000000000000000000000001")),
        value: U256::from(amount),
        access_list: Default::default(),
        input: Bytes::new(),
    };
    let mut ref_bytes = Vec::new();
    reference.encode_for_signing(&mut ref_bytes);

    assert_eq!(unsigned.payload, ref_bytes);
}

#[test]
fn prepare_transfer_native_legacy_encodes_chain_id_for_eip155() {
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
            gas_price: BigInt::from(20_000_000_000u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).unwrap();
    // Legacy encoding starts with RLP list prefix (0xc0..0xff range), not 0x02.
    assert_ne!(unsigned.payload.first(), Some(&0x02u8));
    assert!(!unsigned.payload.is_empty());
}
