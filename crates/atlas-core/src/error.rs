use crate::id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, ChainId, NetworkId, SignerId};

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RegistryError {
    #[error("missing chain: {0}")]
    MissingChain(ChainId),
    #[error("missing network: {0}")]
    MissingNetwork(NetworkId),
    #[error("missing asset group: {0}")]
    MissingAssetGroup(AssetGroupId),
    #[error("missing asset instrument: {0}")]
    MissingAssetInstrument(AssetInstrumentId),
    #[error("missing asset instance: {0}")]
    MissingAssetInstance(AssetInstanceId),
    #[error("invalid registry reference: {message}")]
    InvalidReference { message: String },
    #[error("unsupported registry version: {version}")]
    UnsupportedVersion { version: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AssetError {
    #[error("unsupported asset standard: {0}")]
    UnsupportedStandard(String),
    #[error("invalid contract: {0}")]
    InvalidContract(String),
    #[error("invalid decimals: {0}")]
    InvalidDecimals(u8),
    #[error("missing required identifier: {0}")]
    MissingRequiredIdentifier(String),
    #[error("asset is not executable: {0}")]
    AssetNotExecutable(String),
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ChainError {
    #[error("unsupported chain: {0}")]
    UnsupportedChain(ChainId),
    #[error("unsupported network: {0}")]
    UnsupportedNetwork(NetworkId),
    #[error("unsupported asset instance: {0}")]
    UnsupportedAssetInstance(AssetInstanceId),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("fee estimation failed: {0}")]
    FeeEstimationFailed(String),
    #[error("transaction build failed: {0}")]
    TransactionBuildFailed(String),
    #[error("broadcast failed: {0}")]
    BroadcastFailed(String),
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SigningError {
    #[error("signer not found: {0}")]
    SignerNotFound(SignerId),
    #[error("unsupported curve: {0}")]
    UnsupportedCurve(String),
    #[error("unsupported payload: {0}")]
    UnsupportedPayload(String),
    #[error("user rejected signing")]
    UserRejected,
    #[error("signature failed: {0}")]
    SignatureFailed(String),
    #[error("invalid signature: {0}")]
    InvalidSignature(String),
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RpcError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("request timed out")]
    Timeout,
    #[error("rate limited")]
    RateLimited,
    #[error("malformed response: {0}")]
    MalformedResponse(String),
    #[error("node error: {0}")]
    NodeError(String),
}
