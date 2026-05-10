//! `EvmReader` — RPC reads (balance, nonce, status) via alloy-provider.

use alloy_primitives::{Address, B256, U256};
use alloy_provider::Provider;
use alloy_rpc_types_eth::TransactionInput;
use async_trait::async_trait;
use atlas_core::amount::RawAmount;
use atlas_core::asset::{AssetInstance, AssetStandard};
use atlas_core::error::ChainError;
use atlas_core::fee::TransactionStatus;
use atlas_core::id::{AddressRef, NetworkId};
use atlas_core::service::ChainReader;
use num_bigint::BigInt;
use std::str::FromStr;

use crate::error::{map_node_err, map_transport_err};

/// keccak256("balanceOf(address)")[..4]
const ERC20_BALANCE_OF_SELECTOR: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

pub struct EvmReader<P> {
    provider: P,
}

impl<P> EvmReader<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: Provider + Clone> ChainReader for EvmReader<P> {
    async fn get_balance(
        &self,
        instance: &AssetInstance,
        address: &AddressRef,
    ) -> Result<RawAmount, ChainError> {
        let addr = parse_address(address.as_str())?;
        let raw = match instance.standard {
            AssetStandard::Native => {
                let balance: U256 = self
                    .provider
                    .get_balance(addr)
                    .await
                    .map_err(map_transport_err)?;
                u256_to_bigint(balance)
            }
            AssetStandard::Erc20 => {
                let contract = instance.contract.as_deref().ok_or_else(|| {
                    ChainError::TransactionBuildFailed(
                        "ERC-20 instance missing contract".to_string(),
                    )
                })?;
                let contract_addr = parse_address(contract)?;
                let mut calldata = Vec::with_capacity(36);
                calldata.extend_from_slice(&ERC20_BALANCE_OF_SELECTOR);
                calldata.extend_from_slice(&[0u8; 12]);
                calldata.extend_from_slice(addr.as_slice());
                let result = self
                    .provider
                    .call(
                        alloy_rpc_types_eth::TransactionRequest::default()
                            .to(contract_addr)
                            .input(TransactionInput::new(calldata.into())),
                    )
                    .await
                    .map_err(map_node_err)?;
                if result.len() != 32 {
                    return Err(ChainError::Rpc(
                        atlas_core::error::RpcError::MalformedResponse(format!(
                            "balanceOf returned {} bytes, expected 32",
                            result.len()
                        )),
                    ));
                }
                let value = U256::from_be_slice(&result);
                u256_to_bigint(value)
            }
            AssetStandard::Spl => {
                return Err(ChainError::StandardNotSupported {
                    instance: instance.id.clone(),
                    standard: AssetStandard::Spl,
                });
            }
        };

        // `u256_to_bigint` always returns a non-negative value, so
        // `RawAmount::new` cannot fail here.
        Ok(RawAmount::new(raw, instance.decimals).expect("u256-derived value is non-negative"))
    }

    async fn get_nonce(
        &self,
        _network: &NetworkId,
        address: &AddressRef,
    ) -> Result<u64, ChainError> {
        let addr = parse_address(address.as_str())?;
        let nonce = self
            .provider
            .get_transaction_count(addr)
            .await
            .map_err(map_transport_err)?;
        Ok(nonce)
    }

    async fn get_transaction_status(
        &self,
        _network: &NetworkId,
        hash: &str,
    ) -> Result<TransactionStatus, ChainError> {
        let tx_hash =
            B256::from_str(hash).map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;
        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(map_transport_err)?;

        match receipt {
            Some(r) => {
                let block_number = r.block_number.unwrap_or(0);
                let gas_used = BigInt::from(r.gas_used);
                if r.status() {
                    Ok(TransactionStatus::Confirmed {
                        hash: hash.to_string(),
                        block_number,
                        gas_used,
                    })
                } else {
                    Ok(TransactionStatus::Failed {
                        hash: hash.to_string(),
                        reason: "transaction reverted".to_string(),
                    })
                }
            }
            None => {
                let pending = self
                    .provider
                    .get_transaction_by_hash(tx_hash)
                    .await
                    .map_err(map_transport_err)?;
                if pending.is_some() {
                    Ok(TransactionStatus::Pending {
                        hash: hash.to_string(),
                    })
                } else {
                    Ok(TransactionStatus::NotFound {
                        hash: hash.to_string(),
                    })
                }
            }
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn parse_address(s: &str) -> Result<Address, ChainError> {
    Address::from_str(s).map_err(|e| ChainError::InvalidAddress(format!("{}: {}", s, e)))
}

fn u256_to_bigint(v: U256) -> BigInt {
    let bytes = v.to_be_bytes::<32>();
    BigInt::from_bytes_be(num_bigint::Sign::Plus, &bytes)
}
