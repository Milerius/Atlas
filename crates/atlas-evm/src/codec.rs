//! `EvmCodec` — the pure encoding seam. No RPC, no I/O.
//!
//! Produces RLP-encoded EVM transactions (legacy or EIP-1559) from a
//! `TransferIntent`, computes the keccak256 signing digest, and assembles
//! a signed transaction from any of the three [`SigningResponse`] shapes.

use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope, TxLegacy};
use alloy_primitives::{Address, Bytes, Signature, TxKind, B256, U256};
use alloy_rlp::Decodable;
use atlas_core::asset::AssetStandard;
use atlas_core::chain::Curve;
use atlas_core::error::ChainError;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, NetworkId};
use atlas_core::service::ChainCodec;
use atlas_core::signing::{SigningPayloadKind, SigningRequest, SigningResponse};
use atlas_core::transaction::{SignedTransaction, TransferIntent, UnsignedTransaction};
use num_bigint::BigInt;
use std::str::FromStr;

use crate::abi::encode_erc20_transfer;

/// Per-EVM-chain context passed into [`EvmCodec::prepare_transfer`].
#[derive(Clone, Debug)]
pub struct EvmPrepareContext {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    /// EIP-155 chain id used for replay protection.
    pub chain_id: u64,
    pub nonce: u64,
    pub fee: EvmFee,
    /// Asset standard of `intent.asset_instance_id`. Caller resolves this
    /// from the registry; the codec doesn't carry a registry.
    pub standard: AssetStandard,
    /// Required for ERC-20 transfers: the contract address. `None` for
    /// `Native` standard.
    pub contract: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EvmCodec;

impl ChainCodec for EvmCodec {
    type PrepareContext = EvmPrepareContext;

    fn prepare_transfer(&self, ctx: EvmPrepareContext) -> Result<UnsignedTransaction, ChainError> {
        // Parse recipient address.
        let to_addr = parse_address(ctx.intent.to.as_str())?;

        // Match on standard to decide native (value) vs ERC-20 (data).
        let (tx_kind, value, data) = match ctx.standard {
            AssetStandard::Native => {
                let value = bigint_to_u256(ctx.intent.amount.value())?;
                (TxKind::Call(to_addr), value, Vec::new())
            }
            AssetStandard::Erc20 => {
                let contract = ctx.contract.as_deref().ok_or_else(|| {
                    ChainError::TransactionBuildFailed(
                        "ERC-20 transfer requires contract address".to_string(),
                    )
                })?;
                let contract_addr = parse_address(contract)?;
                let amount = bigint_to_u256(ctx.intent.amount.value())?;
                let calldata = encode_erc20_transfer(to_addr, amount);
                (TxKind::Call(contract_addr), U256::ZERO, calldata)
            }
            AssetStandard::Spl => {
                return Err(ChainError::StandardNotSupported {
                    instance: ctx.intent.asset_instance_id.clone(),
                    standard: AssetStandard::Spl,
                });
            }
        };

        // Build either TxEip1559 or TxLegacy depending on EvmFee variant.
        let envelope_bytes = match ctx.fee {
            EvmFee::Eip1559 {
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas_limit,
                l1_fee_wei: _, // L1 fee influences fee accounting, not encoding
            } => {
                let tx = TxEip1559 {
                    chain_id: ctx.chain_id,
                    nonce: ctx.nonce,
                    gas_limit,
                    max_fee_per_gas: bigint_to_u128(&max_fee_per_gas)?,
                    max_priority_fee_per_gas: bigint_to_u128(&max_priority_fee_per_gas)?,
                    to: tx_kind,
                    value,
                    access_list: Default::default(),
                    input: Bytes::from(data),
                };
                let mut buf = Vec::new();
                tx.encode_for_signing(&mut buf);
                buf
            }
            EvmFee::Legacy {
                gas_price,
                gas_limit,
            } => {
                let tx = TxLegacy {
                    chain_id: Some(ctx.chain_id),
                    nonce: ctx.nonce,
                    gas_price: bigint_to_u128(&gas_price)?,
                    gas_limit,
                    to: tx_kind,
                    value,
                    input: Bytes::from(data),
                };
                let mut buf = Vec::new();
                tx.encode_for_signing(&mut buf);
                buf
            }
        };

        Ok(UnsignedTransaction {
            account: ctx.account,
            network: ctx.network,
            intent: ctx.intent,
            payload: envelope_bytes,
        })
    }

    fn signing_request(
        &self,
        unsigned: &UnsignedTransaction,
    ) -> Result<SigningRequest, ChainError> {
        // The payload is the encode_for_signing bytes. Compute keccak256 ourselves.
        let digest: B256 = alloy_primitives::keccak256(&unsigned.payload);
        Ok(SigningRequest {
            account: unsigned.account.clone(),
            network: unsigned.network.clone(),
            curve: Curve::Secp256k1,
            payload_kind: SigningPayloadKind::TransactionDigest,
            payload: digest.to_vec(),
        })
    }

    fn assemble_signed(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError> {
        match response {
            SigningResponse::SignatureOnly { signature, .. } => {
                // Expect 65 bytes: r ‖ s ‖ v (or recovery id).
                if signature.len() != 65 {
                    return Err(ChainError::TransactionBuildFailed(format!(
                        "expected 65-byte signature, got {}",
                        signature.len()
                    )));
                }
                let sig = parse_65_byte_signature(&signature)?;

                // Re-decode the unsigned envelope from payload to know which
                // tx variant we're assembling.
                let raw = encode_signed_from_unsigned(&unsigned.payload, sig)?;
                Ok(SignedTransaction {
                    network: unsigned.network,
                    raw,
                })
            }
            SigningResponse::SignedTransaction { raw, .. } => Ok(SignedTransaction {
                network: unsigned.network,
                raw,
            }),
            SigningResponse::SubmittedTransaction { tx_hash, .. } => {
                Err(ChainError::TransactionBuildFailed(format!(
                    "atlas-evm assemble expects signed bytes, got submitted hash {tx_hash}"
                )))
            }
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn parse_address(s: &str) -> Result<Address, ChainError> {
    Address::from_str(s).map_err(|e| ChainError::InvalidAddress(format!("{}: {}", s, e)))
}

fn bigint_to_u256(v: &BigInt) -> Result<U256, ChainError> {
    use num_bigint::Sign;
    if v.sign() == Sign::Minus {
        return Err(ChainError::TransactionBuildFailed(
            "negative amount".to_string(),
        ));
    }
    let (_, bytes_be) = v.to_bytes_be();
    if bytes_be.len() > 32 {
        return Err(ChainError::TransactionBuildFailed(
            "amount exceeds 256 bits".to_string(),
        ));
    }
    let mut padded = [0u8; 32];
    padded[32 - bytes_be.len()..].copy_from_slice(&bytes_be);
    Ok(U256::from_be_bytes(padded))
}

fn bigint_to_u128(v: &BigInt) -> Result<u128, ChainError> {
    use num_bigint::Sign;
    if v.sign() == Sign::Minus {
        return Err(ChainError::TransactionBuildFailed(
            "negative fee".to_string(),
        ));
    }
    let (_, bytes_be) = v.to_bytes_be();
    if bytes_be.len() > 16 {
        return Err(ChainError::FeeEstimationFailed(
            "fee value exceeds u128".to_string(),
        ));
    }
    let mut padded = [0u8; 16];
    padded[16 - bytes_be.len()..].copy_from_slice(&bytes_be);
    Ok(u128::from_be_bytes(padded))
}

fn parse_65_byte_signature(bytes: &[u8]) -> Result<Signature, ChainError> {
    // Accept either 65-byte r ‖ s ‖ v form. v can be 0/1 or 27/28.
    let r = U256::from_be_slice(&bytes[0..32]);
    let s = U256::from_be_slice(&bytes[32..64]);
    let v_byte = bytes[64];
    let y_parity = match v_byte {
        0 | 27 => false,
        1 | 28 => true,
        _ => {
            // EIP-155: v = 35 + 2 * chain_id + parity. Caller may have already
            // applied EIP-155; treat anything else as an error.
            return Err(ChainError::InvalidAddress(format!(
                "unexpected signature recovery byte: {}",
                v_byte
            )));
        }
    };
    Ok(Signature::new(r, s, y_parity))
}

/// Take the unsigned RLP bytes (encode_for_signing output) and a signature,
/// decode the unsigned tx variant, then encode the signed envelope.
fn encode_signed_from_unsigned(
    unsigned_payload: &[u8],
    sig: Signature,
) -> Result<Vec<u8>, ChainError> {
    // Try EIP-1559 first (typed envelope: leading byte = 0x02)
    if unsigned_payload.first() == Some(&0x02) {
        decode_eip1559_unsigned_and_sign(unsigned_payload, sig)
    } else {
        decode_legacy_unsigned_and_sign(unsigned_payload, sig)
    }
}

fn decode_eip1559_unsigned_and_sign(
    unsigned_payload: &[u8],
    sig: Signature,
) -> Result<Vec<u8>, ChainError> {
    use alloy_eips::eip2718::Encodable2718;
    let mut buf: &[u8] = &unsigned_payload[1..]; // skip 0x02 envelope tag
    let tx = TxEip1559::decode(&mut buf).map_err(|e| {
        ChainError::TransactionBuildFailed(format!("EIP-1559 decode failed: {}", e))
    })?;
    let signed = tx.into_signed(sig);
    let envelope: TxEnvelope = signed.into();
    let mut out = Vec::new();
    envelope.encode_2718(&mut out);
    Ok(out)
}

fn decode_legacy_unsigned_and_sign(
    unsigned_payload: &[u8],
    sig: Signature,
) -> Result<Vec<u8>, ChainError> {
    use alloy_eips::eip2718::Encodable2718;
    let mut buf: &[u8] = unsigned_payload;
    let tx = TxLegacy::decode(&mut buf)
        .map_err(|e| ChainError::TransactionBuildFailed(format!("Legacy decode failed: {}", e)))?;
    let signed = tx.into_signed(sig);
    let envelope: TxEnvelope = signed.into();
    let mut out = Vec::new();
    envelope.encode_2718(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigint_to_u256_zero() {
        let v = BigInt::from(0u64);
        assert_eq!(bigint_to_u256(&v).unwrap(), U256::ZERO);
    }

    #[test]
    fn bigint_to_u256_max_u128() {
        let v = BigInt::from(u128::MAX);
        let u = bigint_to_u256(&v).unwrap();
        assert_eq!(u, U256::from(u128::MAX));
    }

    #[test]
    fn bigint_to_u256_rejects_negative() {
        let v = BigInt::from(-1);
        assert!(bigint_to_u256(&v).is_err());
    }

    #[test]
    fn bigint_to_u128_rejects_overflow() {
        let v = BigInt::from(u128::MAX) + BigInt::from(1u64);
        assert!(bigint_to_u128(&v).is_err());
    }
}
