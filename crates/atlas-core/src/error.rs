//! Typed error enums for atlas-core.
//!
//! Each public boundary has its own error type so consumers can `match`
//! on the failure mode without parsing strings. No SDK-level path uses
//! `Box<dyn Error>` or `anyhow` — those belong at application leaves.

use crate::asset::AssetStandard;
use crate::id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, ChainId, NetworkId, SignerId};

/// Errors raised by [`crate::registry::Registry`] construction and lookups.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RegistryError {
    /// A network references a chain id that isn't in the chain registry.
    #[error("missing chain: {0}")]
    MissingChain(ChainId),
    /// A network or asset instance references a network id that isn't in
    /// the chain registry, or a lookup asked for a network that wasn't
    /// loaded.
    #[error("missing network: {0}")]
    MissingNetwork(NetworkId),
    /// An instrument references an asset group that isn't in the asset
    /// registry, or a lookup asked for a group that wasn't loaded.
    #[error("missing asset group: {0}")]
    MissingAssetGroup(AssetGroupId),
    /// An asset instance references an instrument that isn't in the asset
    /// registry, or a lookup asked for an instrument that wasn't loaded.
    #[error("missing asset instrument: {0}")]
    MissingAssetInstrument(AssetInstrumentId),
    /// A network references a native asset instance id that isn't in the
    /// asset registry, or a lookup asked for an instance that wasn't
    /// loaded.
    #[error("missing asset instance: {0}")]
    MissingAssetInstance(AssetInstanceId),
    /// Duplicate id, malformed cross-reference, or shape-validation
    /// failure surfaced through the registry. The `message` names the
    /// failing entity and the expected relationship.
    #[error("invalid registry reference: {message}")]
    InvalidReference {
        /// Human-readable description naming the failing id and rule.
        message: String,
    },
    /// The registry document declared a `version` other than
    /// [`crate::registry::LATEST_REGISTRY_VERSION`].
    #[error("unsupported registry version: {version}")]
    UnsupportedVersion {
        /// The unsupported version number found in the input document.
        version: u32,
    },
}

/// Errors raised by asset-shape validation
/// ([`crate::asset::AssetInstance::validate_shape`]) and related checks.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AssetError {
    /// Asset standard is outside the supported set
    /// (`native` / `erc20` / `spl`).
    #[error("unsupported asset standard: {0}")]
    UnsupportedStandard(String),
    /// `contract` field violates the standard's shape rule (e.g. a native
    /// asset has a contract, or an SPL token's mint is empty).
    #[error("invalid contract: {0}")]
    InvalidContract(String),
    /// Decimal scale is outside the supported range for the standard.
    #[error("invalid decimals: {0}")]
    InvalidDecimals(u8),
    /// A required identifier (ERC-20 contract, SPL mint, instrument id,
    /// …) was missing or empty.
    #[error("missing required identifier: {0}")]
    MissingRequiredIdentifier(String),
    /// Caller tried to execute on an `AssetGroup`. Resolve to a concrete
    /// `AssetInstance` first.
    #[error("asset is not executable: {0}")]
    AssetNotExecutable(AssetGroupId),
}

/// Errors raised by [`crate::service::ChainService`] implementations.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ChainError {
    /// The chain family this service was asked to handle isn't supported.
    #[error("unsupported chain: {0}")]
    UnsupportedChain(ChainId),
    /// The network this service was asked to operate on isn't supported.
    #[error("unsupported network: {0}")]
    UnsupportedNetwork(NetworkId),
    /// The asset instance doesn't belong to the target network, or its
    /// CAIP path isn't recognized by the service.
    #[error("unsupported asset instance: {0}")]
    UnsupportedAssetInstance(AssetInstanceId),
    /// `Network.chain_id` was missing, empty, or non-numeric on a network
    /// that requires it (currently EVM).
    #[error("missing chain id on network: {0}")]
    MissingChainId(NetworkId),
    /// The asset's standard is supported by the registry but not by this
    /// chain service (e.g. SPL passed to the EVM service).
    #[error("asset standard {standard:?} not supported for instance {instance}")]
    StandardNotSupported {
        instance: AssetInstanceId,
        standard: AssetStandard,
    },
    /// Recipient address failed format validation for the target chain.
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    /// Gas / fee estimation step failed.
    #[error("fee estimation failed: {0}")]
    FeeEstimationFailed(String),
    /// `prepare_transfer` or `assemble_signed_transaction` could not
    /// produce a valid encoded transaction.
    #[error("transaction build failed: {0}")]
    TransactionBuildFailed(String),
    /// Broadcast step failed for a chain-specific reason
    /// (e.g. nonce too low, gas underpriced, transaction underpriced).
    /// For RPC transport failures (timeout, network unreachable, malformed
    /// response) prefer [`Self::Rpc`] — atlas-evm wraps `alloy` transport
    /// errors there.
    #[error("broadcast failed: {0}")]
    BroadcastFailed(String),
    /// RPC transport / provider error encountered by a chain service.
    #[error("rpc error: {0}")]
    Rpc(#[from] RpcError),
    /// Signer-side failure surfaced through a chain-service orchestrator.
    /// Preserves the typed [`SigningError`] variant so callers can match
    /// on `UserRejected` / `UnsupportedCurve` / `InvalidSignature` etc.
    /// without parsing strings.
    #[error("signing error: {0}")]
    Signing(#[from] SigningError),
}

/// Errors raised by [`crate::signing::SignerProvider`] implementations.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SigningError {
    /// The requested signer id wasn't registered with the provider.
    #[error("signer not found: {0}")]
    SignerNotFound(SignerId),
    /// Signer doesn't support the requested elliptic curve
    /// (e.g. asked to sign with `ed25519` but only secp256k1 is
    /// available).
    #[error("unsupported curve: {0}")]
    UnsupportedCurve(String),
    /// Signer doesn't support the requested payload kind
    /// (e.g. asked for `typed_data` but only `transaction_digest` is
    /// supported).
    #[error("unsupported payload: {0}")]
    UnsupportedPayload(String),
    /// User-mediated signer (hardware wallet, mobile prompt, …) returned
    /// rejection. Callers should treat this as a clean abort, not retry.
    #[error("user rejected signing")]
    UserRejected,
    /// Signing operation failed (HSM error, MPC quorum failure, network
    /// timeout to a remote signer, …).
    #[error("signature failed: {0}")]
    SignatureFailed(String),
    /// The returned signature failed structural validation.
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
}

/// Errors raised by RPC adapters used by future real chain services.
/// atlas-core itself doesn't ship an HTTP client; this enum exists so
/// chain-service crates have a uniform error surface to bubble up.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RpcError {
    /// Transport-level failure (TCP, TLS, DNS, HTTP framing, …).
    #[error("transport error: {0}")]
    Transport(String),
    /// Request exceeded its deadline before a response was received.
    #[error("request timed out")]
    Timeout,
    /// Provider returned a rate-limit response (HTTP 429, RPC quota,
    /// …).
    #[error("rate limited")]
    RateLimited,
    /// Response wasn't valid for the expected schema.
    #[error("malformed response: {0}")]
    MalformedResponse(String),
    /// Node returned an error result (chain-specific RPC error code).
    #[error("node error: {0}")]
    NodeError(String),
}
