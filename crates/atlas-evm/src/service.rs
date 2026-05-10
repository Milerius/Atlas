//! `EvmChainService` — orchestrator that composes `EvmCodec`, `EvmReader`,
//! `EvmFeeEstimator`, `EvmBroadcaster`, and a `SignerProvider` into the
//! happy-path transfer flow.

use alloy_provider::Provider;
use async_trait::async_trait;
use atlas_core::asset::AssetStandard;
use atlas_core::chain::AddressFormat;
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
        // Reject intents that don't belong to this orchestrator's network.
        // CAIP-19 already guarantees the asset instance has a CAIP-2 chain
        // segment; here we just compare it against the bound network id.
        if intent.asset_instance_id.network_id() != self.network {
            return Err(ChainError::UnsupportedAssetInstance(
                intent.asset_instance_id,
            ));
        }

        // v1 contract: `account` carries an EVM address. Validate the
        // recipient + sender against the chain's address format before
        // doing any RPC work. EIP-55 mismatched-case input is rejected
        // here, ahead of any nonce / fee fetch.
        intent
            .to
            .validate_for(&AddressFormat::EvmAddress)
            .map_err(|e| ChainError::InvalidAddress(e.to_string()))?;
        let sender = AddressRef::for_format(account.as_str(), AddressFormat::EvmAddress)
            .map_err(|e| ChainError::InvalidAddress(e.to_string()))?;

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

        // `EvmCodec::signing_request` is total: it computes
        // `keccak256(payload)` and returns Ok. The `?` desugar would
        // leave the (unreachable) Err arm uncovered — use `.expect()`
        // so the contract is explicit and there's no dead branch to
        // trip coverage tooling.
        let signing_request = self
            .codec
            .signing_request(&unsigned)
            .expect("EvmCodec::signing_request is infallible");
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
    // `AssetInstanceId` is CAIP-19-validated at construction, so we can
    // pull the `(asset_namespace, asset_reference)` decomposition via
    // typed accessors and only have to switch on the namespace.
    let namespace = intent.asset_instance_id.asset_namespace();
    let reference = intent.asset_instance_id.asset_reference();
    match namespace {
        "erc20" => Ok((AssetStandard::Erc20, Some(reference.to_string()))),
        "native" => Ok((AssetStandard::Native, None)),
        _ => Err(ChainError::UnsupportedAssetInstance(
            intent.asset_instance_id.clone(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::amount::RawAmount;
    use atlas_core::id::{AddressRef, AssetInstanceId};
    use num_bigint::BigInt;
    use std::str::FromStr;

    fn mk_intent(instance_id: &str) -> TransferIntent {
        TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(instance_id).unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        }
    }

    #[test]
    fn parse_standard_resolves_native() {
        let (standard, contract) =
            parse_standard_from_instance(&mk_intent("eip155:1/native:eth")).unwrap();
        assert_eq!(standard, AssetStandard::Native);
        assert_eq!(contract, None);
    }

    #[test]
    fn parse_standard_resolves_erc20_with_contract() {
        let (standard, contract) = parse_standard_from_instance(&mk_intent(
            "eip155:1/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
        ))
        .unwrap();
        assert_eq!(standard, AssetStandard::Erc20);
        assert_eq!(
            contract.as_deref(),
            Some("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")
        );
    }

    // Note: a `parse_standard_rejects_instance_without_slash` test was
    // dropped when CAIP-19 validation landed at the `AssetInstanceId`
    // construction boundary — the type now refuses inputs without a
    // `/` separator before they ever reach this function.

    #[test]
    fn parse_standard_rejects_unknown_segment() {
        let err = parse_standard_from_instance(&mk_intent("eip155:1/spl:something")).unwrap_err();
        assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
    }
}
