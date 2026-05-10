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
//! Concrete implementations (real `EvmChainService`, in-tree
//! `MockEvmChainService`) live in the per-chain crates (e.g. `atlas-evm`).

use crate::{
    amount::RawAmount,
    asset::AssetInstance,
    error::ChainError,
    fee::TransactionStatus,
    id::{AccountRef, AddressRef, NetworkId},
    signing::{SigningRequest, SigningResponse},
    transaction::{BroadcastResult, SignedTransaction, TransferIntent, UnsignedTransaction},
};
use async_trait::async_trait;

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

    /// Send `intent` from `account`, signing with `signer`.
    ///
    /// **v1 `account` contract:** `AccountRef` must carry a chain-native
    /// address string (e.g. an EIP-55 hex address for EVM). Mocks that
    /// use opaque ids like `"account-1"` are accepted by the in-tree
    /// `MockEvmChainService` (in `atlas-evm`) but rejected by real chain
    /// services. A later atlas-account milestone will derive the sender
    /// from the signer's public key; until then, callers are responsible
    /// for pairing matching `account` + `signer` values.
    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn crate::signing::SignerProvider,
    ) -> Result<BroadcastResult, ChainError>;
}
