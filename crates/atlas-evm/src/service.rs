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
use atlas_core::signing::{SignerProvider, SigningResponse};
use atlas_core::transaction::{
    BroadcastResult, TransferIntent, UnsignedBundle, UnsignedTransaction,
};

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

    /// Server-side build step: validate the intent, fetch nonce + fee,
    /// run the codec, and return the [`UnsignedBundle`] (encoded
    /// unsigned bytes plus the pre-computed signing request).
    ///
    /// **No signer required.** A backend that holds only RPC credentials
    /// and zero key material can call this, serialise the result, and
    /// hand it to a client (browser, mobile, hardware wallet) for the
    /// signing step.
    ///
    /// The corresponding client step is [`Self::assemble_and_broadcast`].
    pub async fn prepare_unsigned_bundle(
        &self,
        intent: TransferIntent,
        account: AccountRef,
    ) -> Result<UnsignedBundle, ChainError>
    where
        P: Send + Sync + 'static,
    {
        let prefix = format!("{}/", self.network.as_str());
        if !intent.asset_instance_id.as_str().starts_with(&prefix) {
            return Err(ChainError::UnsupportedAssetInstance(
                intent.asset_instance_id,
            ));
        }

        let sender = AddressRef::new(account.as_str())
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;

        let (nonce, fee) = tokio::try_join!(
            self.reader.get_nonce(&self.network, &sender),
            self.fee_estimator.estimate_fee(&intent, &sender),
        )?;

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

        let signing_request = self.codec.signing_request(&unsigned)?;
        Ok(UnsignedBundle {
            unsigned,
            signing_request,
        })
    }

    /// Client-side finalize step: assemble the signed envelope from
    /// `unsigned` + whatever the signer returned, then broadcast.
    ///
    /// This is the counterpart to [`Self::prepare_unsigned_bundle`]:
    /// the unsigned bytes typically arrived over the wire from a
    /// backend that has no key material, and `response` came from a
    /// signer the client trusts (local key, hardware wallet, MPC).
    pub async fn assemble_and_broadcast(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<BroadcastResult, ChainError>
    where
        P: Send + Sync + 'static,
    {
        let signed = self.codec.assemble_signed(unsigned, response)?;
        self.broadcaster.broadcast(signed).await
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
        // In-process flow = server build + client sign + broadcast,
        // collapsed onto one machine. Any change to the build or
        // assemble step must keep both halves consistent.
        let bundle = self.prepare_unsigned_bundle(intent, account).await?;
        // Typed: `SigningError` propagates through `ChainError::Signing`
        // (the `#[from]` impl). Callers that need to distinguish, e.g.,
        // `UserRejected` can match on the inner variant.
        let response = signer.sign(bundle.signing_request).await?;
        self.assemble_and_broadcast(bundle.unsigned, response).await
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
