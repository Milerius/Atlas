//! `EvmFeeEstimator` — fee suggestion delegated to alloy.
//!
//! Atlas previously hand-rolled a `eth_feeHistory`-based EIP-1559 estimator
//! plus min/max priority-fee clamps. alloy 2.x ships the same algorithm in
//! [`Provider::estimate_eip1559_fees`] (modelled after MetaMask's gas-fee
//! controller, which is what most of the Rust EVM ecosystem already uses),
//! so the estimator now wraps the alloy call and contributes only the
//! Atlas-specific bits: the per-asset gas-floor selection and the
//! [`EvmFee`] envelope shape.
//!
//! What remains here:
//!
//! - Gas-limit floor: 21_000 for native sends, 60_000 for ERC-20 (alloy
//!   doesn't have a stance on what gas a wallet should put on a transfer
//!   it hasn't seen the calldata for, and a real estimator is out of
//!   scope for the codec seam).
//! - Legacy-fee fallback via [`Provider::get_gas_price`] when
//!   `Network.features.eip1559 == false` or alloy returns an error.
//! - OP-Stack L1 fee oracle integration is still deferred — the
//!   [`EvmFee::Eip1559::l1_fee_wei`] field stays `None`, matching the
//!   `Provider::estimate_eip1559_fees` contract (which is L2-agnostic).

use alloy_provider::Provider;
use async_trait::async_trait;
use atlas_core::error::ChainError;
use atlas_core::fee::EvmFee;
use atlas_core::id::AddressRef;
use atlas_core::service::FeeEstimator;
use atlas_core::transaction::TransferIntent;
use num_bigint::BigInt;

use crate::error::map_transport_err;

/// Default native-transfer gas limit (21_000 — the EVM intrinsic cost
/// of a value transfer with empty calldata).
const NATIVE_TRANSFER_GAS_LIMIT: u64 = 21_000;
/// Conservative ERC-20 transfer gas limit floor.
///
/// The standard `transfer(address,uint256)` typically uses 45_000–55_000
/// gas; 60_000 leaves headroom for tokens with hooks (storage writes on
/// first-time recipients, fee-on-transfer logic, etc.). Real estimation
/// via `eth_estimateGas` against the actual contract is a follow-up.
const ERC20_TRANSFER_GAS_LIMIT_FLOOR: u64 = 60_000;

pub struct EvmFeeEstimator<P> {
    provider: P,
    /// Whether this network supports EIP-1559. Set at construction from
    /// `Network.features.eip1559`. When false, the estimator returns
    /// [`EvmFee::Legacy`].
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
        let gas_limit = if intent.asset_instance_id.asset_namespace() == "erc20" {
            ERC20_TRANSFER_GAS_LIMIT_FLOOR
        } else {
            NATIVE_TRANSFER_GAS_LIMIT
        };
        if self.eip1559 {
            match self.eip1559_fee(gas_limit).await {
                Ok(fee) => Ok(fee),
                // Fall back to legacy if the alloy estimator can't get a
                // base fee — pre-1559 networks, anvil with empty history,
                // or transient RPC failures all surface here.
                Err(_) => self.legacy_fee(gas_limit).await,
            }
        } else {
            self.legacy_fee(gas_limit).await
        }
    }
}

impl<P: Provider + Clone> EvmFeeEstimator<P> {
    async fn eip1559_fee(&self, gas_limit: u64) -> Result<EvmFee, ChainError> {
        // Delegate to alloy's default estimator. Internally it calls
        // `eth_feeHistory(10, latest, [20.0])`, takes the median of
        // non-zero priority-fee samples, and returns
        // `max_fee_per_gas = base_fee * 2 + max_priority_fee_per_gas`.
        // Same algorithm the MetaMask gas-fee controller uses.
        let est = self
            .provider
            .estimate_eip1559_fees()
            .await
            .map_err(map_transport_err)?;
        Ok(EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(est.max_fee_per_gas),
            max_priority_fee_per_gas: BigInt::from(est.max_priority_fee_per_gas),
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
