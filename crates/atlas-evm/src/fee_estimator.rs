//! `EvmFeeEstimator` — fee suggestion via `eth_feeHistory` + fallback.

use alloy_provider::Provider;
use alloy_rpc_types_eth::BlockNumberOrTag;
use async_trait::async_trait;
use atlas_core::error::ChainError;
use atlas_core::fee::EvmFee;
use atlas_core::id::AddressRef;
use atlas_core::service::FeeEstimator;
use atlas_core::transaction::TransferIntent;
use num_bigint::BigInt;

use crate::error::map_transport_err;

/// Minimum priority fee in wei (0.001 gwei).
const MIN_PRIORITY_FEE_WEI: u128 = 1_000_000;
/// Maximum priority fee in wei (0.2 gwei).
const MAX_PRIORITY_FEE_WEI: u128 = 200_000_000;
/// Multiplier applied to baseFee when computing maxFeePerGas.
const BASE_FEE_MULTIPLIER: u128 = 2;
/// Default native-transfer gas limit (21000).
const NATIVE_TRANSFER_GAS_LIMIT: u64 = 21_000;
/// Conservative ERC-20 transfer gas limit floor.
const ERC20_TRANSFER_GAS_LIMIT_FLOOR: u64 = 60_000;

pub struct EvmFeeEstimator<P> {
    provider: P,
    /// Whether this network supports EIP-1559. Set at construction from
    /// `Network.features.eip1559`. When false, the estimator returns
    /// `EvmFee::Legacy`.
    pub eip1559: bool,
}

impl<P> EvmFeeEstimator<P> {
    pub fn new(provider: P, eip1559: bool) -> Self {
        Self { provider, eip1559 }
    }
}

#[async_trait]
impl<P: Provider + Clone> FeeEstimator for EvmFeeEstimator<P> {
    type Fee = EvmFee;

    async fn estimate_fee(
        &self,
        intent: &TransferIntent,
        _sender: &AddressRef,
    ) -> Result<EvmFee, ChainError> {
        let gas_limit = if intent.asset_instance_id.as_str().contains("/erc20:") {
            ERC20_TRANSFER_GAS_LIMIT_FLOOR
        } else {
            NATIVE_TRANSFER_GAS_LIMIT
        };
        if self.eip1559 {
            match self.eip1559_fee(gas_limit).await {
                Ok(fee) => Ok(fee),
                Err(_) => self.legacy_fee(gas_limit).await,
            }
        } else {
            self.legacy_fee(gas_limit).await
        }
    }
}

impl<P: Provider + Clone> EvmFeeEstimator<P> {
    async fn eip1559_fee(&self, gas_limit: u64) -> Result<EvmFee, ChainError> {
        let fh = self
            .provider
            .get_fee_history(10, BlockNumberOrTag::Latest, &[50.0])
            .await
            .map_err(map_transport_err)?;

        let base_fee = fh.base_fee_per_gas.last().copied().ok_or_else(|| {
            ChainError::FeeEstimationFailed("feeHistory returned empty baseFeePerGas".to_string())
        })?;

        let mut samples: Vec<u128> = fh
            .reward
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| r.first().copied())
            .filter(|&v| v > 0)
            .collect();
        samples.sort_unstable();
        let suggested = samples
            .get(samples.len() / 2)
            .copied()
            .unwrap_or(MIN_PRIORITY_FEE_WEI);

        let max_priority = suggested.clamp(MIN_PRIORITY_FEE_WEI, MAX_PRIORITY_FEE_WEI);
        let max_fee_candidate = base_fee
            .saturating_mul(BASE_FEE_MULTIPLIER)
            .saturating_add(max_priority);
        let max_fee = max_fee_candidate.max(base_fee.saturating_add(max_priority));

        Ok(EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(max_fee),
            max_priority_fee_per_gas: BigInt::from(max_priority),
            gas_limit,
            l1_fee_wei: None,
        })
    }

    async fn legacy_fee(&self, gas_limit: u64) -> Result<EvmFee, ChainError> {
        let gas_price = self
            .provider
            .get_gas_price()
            .await
            .map_err(map_transport_err)?;
        Ok(EvmFee::Legacy {
            gas_price: BigInt::from(gas_price),
            gas_limit,
        })
    }
}
