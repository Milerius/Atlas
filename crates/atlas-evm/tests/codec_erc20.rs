use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_primitives::{address, Bytes, TxKind, U256};
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId};
use atlas_core::service::ChainCodec;
use atlas_core::transaction::TransferIntent;
use atlas_evm::abi::encode_erc20_transfer;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use num_bigint::BigInt;
use std::str::FromStr;

#[test]
fn prepare_transfer_erc20_uses_contract_as_to_and_zero_value() {
    let codec = EvmCodec;
    // Base USDC contract.
    let usdc_base = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
    let recipient = "0x0000000000000000000000000000000000000003";
    let amount: u64 = 1_000_000; // 1 USDC (6 decimals)

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:8453").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(
                "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            )
            .unwrap(),
            to: AddressRef::from_str(recipient).unwrap(),
            amount: RawAmount::new(BigInt::from(amount), 6).unwrap(),
        },
        chain_id: 8453,
        nonce: 12,
        fee: EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(2_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000u64),
            gas_limit: 60_000,
            l1_fee_wei: None,
        },
        standard: AssetStandard::Erc20,
        contract: Some(usdc_base.to_string()),
    };

    let unsigned = codec.prepare_transfer(ctx).unwrap();

    // Reference: build same TxEip1559 directly and assert bytes match.
    let calldata = encode_erc20_transfer(
        address!("0000000000000000000000000000000000000003"),
        U256::from(amount),
    );
    let reference = TxEip1559 {
        chain_id: 8453,
        nonce: 12,
        gas_limit: 60_000,
        max_fee_per_gas: 2_000_000_000u128,
        max_priority_fee_per_gas: 1_000_000u128,
        to: TxKind::Call(address!("833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")),
        value: U256::ZERO,
        access_list: Default::default(),
        input: Bytes::from(calldata),
    };
    let mut ref_bytes = Vec::new();
    reference.encode_for_signing(&mut ref_bytes);

    assert_eq!(unsigned.payload, ref_bytes);
}

#[test]
fn prepare_transfer_erc20_without_contract_errors() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:8453").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(
                "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            )
            .unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000003").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 6).unwrap(),
        },
        chain_id: 8453,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 60_000,
        },
        standard: AssetStandard::Erc20,
        contract: None, // ← missing
    };
    let err = codec.prepare_transfer(ctx).unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::ChainError::TransactionBuildFailed(_)
    ));
}

#[test]
fn prepare_transfer_spl_returns_standard_not_supported() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(
                "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/spl:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            )
            .unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 6).unwrap(),
        },
        chain_id: 0,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Spl,
        contract: Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()),
    };
    let err = codec.prepare_transfer(ctx).unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::ChainError::StandardNotSupported { .. }
    ));
}
