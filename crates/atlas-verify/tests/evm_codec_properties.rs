//! Property tests for `atlas_evm::codec::EvmCodec` round-trips.
//!
//! Each property generates arbitrary primitive parameters (chain_id,
//! nonce, gas, fee, recipient, amount), runs them through Atlas's codec,
//! decodes the resulting RLP using alloy-consensus, and asserts every
//! field round-tripped lossless. This pins the codec's bit-equivalence
//! against alloy's reference encoding under Bolero's input space — far
//! more thorough than the hand-picked unit tests in
//! `crates/atlas-evm/tests/codec_*.rs`.

use alloy_consensus::{TxEip1559, TxLegacy};
use alloy_primitives::{Address, TxKind, U256};
use alloy_rlp::Decodable;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId};
use atlas_core::service::ChainCodec;
use atlas_core::transaction::TransferIntent;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use bolero::check;
use num_bigint::BigInt;
use std::str::FromStr;

/// Build an `AddressRef` from a raw 20-byte array. Always succeeds —
/// `Address::from(bytes)` formats as lowercase hex with `0x` prefix,
/// which is a valid `AddressRef` and a valid lowercase EVM address.
fn address_ref_from_bytes(bytes: [u8; 20]) -> AddressRef {
    let addr = Address::from(bytes);
    AddressRef::from_str(&format!("{addr:#x}")).expect("alloy lowercase hex is a valid AddressRef")
}

/// Native-asset EIP-1559 round-trip: every field Atlas encodes must
/// decode back equal under `alloy_consensus::TxEip1559::decode`.
#[test]
fn evm_codec_native_eip1559_round_trips_through_alloy_decode() {
    check!()
        .with_type::<(u64, u64, u64, u128, u128, [u8; 20], u128)>()
        .for_each(
            |(chain_id, nonce, gas_limit, max_fee, max_priority, to_bytes, value): &(
                u64,
                u64,
                u64,
                u128,
                u128,
                [u8; 20],
                u128,
            )| {
                let codec = EvmCodec;
                let ctx = EvmPrepareContext {
                    account: AccountRef::from_str("acct").unwrap(),
                    network: NetworkId::from_str("eip155:1").unwrap(),
                    intent: TransferIntent {
                        asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth")
                            .unwrap(),
                        to: address_ref_from_bytes(*to_bytes),
                        amount: RawAmount::new(BigInt::from(*value), 18).unwrap(),
                    },
                    chain_id: *chain_id,
                    nonce: *nonce,
                    fee: EvmFee::Eip1559 {
                        max_fee_per_gas: BigInt::from(*max_fee),
                        max_priority_fee_per_gas: BigInt::from(*max_priority),
                        gas_limit: *gas_limit,
                        l1_fee_wei: None,
                    },
                    standard: AssetStandard::Native,
                    contract: None,
                };

                let unsigned = codec
                    .prepare_transfer(ctx)
                    .expect("native encode infallible");

                // EIP-1559 unsigned RLP starts with the typed-envelope tag 0x02.
                assert_eq!(unsigned.payload[0], 0x02, "missing EIP-1559 envelope tag");
                let mut buf: &[u8] = &unsigned.payload[1..];
                let tx = TxEip1559::decode(&mut buf).expect("alloy round-trip decode");

                assert_eq!(tx.chain_id, *chain_id);
                assert_eq!(tx.nonce, *nonce);
                assert_eq!(tx.gas_limit, *gas_limit);
                assert_eq!(tx.max_fee_per_gas, *max_fee);
                assert_eq!(tx.max_priority_fee_per_gas, *max_priority);
                assert_eq!(tx.value, U256::from(*value));
                assert_eq!(tx.to, TxKind::Call(Address::from(*to_bytes)));
                assert!(tx.input.is_empty(), "native intent has empty calldata");
                assert!(tx.access_list.0.is_empty(), "no access list for native");
            },
        );
}

/// Native-asset legacy round-trip: Atlas's legacy encoding decodes
/// back through `alloy_consensus::TxLegacy::decode` field-equivalent.
#[test]
fn evm_codec_native_legacy_round_trips_through_alloy_decode() {
    check!()
        .with_type::<(u64, u64, u64, u128, [u8; 20], u128)>()
        .for_each(
            |(chain_id, nonce, gas_limit, gas_price, to_bytes, value): &(
                u64,
                u64,
                u64,
                u128,
                [u8; 20],
                u128,
            )| {
                let codec = EvmCodec;
                let ctx = EvmPrepareContext {
                    account: AccountRef::from_str("acct").unwrap(),
                    network: NetworkId::from_str("eip155:1").unwrap(),
                    intent: TransferIntent {
                        asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth")
                            .unwrap(),
                        to: address_ref_from_bytes(*to_bytes),
                        amount: RawAmount::new(BigInt::from(*value), 18).unwrap(),
                    },
                    chain_id: *chain_id,
                    nonce: *nonce,
                    fee: EvmFee::Legacy {
                        gas_price: BigInt::from(*gas_price),
                        gas_limit: *gas_limit,
                    },
                    standard: AssetStandard::Native,
                    contract: None,
                };

                let unsigned = codec.prepare_transfer(ctx).expect("native legacy encode");

                // Legacy unsigned RLP has no envelope-tag prefix.
                let mut buf: &[u8] = &unsigned.payload;
                let tx = TxLegacy::decode(&mut buf).expect("alloy legacy decode");

                assert_eq!(tx.chain_id, Some(*chain_id));
                assert_eq!(tx.nonce, *nonce);
                assert_eq!(tx.gas_limit, *gas_limit);
                assert_eq!(tx.gas_price, *gas_price);
                assert_eq!(tx.value, U256::from(*value));
                assert_eq!(tx.to, TxKind::Call(Address::from(*to_bytes)));
                assert!(tx.input.is_empty());
            },
        );
}

/// ERC-20 round-trip: the encoded transaction's `to` is the contract
/// address (not the recipient), `value` is zero, and `input` starts
/// with the `transfer(address,uint256)` selector followed by the
/// 32-byte-padded recipient and a 32-byte amount.
#[test]
fn evm_codec_erc20_calldata_carries_transfer_selector_and_args() {
    /// 4-byte function selector for `transfer(address,uint256)`.
    const ERC20_TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

    check!()
        .with_type::<(u64, u64, u64, u128, [u8; 20], [u8; 20], u128)>()
        .for_each(
            |(chain_id, nonce, gas_limit, gas_price, contract_bytes, recipient_bytes, amount): &(
                u64,
                u64,
                u64,
                u128,
                [u8; 20],
                [u8; 20],
                u128,
            )| {
                let codec = EvmCodec;
                let contract_addr = format!("{:#x}", Address::from(*contract_bytes));
                let ctx = EvmPrepareContext {
                    account: AccountRef::from_str("acct").unwrap(),
                    network: NetworkId::from_str("eip155:1").unwrap(),
                    intent: TransferIntent {
                        asset_instance_id: AssetInstanceId::from_str(
                            "eip155:1/erc20:0xc083e9947cf02b8ffc7d3090ae9aea72df98fd47",
                        )
                        .unwrap(),
                        to: address_ref_from_bytes(*recipient_bytes),
                        amount: RawAmount::new(BigInt::from(*amount), 6).unwrap(),
                    },
                    chain_id: *chain_id,
                    nonce: *nonce,
                    fee: EvmFee::Legacy {
                        gas_price: BigInt::from(*gas_price),
                        gas_limit: *gas_limit,
                    },
                    standard: AssetStandard::Erc20,
                    contract: Some(contract_addr),
                };

                let unsigned = codec.prepare_transfer(ctx).expect("erc20 encode");
                let mut buf: &[u8] = &unsigned.payload;
                let tx = TxLegacy::decode(&mut buf).expect("alloy legacy decode");

                // ERC-20 encoding contract: tx.to is the contract, value is 0.
                assert_eq!(tx.to, TxKind::Call(Address::from(*contract_bytes)));
                assert_eq!(tx.value, U256::ZERO);

                // Calldata = selector(4) + zero-padded recipient(32) + amount(32) = 68 bytes.
                let input = tx.input.as_ref();
                assert_eq!(input.len(), 4 + 32 + 32);
                assert_eq!(&input[0..4], &ERC20_TRANSFER_SELECTOR);
                // Recipient is right-aligned in the 32-byte slot — first 12 bytes are zero,
                // last 20 bytes match `recipient_bytes`.
                assert!(input[4..16].iter().all(|&b| b == 0));
                assert_eq!(&input[16..36], recipient_bytes);
                // Amount is U256 big-endian.
                let decoded_amount = U256::from_be_slice(&input[36..68]);
                assert_eq!(decoded_amount, U256::from(*amount));
            },
        );
}
