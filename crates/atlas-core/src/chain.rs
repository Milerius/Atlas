//! Chain and network domain model.
//!
//! Atlas distinguishes:
//!
//! - [`Chain`] — chain *family* / execution model (EVM, Solana, …).
//!   Slow-moving; defines address format, signing curve, and the set
//!   of token standards that family supports.
//! - [`Network`] — concrete deployed environment within a family
//!   (Ethereum mainnet, Base, Solana mainnet, …). References its
//!   parent [`Chain`] and the native asset instance for that network.
//!
//! Splitting the two keeps registries clean: adding a new EVM L2
//! network doesn't require duplicating EVM-family fields.

use crate::id::{AssetInstanceId, ChainId, NetworkId};
use serde::{Deserialize, Serialize};

/// A chain family — execution model and adapter target. Concrete
/// networks belong to a chain via [`Network::chain`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    /// Stable chain id (e.g. `evm`, `solana`).
    pub id: ChainId,
    /// Human-readable name (e.g. `"EVM"`, `"Solana"`).
    pub name: String,
    /// Account model used by this chain family.
    pub family: ChainFamily,
    /// On-chain address format used across this family's networks.
    #[serde(rename = "addressFormat")]
    pub address_format: AddressFormat,
    /// Curve the family's default signer uses.
    #[serde(rename = "defaultCurve")]
    pub default_curve: Curve,
    /// Token standards this chain family supports
    /// (`["native", "erc20"]` for EVM, `["native", "spl"]` for Solana,
    /// …). Strings, not [`crate::asset::AssetStandard`], to keep the
    /// schema forward-compatible.
    #[serde(rename = "supportedStandards")]
    pub supported_standards: Vec<String>,
    /// Top-level chain capabilities (balance, transfer, approve,
    /// contract-call, broadcast). Asset-level capabilities live on
    /// [`crate::asset::AssetInstance::capabilities`].
    #[serde(default)]
    pub capabilities: Vec<ChainCapability>,
}

/// A concrete deployed network within a [`Chain`] family.
///
/// `id` is in CAIP-2 form (`eip155:1`, `solana:5eykt4Us…vdp`).
/// `chain_id` is an EVM-specific decimal string (e.g. `"1"`, `"8453"`)
/// kept on the parent [`Network`] for convenience; non-EVM networks
/// leave it `None`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Network {
    /// CAIP-2 network id.
    pub id: NetworkId,
    /// Optional short alias (`ethereum`, `base`, `solana-mainnet`).
    #[serde(default)]
    pub alias: Option<String>,
    /// EVM-specific decimal chain id as a string. `None` for non-EVM
    /// networks; see `registries/README.md` for the schema note.
    #[serde(default, rename = "chainId")]
    pub chain_id: Option<String>,
    /// Reference to the parent [`Chain`] family.
    pub chain: ChainId,
    /// Display name (`"Ethereum"`, `"Base"`, `"Solana Mainnet"`).
    pub name: String,
    /// Mainnet / testnet / devnet classification.
    pub environment: NetworkEnvironment,
    /// Reference to the native asset instance on this network
    /// (e.g. `eip155:1/native:eth`). Validated by
    /// [`crate::registry::Registry::from_documents`].
    #[serde(rename = "nativeAssetInstanceId")]
    pub native_asset_instance_id: AssetInstanceId,
    /// RPC endpoint configuration.
    pub rpc: RpcConfig,
    /// Block explorers for this network.
    #[serde(default)]
    pub explorers: Vec<Explorer>,
    /// Per-network feature flags.
    #[serde(default)]
    pub features: NetworkFeatures,
}

/// Account model used by a [`Chain`] family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainFamily {
    /// Account-based ledger (EVM, Solana, Cosmos, …).
    AccountBased,
    /// UTXO-based ledger (Bitcoin, Litecoin, …). Deferred.
    Utxo,
    /// Object-based ledger (Sui, Move-based chains). Deferred.
    ObjectBased,
}

/// On-chain address format used across a [`Chain`] family's networks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressFormat {
    /// 20-byte hex address with optional EIP-55 checksum (EVM).
    EvmAddress,
    /// 32-byte ed25519 public key, base58-encoded (Solana).
    SolanaPubkey,
}

/// Elliptic curve used by a chain family's default signer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    /// secp256k1 (EVM, Bitcoin).
    Secp256k1,
    /// Ed25519 (Solana, Sui, Aptos, …).
    Ed25519,
}

/// Top-level operations a chain family supports. Asset-level
/// capabilities live on [`crate::asset::AssetCapability`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainCapability {
    /// Account balance lookups.
    Balance,
    /// Native and token transfers.
    Transfer,
    /// ERC-20 / SPL approval flows.
    Approve,
    /// Arbitrary contract / program invocations.
    ContractCall,
    /// Submitting signed transactions to the network.
    Broadcast,
}

/// Network environment classification. Influences fee defaults, faucet
/// availability, and RPC selection in higher layers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkEnvironment {
    /// Production network with real value at stake.
    Mainnet,
    /// Public test network (Sepolia, Holesky, Solana Devnet, …).
    Testnet,
    /// Local / development network (anvil, solana-test-validator, …).
    Devnet,
}

/// RPC endpoint configuration for a [`Network`].
///
/// atlas-core does not invoke this URL — there's no HTTP client in the
/// crate today. Real chain services should override `default_url` with
/// the consumer's provider URL (Alchemy, Infura, Helius, …).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RpcConfig {
    /// Provider URL used as a starting point. Treat as a placeholder.
    #[serde(rename = "defaultUrl")]
    pub default_url: String,
}

/// Block explorer reference for displaying transaction / address
/// links.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Explorer {
    /// Display name (`"Etherscan"`, `"Basescan"`, …).
    pub name: String,
    /// URL template for transaction links. Contains the literal
    /// `{tx}` placeholder.
    pub tx: String,
    /// URL template for address links. Contains the literal
    /// `{address}` placeholder.
    pub address: String,
}

/// Per-network feature flags. Drive fee-model selection in higher
/// layers (e.g. real EVM `ChainService` switches between legacy and
/// EIP-1559 based on `eip1559`).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NetworkFeatures {
    /// Network supports EIP-1559 dynamic fee transactions.
    #[serde(default)]
    pub eip1559: bool,
    /// Network supports the ERC-20 token standard.
    #[serde(default)]
    pub erc20: bool,
    /// Network is an OP-Stack rollup that charges an L1 data fee in
    /// addition to the L2 execution fee.
    #[serde(default, rename = "opStackL1Fee")]
    pub op_stack_l1_fee: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AssetInstanceId, ChainId, NetworkId};
    use std::str::FromStr;

    #[test]
    fn network_references_chain_and_native_asset() {
        let network = Network {
            id: NetworkId::from_str("eip155:8453").unwrap(),
            alias: Some("base".to_string()),
            chain_id: Some("8453".to_string()),
            chain: ChainId::from_str("evm").unwrap(),
            name: "Base".to_string(),
            environment: NetworkEnvironment::Mainnet,
            native_asset_instance_id: AssetInstanceId::from_str("eip155:8453/native:eth").unwrap(),
            rpc: RpcConfig {
                default_url: "https://mainnet.base.org".to_string(),
            },
            explorers: vec![],
            features: NetworkFeatures::default(),
        };

        assert_eq!(network.chain.as_str(), "evm");
    }
}
