//! Transaction lifecycle types for the chain-service boundary.
//!
//! A transfer flows through four shapes:
//! [`TransferIntent`] → [`UnsignedTransaction`] → [`SignedTransaction`]
//! → [`BroadcastResult`]. The first is what callers express, the next two
//! are produced by a `ChainService`, and the last is returned after
//! broadcast.

use crate::{
    amount::RawAmount,
    id::{AccountRef, AddressRef, AssetInstanceId, NetworkId},
    signing::SigningRequest,
};
use serde::{Deserialize, Serialize};

/// User-facing transfer intent: "send `amount` of `asset_instance_id` to
/// `to`". The asset must already be a concrete on-chain instance — Atlas
/// never signs from an `AssetGroup` or `AssetInstrument`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransferIntent {
    /// Concrete on-chain asset to transfer (e.g. `eip155:8453/native:eth`).
    #[serde(rename = "assetInstanceId")]
    pub asset_instance_id: AssetInstanceId,
    /// Recipient address. Format is chain-specific; today the field is a
    /// typed string with no per-chain validation (deferred work).
    pub to: AddressRef,
    /// Raw base-unit amount (wei, satoshi, token units, lamports, …) plus
    /// the instrument's decimal scale.
    pub amount: RawAmount,
}

/// Output of `ChainService::prepare_transfer`: the intent enriched with
/// the account and network it will execute on, plus the chain-specific
/// `payload` bytes the signer must sign over.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnsignedTransaction {
    /// Identifier of the executing account (matches what the signer expects).
    pub account: AccountRef,
    /// Network the transaction is bound to.
    pub network: NetworkId,
    /// The original intent, preserved for assembly and audit.
    pub intent: TransferIntent,
    /// Chain-specific encoded payload (RLP for EVM, message bytes for
    /// Solana, …). Opaque to atlas-core.
    pub payload: Vec<u8>,
}

/// A signed transaction ready to broadcast. The `raw` bytes are
/// chain-specific and consumed verbatim by the broadcast step.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedTransaction {
    /// Network the signed bytes belong to.
    pub network: NetworkId,
    /// Chain-specific signed bytes (encoded EVM tx, signed Solana
    /// transaction, …).
    pub raw: Vec<u8>,
}

/// Result of a successful broadcast — the transaction hash returned by
/// the network's RPC. Callers can use this for receipts and explorer
/// links.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BroadcastResult {
    /// Network-specific transaction hash (`0x…` for EVM, base58 signature
    /// for Solana).
    #[serde(rename = "txHash")]
    pub tx_hash: String,
}

/// Wire-format pair of "what to sign" + "what to assemble against",
/// emitted by a server-side build step and consumed by a client-side
/// signer.
///
/// Atlas's chain services split the transfer flow into a pure codec
/// seam (`prepare_transfer` / `signing_request` / `assemble_signed`)
/// and an RPC seam (reader / fee estimator / broadcaster). A common
/// integration shape is:
///
/// 1. The **server** holds the codec + reader + fee estimator. It has
///    no key material. It calls a `prepare_unsigned_bundle` on the
///    chain service and serialises the resulting [`UnsignedBundle`]
///    over the wire.
/// 2. The **client** holds a `SignerProvider` (local key, MPC, Privy,
///    hardware wallet, …) and a broadcaster. It deserialises the
///    bundle, signs `signing_request`, and calls
///    `assemble_and_broadcast` on its own chain service.
///
/// Both halves of the bundle are independently `Serialize` /
/// `Deserialize`, but shipping them together as a single JSON object
/// keeps the wire format obvious and lets the server pre-compute the
/// signing digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnsignedBundle {
    /// Encoded unsigned transaction. Opaque to the client beyond the
    /// fact that it must be passed back into `assemble_signed` along
    /// with whatever the signer returned.
    pub unsigned: UnsignedTransaction,
    /// Pre-computed signing request — the chain-specific digest /
    /// payload kind / curve the signer is expected to consume. Saves
    /// the client from re-deriving it from `unsigned.payload`.
    #[serde(rename = "signingRequest")]
    pub signing_request: SigningRequest,
}
