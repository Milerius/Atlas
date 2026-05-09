//! `EvmChainService` — orchestrator that composes `EvmCodec`, `EvmReader`,
//! `EvmFeeEstimator`, `EvmBroadcaster`, and a `SignerProvider` into the
//! happy-path transfer flow.

use alloy_provider::Provider;
use async_trait::async_trait;
use atlas_core::asset::AssetStandard;
use atlas_core::error::ChainError;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, NetworkId};
use atlas_core::service::{ChainBroadcaster, ChainCodec, ChainReader, ChainService, FeeEstimator};
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::{BroadcastResult, TransferIntent};

use crate::broadcaster::EvmBroadcaster;
use crate::codec::{EvmCodec, EvmPrepareContext};
use crate::fee_estimator::EvmFeeEstimator;
use crate::reader::EvmReader;

pub struct EvmChainService<P> {
    pub codec: EvmCodec,
    pub reader: EvmReader<P>,
    pub fee_estimator: EvmFeeEstimator<P>,
    pub broadcaster: EvmBroadcaster<P>,
    /// Network this orchestrator binds to.
    pub network: NetworkId,
    /// EVM chain id for replay protection.
    pub chain_id: u64,
}

impl<P: Provider + Clone> EvmChainService<P> {
    /// Construct an orchestrator from a provider and network metadata.
    /// Caller is responsible for parsing `Network.chain_id` to a `u64`.
    pub fn new(provider: P, network: NetworkId, chain_id: u64, eip1559: bool) -> Self {
        Self {
            codec: EvmCodec,
            reader: EvmReader::new(provider.clone()),
            fee_estimator: EvmFeeEstimator::new(provider.clone(), eip1559),
            broadcaster: EvmBroadcaster::new(provider),
            network,
            chain_id,
        }
    }
}

#[async_trait]
impl<P: Provider + Clone + Send + Sync + 'static> ChainService for EvmChainService<P> {
    type PrepareContext = EvmPrepareContext;
    type Fee = EvmFee;

    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn SignerProvider,
    ) -> Result<BroadcastResult, ChainError> {
        // Validate that the intent's asset belongs to this orchestrator's network.
        let prefix = format!("{}/", self.network.as_str());
        if !intent.asset_instance_id.as_str().starts_with(&prefix) {
            return Err(ChainError::UnsupportedAssetInstance(
                intent.asset_instance_id,
            ));
        }

        // For v1, the AccountRef holds the EVM address directly. Higher-level
        // account types (atlas-account) come later. The sender address is
        // derived from `account` here without consulting the signer's pubkey.
        let sender = AddressRef::new(account.as_str())
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;

        // Concurrent: fetch nonce + estimate fee.
        let (nonce, fee) = tokio::try_join!(
            self.reader.get_nonce(&self.network, &sender),
            self.fee_estimator.estimate_fee(&intent, &sender),
        )?;

        // Resolve standard + contract from intent.asset_instance_id.
        // Caller is expected to pre-resolve via Registry::asset_instance(...);
        // for v1 we infer from the CAIP path: `/native:` vs `/erc20:`.
        let (standard, contract) = parse_standard_from_instance(&intent)?;

        let unsigned = self.codec.prepare_transfer(EvmPrepareContext {
            account,
            network: self.network.clone(),
            intent,
            chain_id: self.chain_id,
            nonce,
            fee,
            standard,
            contract,
        })?;

        let request = self.codec.signing_request(&unsigned)?;
        let response = signer
            .sign(request)
            .await
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;
        let signed = self.codec.assemble_signed(unsigned, response)?;
        self.broadcaster.broadcast(signed).await
    }
}

fn parse_standard_from_instance(
    intent: &TransferIntent,
) -> Result<(AssetStandard, Option<String>), ChainError> {
    let id = intent.asset_instance_id.as_str();
    if let Some((_, rest)) = id.split_once('/') {
        if let Some(contract) = rest.strip_prefix("erc20:") {
            return Ok((AssetStandard::Erc20, Some(contract.to_string())));
        }
        if rest.starts_with("native:") {
            return Ok((AssetStandard::Native, None));
        }
    }
    Err(ChainError::UnsupportedAssetInstance(
        intent.asset_instance_id.clone(),
    ))
}
