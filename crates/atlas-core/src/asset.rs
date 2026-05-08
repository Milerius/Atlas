//! Asset domain model.
//!
//! Atlas separates an asset's *display identity* from its *executable
//! shape*:
//!
//! - [`AssetGroup`] — the user-facing thing ("USDC", "ETH"). Drives
//!   search, pricing, aggregation. **Never signs.**
//! - [`AssetInstrument`] — issuer-level instrument
//!   ("Circle USDC", "native ETH"). Owns metadata (decimals, traits)
//!   that's stable across networks.
//! - [`AssetInstance`] — concrete on-chain instance ("USDC on Base",
//!   "native ETH on Ethereum"). Has a contract / mint, decimals, and
//!   capabilities. **Only this shape signs.**
//!
//! Higher layers may accept a group or instrument; chain services
//! must resolve to an instance before execution.

use crate::{
    error::AssetError,
    id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, NetworkId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// User-facing asset grouping (e.g. "USDC", "ETH"). Drives display,
/// search, pricing, and aggregation. Resolves to a set of
/// [`AssetInstance`]s via [`crate::registry::Registry::asset_instances_for_group`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetGroup {
    /// Stable group identifier (e.g. `usdc`, `eth`).
    pub id: AssetGroupId,
    /// Display ticker shown to users (e.g. `"USDC"`).
    pub symbol: String,
    /// Human-readable name (e.g. `"USD Coin"`).
    pub name: String,
    /// Free-form key/value metadata.
    #[serde(default)]
    pub metadata: AssetMetadata,
}

/// Issuer-level token instrument — the layer between an
/// [`AssetGroup`] and its concrete on-chain
/// [`AssetInstance`]s. Multiple issuers can map to the same group
/// (e.g. Circle USDC vs Bridged USDC.e).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetInstrument {
    /// Stable instrument id (e.g. `usdc.circle`).
    pub id: AssetInstrumentId,
    /// Group this instrument belongs to.
    #[serde(rename = "groupId")]
    pub group_id: AssetGroupId,
    /// Broad asset class (`crypto` for the first scope).
    #[serde(rename = "assetClass")]
    pub asset_class: AssetClass,
    /// What kind of token this represents.
    pub kind: InstrumentKind,
    /// Display ticker (e.g. `"USDC"`).
    pub symbol: String,
    /// Human-readable name (e.g. `"USD Coin"`).
    pub name: String,
    /// Decimal scale of the instrument across networks.
    pub decimals: u8,
    /// Optional issuer label (e.g. `"circle"`).
    #[serde(default)]
    pub issuer: Option<String>,
    /// Behavioral tags (`fungible`, `transferable`, `gas_asset`, …).
    #[serde(default)]
    pub traits: Vec<AssetTrait>,
    /// Free-form key/value metadata.
    #[serde(default)]
    pub metadata: AssetMetadata,
}

/// Concrete on-chain asset instance — the only shape that signs and
/// broadcasts.
///
/// `id` is a CAIP-19-ish path tying the instance to its network
/// (e.g. `eip155:8453/erc20:0x833…2913`). `contract` is required for
/// `Erc20` / `Spl` standards (the contract address / mint pubkey)
/// and forbidden for `Native`; see [`AssetInstance::validate_shape`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetInstance {
    /// Stable on-chain id (e.g. `eip155:8453/native:eth`).
    pub id: AssetInstanceId,
    /// Instrument this instance is a deployment of.
    #[serde(rename = "instrumentId")]
    pub instrument_id: AssetInstrumentId,
    /// Network this instance lives on.
    pub network: NetworkId,
    /// Token standard (`native`, `erc20`, `spl`).
    pub standard: AssetStandard,
    /// Decimal scale of this specific deployment (usually equals the
    /// instrument's decimals, but kept per-instance for clarity).
    pub decimals: u8,
    /// Contract address (`Erc20`) or mint pubkey (`Spl`). `None` for
    /// `Native`.
    #[serde(default)]
    pub contract: Option<String>,
    /// Operations supported by this instance
    /// (`balance`, `transfer`, `approve`, `pay_gas`, `swap`).
    #[serde(default)]
    pub capabilities: Vec<AssetCapability>,
    /// Free-form key/value metadata.
    #[serde(default)]
    pub metadata: AssetMetadata,
}

impl AssetInstance {
    /// Validate the relationship between `standard` and `contract`.
    ///
    /// Rules enforced:
    /// - `Native` must not have a contract.
    /// - `Erc20` must have a non-empty contract address.
    /// - `Spl` must have a non-empty mint pubkey.
    ///
    /// Called by [`crate::registry::Registry::from_documents`] when
    /// constructing the registry, so a registry that exposes any
    /// instance has already passed this check.
    pub fn validate_shape(&self) -> Result<(), AssetError> {
        match (&self.standard, self.contract.as_deref()) {
            (AssetStandard::Native, None) => Ok(()),
            (AssetStandard::Native, Some(_)) => Err(AssetError::InvalidContract(
                "native asset must not have a contract".to_string(),
            )),
            (AssetStandard::Erc20, Some(value)) if !value.trim().is_empty() => Ok(()),
            (AssetStandard::Erc20, _) => Err(AssetError::MissingRequiredIdentifier(
                "erc20 contract".to_string(),
            )),
            (AssetStandard::Spl, Some(value)) if !value.trim().is_empty() => Ok(()),
            (AssetStandard::Spl, _) => Err(AssetError::MissingRequiredIdentifier(
                "spl mint".to_string(),
            )),
        }
    }
}

/// Broad classification of an asset. Atlas's first scope is
/// crypto-native; product layers may grow this enum to cover stocks,
/// fiat balances, etc.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    /// On-chain crypto asset.
    Crypto,
}

/// What kind of on-chain token an instrument represents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    /// Chain-native coin (ETH, SOL, BTC, …).
    NativeCoin,
    /// Fungible token deployed by an issuer (ERC-20, SPL token, …).
    FungibleToken,
}

/// On-chain token standard. Determines the shape rules enforced by
/// [`AssetInstance::validate_shape`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStandard {
    /// Chain-native asset (ETH, SOL, …). Must not carry a contract
    /// field.
    Native,
    /// EVM ERC-20 token. Requires a non-empty contract address.
    Erc20,
    /// Solana SPL token. Requires a non-empty mint pubkey.
    Spl,
}

/// Behavioral tags attached to an [`AssetInstrument`]. Used by
/// higher layers for routing, filtering, and gas-token detection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetTrait {
    /// Fungible (vs. NFT-like, deferred).
    Fungible,
    /// Can be transferred between accounts.
    Transferable,
    /// Used to pay gas / network fees on its native chain.
    GasAsset,
}

/// Operations supported by a concrete [`AssetInstance`]. Lets
/// callers check up front whether an instance is wired up for
/// `transfer` vs `approve` vs `swap` etc., without trying and
/// observing a `ChainError`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetCapability {
    /// Balance lookups are supported.
    Balance,
    /// Plain transfer is supported.
    Transfer,
    /// ERC-20 / SPL approval flow is supported.
    Approve,
    /// Asset can be used to pay gas / network fees.
    PayGas,
    /// Swap routing is supported through some venue.
    Swap,
}

/// Free-form metadata attached to groups, instruments, and instances.
///
/// `BTreeMap<String, String>` for stable JSON ordering across (de)serialize
/// cycles. Higher layers can interpret known keys (e.g. icon URLs,
/// CoinGecko ids) without atlas-core having to model them.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetMetadata {
    /// Key/value pairs flattened into the parent JSON object.
    #[serde(flatten)]
    pub values: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, NetworkId};
    use std::str::FromStr;

    #[test]
    fn erc20_instance_requires_contract() {
        let instance = AssetInstance {
            id: AssetInstanceId::from_str("eip155:8453/erc20:missing").unwrap(),
            instrument_id: AssetInstrumentId::from_str("usdc.circle").unwrap(),
            network: NetworkId::from_str("eip155:8453").unwrap(),
            standard: AssetStandard::Erc20,
            decimals: 6,
            contract: None,
            capabilities: vec![AssetCapability::Balance],
            metadata: AssetMetadata::default(),
        };

        assert_eq!(
            instance.validate_shape().unwrap_err().to_string(),
            "missing required identifier: erc20 contract"
        );
    }

    #[test]
    fn spl_instance_requires_mint() {
        let instance = AssetInstance {
            id: AssetInstanceId::from_str("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/spl:missing")
                .unwrap(),
            instrument_id: AssetInstrumentId::from_str("usdc.circle").unwrap(),
            network: NetworkId::from_str("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").unwrap(),
            standard: AssetStandard::Spl,
            decimals: 6,
            contract: None,
            capabilities: vec![AssetCapability::Balance],
            metadata: AssetMetadata::default(),
        };
        assert_eq!(
            instance.validate_shape().unwrap_err().to_string(),
            "missing required identifier: spl mint"
        );
    }

    #[test]
    fn spl_instance_rejects_whitespace_only_mint() {
        for mint in ["", " ", "\t", " \n "] {
            let instance = AssetInstance {
                id: AssetInstanceId::from_str("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/spl:test")
                    .unwrap(),
                instrument_id: AssetInstrumentId::from_str("test").unwrap(),
                network: NetworkId::from_str("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").unwrap(),
                standard: AssetStandard::Spl,
                decimals: 6,
                contract: Some(mint.to_string()),
                capabilities: vec![AssetCapability::Balance],
                metadata: AssetMetadata::default(),
            };
            assert!(
                instance.validate_shape().is_err(),
                "whitespace-only mint {mint:?} must be rejected",
            );
        }
    }

    #[test]
    fn erc20_instance_rejects_whitespace_only_contract() {
        // The match guard `!value.trim().is_empty()` must hold; cargo-mutants
        // flagged that replacing this with `true` survived our property test
        // (Bolero's String generator rarely produces whitespace-only strings).
        for contract in ["", " ", "   ", "\t", "\n", " \t\n "] {
            let instance = AssetInstance {
                id: AssetInstanceId::from_str("eip155:1/erc20:test").unwrap(),
                instrument_id: AssetInstrumentId::from_str("test").unwrap(),
                network: NetworkId::from_str("eip155:1").unwrap(),
                standard: AssetStandard::Erc20,
                decimals: 6,
                contract: Some(contract.to_string()),
                capabilities: vec![AssetCapability::Balance],
                metadata: AssetMetadata::default(),
            };
            assert!(
                instance.validate_shape().is_err(),
                "whitespace-only contract {contract:?} must be rejected",
            );
        }
    }

    #[test]
    fn native_instance_rejects_contract() {
        let instance = AssetInstance {
            id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            instrument_id: AssetInstrumentId::from_str("eth.native").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            standard: AssetStandard::Native,
            decimals: 18,
            contract: Some("0xabc".to_string()),
            capabilities: vec![AssetCapability::Balance],
            metadata: AssetMetadata::default(),
        };

        assert_eq!(
            instance.validate_shape().unwrap_err().to_string(),
            "invalid contract: native asset must not have a contract"
        );
    }

    #[test]
    fn asset_group_is_display_only() {
        let group = AssetGroup {
            id: AssetGroupId::from_str("usdc").unwrap(),
            symbol: "USDC".to_string(),
            name: "USDC".to_string(),
            metadata: AssetMetadata::default(),
        };

        assert_eq!(group.symbol, "USDC");
    }
}
