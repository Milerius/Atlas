//! Per-chain-family transaction lifecycle, split into 5 focused traits.
//!
//! - [`ChainCodec`] — pure: encode / sign-request / assemble. No RPC, no I/O.
//! - [`ChainReader`] — RPC reads: balance, nonce, tx status.
//! - [`FeeEstimator`] — RPC: estimate fee for an intent (per-chain `Fee` type).
//! - [`ChainBroadcaster`] — RPC: send a signed transaction.
//! - [`ChainService`] — orchestrator that composes the four above for the
//!   happy-path transfer flow.
//!
//! The split keeps the pure-encoding seam separate from RPC-dependent seams,
//! so a server can build transactions offline (codec only) and a client can
//! sign with MPC / Privy / local key.
//!
//! [`MockEvmChainService`] is the in-tree smoke implementation — a single
//! struct that implements all 5 traits, so callers get one type and one
//! import rather than five separate mocks.

use crate::{
    amount::RawAmount,
    asset::AssetInstance,
    chain::Curve,
    error::ChainError,
    fee::{EvmFee, TransactionStatus},
    id::{AccountRef, AddressRef, NetworkId},
    signing::{SigningPayloadKind, SigningRequest, SigningResponse},
    transaction::{BroadcastResult, SignedTransaction, TransferIntent, UnsignedTransaction},
};
use async_trait::async_trait;
use num_bigint::BigInt;

// ── ChainCodec ──────────────────────────────────────────────────────────────

/// Pure transaction codec — encode an intent into chain-specific bytes,
/// produce a signing request, and assemble a signed transaction from
/// whichever [`SigningResponse`] shape the signer returned.
///
/// No RPC, no I/O. Implementations are `Send + Sync`.
pub trait ChainCodec: Send + Sync {
    /// Per-chain context needed to encode a transfer.
    /// Different chains require different inputs (EVM: chain_id + nonce + fee;
    /// Solana: recent_blockhash + compute units; UTXO: UTXO selection).
    type PrepareContext;

    /// Encode a [`TransferIntent`] (in `ctx.intent`) into chain-specific
    /// unsigned bytes carried in [`UnsignedTransaction::payload`]. Pure —
    /// no RPC, no I/O.
    fn prepare_transfer(
        &self,
        ctx: Self::PrepareContext,
    ) -> Result<UnsignedTransaction, ChainError>;

    /// Produce the [`SigningRequest`] a [`crate::signing::SignerProvider`]
    /// should sign over. For EVM this is the keccak256 digest with
    /// `payload_kind = TransactionDigest`.
    fn signing_request(&self, unsigned: &UnsignedTransaction)
        -> Result<SigningRequest, ChainError>;

    /// Combine `unsigned` with whichever [`SigningResponse`] shape the
    /// signer returned, producing broadcast-ready bytes in
    /// [`SignedTransaction::raw`]. Some signers
    /// ([`SigningResponse::SubmittedTransaction`]) bypass this step;
    /// codecs that don't support that path return
    /// [`ChainError::TransactionBuildFailed`].
    fn assemble_signed(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError>;
}

// ── ChainReader ─────────────────────────────────────────────────────────────

/// RPC reads: balance, nonce, transaction status. Does not write.
#[async_trait]
pub trait ChainReader: Send + Sync {
    async fn get_balance(
        &self,
        instance: &AssetInstance,
        address: &AddressRef,
    ) -> Result<RawAmount, ChainError>;

    async fn get_nonce(&self, network: &NetworkId, address: &AddressRef)
        -> Result<u64, ChainError>;

    async fn get_transaction_status(
        &self,
        network: &NetworkId,
        hash: &str,
    ) -> Result<TransactionStatus, ChainError>;
}

// ── FeeEstimator ────────────────────────────────────────────────────────────

/// Estimate the per-chain `Fee` for a transfer intent. The associated type
/// allows EVM to return [`crate::fee::EvmFee`] directly without going through
/// the [`crate::fee::Fee`] wrapper.
#[async_trait]
pub trait FeeEstimator: Send + Sync {
    type Fee;

    async fn estimate_fee(
        &self,
        intent: &TransferIntent,
        sender: &AddressRef,
    ) -> Result<Self::Fee, ChainError>;
}

// ── ChainBroadcaster ────────────────────────────────────────────────────────

/// Submit a [`SignedTransaction`] to the network's RPC.
#[async_trait]
pub trait ChainBroadcaster: Send + Sync {
    async fn broadcast(&self, signed: SignedTransaction) -> Result<BroadcastResult, ChainError>;
}

// ── ChainService ────────────────────────────────────────────────────────────

/// Orchestrator that composes [`ChainCodec`] + [`ChainReader`] +
/// [`FeeEstimator`] + [`ChainBroadcaster`] + a [`crate::signing::SignerProvider`]
/// into the happy-path transfer flow.
///
/// Apps that need fine control compose the four sub-traits directly; this
/// trait is for the common case.
#[async_trait]
pub trait ChainService: Send + Sync {
    /// Per-chain context type, mirrored from this orchestrator's
    /// underlying [`ChainCodec::PrepareContext`]. Exposed at the trait
    /// boundary so generic callers can name the chain-specific
    /// `PrepareContext` via `<S as ChainService>::PrepareContext` when
    /// they hold an `S: ChainService`.
    type PrepareContext;
    /// Per-chain fee type, mirrored from this orchestrator's underlying
    /// [`FeeEstimator::Fee`]. Exposed for the same reason as
    /// [`Self::PrepareContext`] — so `<S as ChainService>::Fee` is
    /// reachable from generic code.
    type Fee;

    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn crate::signing::SignerProvider,
    ) -> Result<BroadcastResult, ChainError>;
}

// ── MockEvmChainService ─────────────────────────────────────────────────────

/// In-tree mock that implements all 5 traits. Returns deterministic
/// placeholder bytes and a `0xmock` tx hash so the smoke flow + BDD scenarios
/// can exercise the full pipeline shape without an EVM RLP encoder or RPC
/// dependency.
///
/// Real EVM implementations live in the `atlas-evm` crate.
#[derive(Clone, Debug, Default)]
pub struct MockEvmChainService;

/// Mock context for [`MockEvmChainService::prepare_transfer`]. Mirrors the
/// shape of the future real EVM `PrepareContext` so call sites don't need
/// to change when migrating from mock to real.
#[derive(Clone, Debug)]
pub struct MockEvmPrepareContext {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    pub chain_id: u64,
    pub nonce: u64,
    pub fee: EvmFee,
}

impl ChainCodec for MockEvmChainService {
    type PrepareContext = MockEvmPrepareContext;

    fn prepare_transfer(
        &self,
        ctx: MockEvmPrepareContext,
    ) -> Result<UnsignedTransaction, ChainError> {
        // Mock keeps the existing network-prefix exact-segment rule so this
        // smoke service still catches the bug we have a property test for.
        let expected_prefix = format!("{}/", ctx.network.as_str());
        if !ctx
            .intent
            .asset_instance_id
            .as_str()
            .starts_with(&expected_prefix)
        {
            return Err(ChainError::UnsupportedAssetInstance(
                ctx.intent.asset_instance_id,
            ));
        }
        let _ = (ctx.chain_id, ctx.nonce, ctx.fee); // mock ignores numeric fields
        Ok(UnsignedTransaction {
            account: ctx.account,
            network: ctx.network,
            intent: ctx.intent,
            payload: b"mock-unsigned-evm-transaction".to_vec(),
        })
    }

    fn signing_request(
        &self,
        unsigned: &UnsignedTransaction,
    ) -> Result<SigningRequest, ChainError> {
        Ok(SigningRequest {
            account: unsigned.account.clone(),
            network: unsigned.network.clone(),
            curve: Curve::Secp256k1,
            payload_kind: SigningPayloadKind::TransactionDigest,
            payload: b"mock-digest".to_vec(),
        })
    }

    fn assemble_signed(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError> {
        match response {
            SigningResponse::SignatureOnly { signature, .. } => {
                let mut raw = unsigned.payload;
                raw.extend(signature);
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
                    "mock service expected signed bytes, got submitted hash {tx_hash}"
                )))
            }
        }
    }
}

#[async_trait]
impl ChainReader for MockEvmChainService {
    async fn get_balance(
        &self,
        instance: &AssetInstance,
        _address: &AddressRef,
    ) -> Result<RawAmount, ChainError> {
        // Deterministic mock balance: 1 unit at the instance's decimals.
        let one_unit = BigInt::from(10u64).pow(instance.decimals as u32);
        RawAmount::new(one_unit, instance.decimals)
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))
    }

    async fn get_nonce(
        &self,
        _network: &NetworkId,
        _address: &AddressRef,
    ) -> Result<u64, ChainError> {
        Ok(0)
    }

    async fn get_transaction_status(
        &self,
        _network: &NetworkId,
        hash: &str,
    ) -> Result<TransactionStatus, ChainError> {
        Ok(TransactionStatus::Confirmed {
            hash: hash.to_string(),
            block_number: 1,
            gas_used: BigInt::from(21_000u64),
        })
    }
}

#[async_trait]
impl FeeEstimator for MockEvmChainService {
    type Fee = EvmFee;

    async fn estimate_fee(
        &self,
        _intent: &TransferIntent,
        _sender: &AddressRef,
    ) -> Result<EvmFee, ChainError> {
        Ok(EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(2_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        })
    }
}

#[async_trait]
impl ChainBroadcaster for MockEvmChainService {
    async fn broadcast(&self, signed: SignedTransaction) -> Result<BroadcastResult, ChainError> {
        if signed.raw.is_empty() {
            return Err(ChainError::BroadcastFailed(
                "empty signed transaction".to_string(),
            ));
        }
        Ok(BroadcastResult {
            tx_hash: "0xmock".to_string(),
        })
    }
}

#[async_trait]
impl ChainService for MockEvmChainService {
    type PrepareContext = MockEvmPrepareContext;
    type Fee = EvmFee;

    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn crate::signing::SignerProvider,
    ) -> Result<BroadcastResult, ChainError> {
        // Mock derives a fake sender address; real impls compute from signer pubkey.
        let sender = AddressRef::new("0xmocksender")
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;
        // Resolve the network from the intent (mock convention: prefix before '/').
        let network_str = intent
            .asset_instance_id
            .as_str()
            .split('/')
            .next()
            .ok_or_else(|| {
                ChainError::UnsupportedAssetInstance(intent.asset_instance_id.clone())
            })?;
        let network = NetworkId::new(network_str)
            .map_err(|_| ChainError::UnsupportedAssetInstance(intent.asset_instance_id.clone()))?;

        let nonce = self.get_nonce(&network, &sender).await?;
        let fee = self.estimate_fee(&intent, &sender).await?;

        let unsigned = self.prepare_transfer(MockEvmPrepareContext {
            account,
            network,
            intent,
            chain_id: 1,
            nonce,
            fee,
        })?;

        let request = self.signing_request(&unsigned)?;
        // Typed: `SigningError` propagates through `ChainError::Signing`.
        let response = signer.sign(request).await?;
        let signed = self.assemble_signed(unsigned, response)?;
        self.broadcast(signed).await
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetStandard;
    use crate::id::{AddressRef, AssetInstanceId, NetworkId, SignerId};
    use crate::signing::MockSigner;
    use std::str::FromStr;

    fn intent(instance_id: &str) -> TransferIntent {
        TransferIntent {
            asset_instance_id: AssetInstanceId::new(instance_id).unwrap(),
            to: AddressRef::new("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        }
    }

    fn mock_ctx(network_str: &str, instance_id: &str) -> MockEvmPrepareContext {
        MockEvmPrepareContext {
            account: AccountRef::from_str("account-1").unwrap(),
            network: NetworkId::from_str(network_str).unwrap(),
            intent: intent(instance_id),
            chain_id: 1,
            nonce: 0,
            fee: EvmFee::Legacy {
                gas_price: BigInt::from(1u64),
                gas_limit: 21_000,
            },
        }
    }

    #[tokio::test]
    async fn mock_transfer_returns_0xmock() {
        let svc = MockEvmChainService;
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let result = svc
            .transfer(
                intent("eip155:1/native:eth"),
                AccountRef::from_str("account-1").unwrap(),
                &signer,
            )
            .await
            .unwrap();
        assert_eq!(result.tx_hash, "0xmock");
    }

    #[test]
    fn prepare_transfer_rejects_partial_network_prefix() {
        let svc = MockEvmChainService;
        let ctx = mock_ctx("eip155:1", "eip155:10/native:eth");
        let err = svc.prepare_transfer(ctx).unwrap_err();
        assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
    }

    #[tokio::test]
    async fn broadcast_rejects_empty_raw() {
        let svc = MockEvmChainService;
        let signed = SignedTransaction {
            network: NetworkId::from_str("eip155:1").unwrap(),
            raw: vec![],
        };
        let err = svc.broadcast(signed).await.unwrap_err();
        assert!(matches!(err, ChainError::BroadcastFailed(_)));
    }

    #[test]
    fn assemble_signed_rejects_submitted_variant() {
        let svc = MockEvmChainService;
        let unsigned = UnsignedTransaction {
            account: AccountRef::from_str("account-1").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            intent: intent("eip155:1/native:eth"),
            payload: b"mock".to_vec(),
        };
        let response = SigningResponse::SubmittedTransaction {
            signer: SignerId::from_str("mock").unwrap(),
            tx_hash: "0xabc".to_string(),
        };
        let err = svc.assemble_signed(unsigned, response).unwrap_err();
        assert!(matches!(err, ChainError::TransactionBuildFailed(_)));
    }

    #[tokio::test]
    async fn reader_returns_one_unit_balance() {
        let svc = MockEvmChainService;
        let instance = AssetInstance {
            id: AssetInstanceId::new("eip155:1/native:eth").unwrap(),
            instrument_id: crate::id::AssetInstrumentId::new("eth.native").unwrap(),
            network: NetworkId::new("eip155:1").unwrap(),
            standard: AssetStandard::Native,
            decimals: 18,
            contract: None,
            capabilities: vec![],
            metadata: crate::asset::AssetMetadata::default(),
        };
        let balance = svc
            .get_balance(&instance, &AddressRef::new("0xanyone").unwrap())
            .await
            .unwrap();
        // 10^18 wei = 1 ETH
        assert_eq!(balance.value().to_string(), "1000000000000000000");
        assert_eq!(balance.decimals(), 18);
    }
}
