//! Split chain + asset registry, with cross-reference validation.
//!
//! Atlas's registry is two documents — [`ChainRegistryDocument`] (slow-
//! moving chain data) and [`AssetRegistryDocument`] (fast-moving asset
//! data) — combined into a [`Registry`] that exposes typed lookups.
//!
//! Construction goes through [`Registry::from_documents`] which
//! validates the combined registry before exposing it: unknown
//! versions, duplicate ids, dangling cross-references, and instances
//! whose `(standard, contract)` shape is invalid are all rejected.
//! Once you have a [`Registry`], every lookup either returns the
//! requested entity or a typed [`RegistryError`].

use crate::{
    asset::{AssetGroup, AssetInstance, AssetInstrument},
    chain::{Chain, Network},
    error::RegistryError,
    id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, ChainId, NetworkId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Highest registry-document version atlas-core understands. Documents
/// at any other version are rejected with
/// [`RegistryError::UnsupportedVersion`] — forward compatibility is
/// opt-in, never silent.
pub const LATEST_REGISTRY_VERSION: u32 = 1;

/// Wire format for the slow-moving chain side of the registry —
/// chains, networks, RPC defaults, and native asset references.
///
/// Deserialized from JSON / YAML / TOML / etc. via serde, then handed
/// to [`Registry::from_documents`] alongside an
/// [`AssetRegistryDocument`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainRegistryDocument {
    /// Document schema version. Must equal
    /// [`LATEST_REGISTRY_VERSION`].
    pub version: u32,
    /// Chain families (`evm`, `solana`, …).
    pub chains: Vec<Chain>,
    /// Concrete networks belonging to those chains.
    pub networks: Vec<Network>,
}

/// Wire format for the fast-moving asset side of the registry —
/// groups, instruments, instances, contracts, decimals, capabilities.
///
/// Deserialized from JSON / YAML / TOML / etc. via serde, then handed
/// to [`Registry::from_documents`] alongside a
/// [`ChainRegistryDocument`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetRegistryDocument {
    /// Document schema version. Must equal
    /// [`LATEST_REGISTRY_VERSION`].
    pub version: u32,
    /// Display-level groupings (e.g. `usdc`, `eth`).
    #[serde(rename = "assetGroups")]
    pub asset_groups: Vec<AssetGroup>,
    /// Issuer-level instruments (e.g. `usdc.circle`).
    #[serde(rename = "assetInstruments")]
    pub asset_instruments: Vec<AssetInstrument>,
    /// Concrete on-chain instances (e.g. `eip155:8453/native:eth`).
    #[serde(rename = "assetInstances")]
    pub asset_instances: Vec<AssetInstance>,
}

/// Validated, in-memory registry with typed lookups and group
/// resolution.
///
/// Build via [`Registry::from_documents`]. Internal storage is private
/// `BTreeMap`s keyed by string id; lookups return references to the
/// owned entries.
#[derive(Clone, Debug)]
pub struct Registry {
    chains: BTreeMap<String, Chain>,
    networks: BTreeMap<String, Network>,
    asset_groups: BTreeMap<String, AssetGroup>,
    asset_instruments: BTreeMap<String, AssetInstrument>,
    asset_instances: BTreeMap<String, AssetInstance>,
}

impl Registry {
    /// Build a [`Registry`] from a chain document and an asset
    /// document, running every validation rule before returning a
    /// reference to a usable registry.
    ///
    /// Failure modes (each returns a typed [`RegistryError`]):
    ///
    /// - Either document declares an unknown `version`.
    /// - Any of the five collections (chains, networks, groups,
    ///   instruments, instances) contains a duplicate id.
    /// - A network references a chain that isn't in the chain doc.
    /// - An instrument references a group that isn't in the asset
    ///   doc.
    /// - An asset instance references a missing network or
    ///   instrument.
    /// - An instance's `(standard, contract)` shape fails
    ///   [`AssetInstance::validate_shape`].
    /// - A network's `native_asset_instance_id` doesn't exist, or the
    ///   referenced instance belongs to a different network.
    pub fn from_documents(
        chain_doc: ChainRegistryDocument,
        asset_doc: AssetRegistryDocument,
    ) -> Result<Self, RegistryError> {
        reject_unknown_version(chain_doc.version)?;
        reject_unknown_version(asset_doc.version)?;

        let chains = collect_unique(chain_doc.chains, |c| c.id.to_string(), "chain")?;
        let networks = collect_unique(chain_doc.networks, |n| n.id.to_string(), "network")?;
        let asset_groups =
            collect_unique(asset_doc.asset_groups, |g| g.id.to_string(), "asset group")?;
        let asset_instruments = collect_unique(
            asset_doc.asset_instruments,
            |i| i.id.to_string(),
            "asset instrument",
        )?;
        let asset_instances = collect_unique(
            asset_doc.asset_instances,
            |i| i.id.to_string(),
            "asset instance",
        )?;

        let registry = Self {
            chains,
            networks,
            asset_groups,
            asset_instruments,
            asset_instances,
        };
        registry.validate_references()?;
        Ok(registry)
    }

    /// Look up a network by [`NetworkId`] string. Returns
    /// [`RegistryError::MissingNetwork`] when the id isn't registered,
    /// or [`RegistryError::InvalidReference`] when the lookup string
    /// is empty.
    pub fn network(&self, id: &str) -> Result<&Network, RegistryError> {
        self.networks.get(id).ok_or_else(|| missing_network(id))
    }

    /// Look up a chain family by [`ChainId`]. Returns
    /// [`RegistryError::MissingChain`] when the id isn't registered.
    ///
    /// Useful for reading family-level metadata such as the canonical
    /// HD derivation path ([`Chain::default_derivation_path`]) before
    /// initializing a signer.
    pub fn chain(&self, id: &ChainId) -> Result<&Chain, RegistryError> {
        self.chains
            .get(id.as_str())
            .ok_or_else(|| RegistryError::MissingChain(id.clone()))
    }

    /// Look up a display-level asset group by [`AssetGroupId`] string.
    pub fn asset_group(&self, id: &str) -> Result<&AssetGroup, RegistryError> {
        self.asset_groups
            .get(id)
            .ok_or_else(|| missing_asset_group(id))
    }

    /// Look up an issuer-level asset instrument by
    /// [`AssetInstrumentId`] string.
    pub fn asset_instrument(&self, id: &str) -> Result<&AssetInstrument, RegistryError> {
        self.asset_instruments
            .get(id)
            .ok_or_else(|| missing_asset_instrument(id))
    }

    /// Look up a concrete on-chain asset instance by
    /// [`AssetInstanceId`] string. This is the only lookup that
    /// returns an executable shape.
    pub fn asset_instance(&self, id: &str) -> Result<&AssetInstance, RegistryError> {
        self.asset_instances
            .get(id)
            .ok_or_else(|| missing_asset_instance(id))
    }

    /// Resolve a display group to all concrete on-chain instances that
    /// belong to it.
    ///
    /// Walks instruments under the given group and collects every
    /// instance pointing at one of those instruments. The returned
    /// slice is sorted by instance id for deterministic iteration.
    /// Returns [`RegistryError::MissingAssetGroup`] if the group id
    /// isn't registered.
    pub fn asset_instances_for_group(
        &self,
        group_id: &str,
    ) -> Result<Vec<&AssetInstance>, RegistryError> {
        self.asset_group(group_id)?;
        let instrument_ids = self
            .asset_instruments
            .values()
            .filter(|instrument| instrument.group_id.as_str() == group_id)
            .map(|instrument| instrument.id.as_str())
            .collect::<Vec<_>>();
        let mut instances = self
            .asset_instances
            .values()
            .filter(|instance| instrument_ids.contains(&instance.instrument_id.as_str()))
            .collect::<Vec<_>>();
        instances.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        Ok(instances)
    }

    fn validate_references(&self) -> Result<(), RegistryError> {
        for network in self.networks.values() {
            if !self.chains.contains_key(network.chain.as_str()) {
                return Err(RegistryError::MissingChain(network.chain.clone()));
            }
        }

        for instrument in self.asset_instruments.values() {
            if !self.asset_groups.contains_key(instrument.group_id.as_str()) {
                return Err(RegistryError::MissingAssetGroup(
                    instrument.group_id.clone(),
                ));
            }
        }

        for instance in self.asset_instances.values() {
            if !self.networks.contains_key(instance.network.as_str()) {
                return Err(RegistryError::MissingNetwork(instance.network.clone()));
            }
            if !self
                .asset_instruments
                .contains_key(instance.instrument_id.as_str())
            {
                return Err(RegistryError::MissingAssetInstrument(
                    instance.instrument_id.clone(),
                ));
            }
            if let Err(err) = instance.validate_shape() {
                return Err(RegistryError::InvalidReference {
                    message: format!("asset instance {} shape invalid: {err}", instance.id),
                });
            }
        }

        for network in self.networks.values() {
            let native = self
                .asset_instances
                .get(network.native_asset_instance_id.as_str());
            match native {
                Some(instance) if instance.network == network.id => {}
                Some(_) => {
                    return Err(RegistryError::InvalidReference {
                        message: format!(
                            "network {} native asset {} belongs to another network",
                            network.id, network.native_asset_instance_id
                        ),
                    });
                }
                None => {
                    return Err(RegistryError::MissingAssetInstance(
                        network.native_asset_instance_id.clone(),
                    ))
                }
            }
        }

        Ok(())
    }
}

fn collect_unique<T, F>(
    items: Vec<T>,
    key_fn: F,
    kind: &'static str,
) -> Result<BTreeMap<String, T>, RegistryError>
where
    F: Fn(&T) -> String,
{
    let mut map = BTreeMap::new();
    for item in items {
        let key = key_fn(&item);
        if map.insert(key.clone(), item).is_some() {
            return Err(RegistryError::InvalidReference {
                message: format!("duplicate {kind} id: {key}"),
            });
        }
    }
    Ok(map)
}

fn reject_unknown_version(version: u32) -> Result<(), RegistryError> {
    if version == LATEST_REGISTRY_VERSION {
        Ok(())
    } else {
        Err(RegistryError::UnsupportedVersion { version })
    }
}

fn missing_network(id: &str) -> RegistryError {
    NetworkId::new(id)
        .map(RegistryError::MissingNetwork)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "network lookup id must not be empty".to_string(),
        })
}

fn missing_asset_group(id: &str) -> RegistryError {
    AssetGroupId::new(id)
        .map(RegistryError::MissingAssetGroup)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "asset group lookup id must not be empty".to_string(),
        })
}

fn missing_asset_instrument(id: &str) -> RegistryError {
    AssetInstrumentId::new(id)
        .map(RegistryError::MissingAssetInstrument)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "asset instrument lookup id must not be empty".to_string(),
        })
}

fn missing_asset_instance(id: &str) -> RegistryError {
    AssetInstanceId::new(id)
        .map(RegistryError::MissingAssetInstance)
        .unwrap_or_else(|_| RegistryError::InvalidReference {
            message: "asset instance lookup id must not be empty".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        asset::{
            AssetCapability, AssetClass, AssetMetadata, AssetStandard, AssetTrait, InstrumentKind,
        },
        chain::{
            AddressFormat, ChainCapability, ChainFamily, Curve, NetworkEnvironment,
            NetworkFeatures, RpcConfig,
        },
        id::ChainId,
    };
    use std::str::FromStr;

    fn evm_chain() -> Chain {
        Chain {
            id: ChainId::from_str("evm").unwrap(),
            name: "EVM".to_string(),
            family: ChainFamily::AccountBased,
            address_format: AddressFormat::EvmAddress,
            default_curve: Curve::Secp256k1,
            supported_standards: vec!["native".to_string(), "erc20".to_string()],
            capabilities: vec![ChainCapability::Transfer],
            default_derivation_path: Some("m/44'/60'/0'/0/0".to_string()),
        }
    }

    fn ethereum_network() -> Network {
        Network {
            id: NetworkId::from_str("eip155:1").unwrap(),
            alias: Some("ethereum".to_string()),
            chain_id: Some("1".to_string()),
            chain: ChainId::from_str("evm").unwrap(),
            name: "Ethereum".to_string(),
            environment: NetworkEnvironment::Mainnet,
            native_asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            rpc: RpcConfig {
                default_url: "https://rpc.example.com".to_string(),
            },
            explorers: vec![],
            features: NetworkFeatures::default(),
        }
    }

    fn eth_group() -> AssetGroup {
        AssetGroup {
            id: AssetGroupId::from_str("eth").unwrap(),
            symbol: "ETH".to_string(),
            name: "Ether".to_string(),
            metadata: AssetMetadata::default(),
        }
    }

    fn eth_native_instrument() -> AssetInstrument {
        AssetInstrument {
            id: AssetInstrumentId::from_str("eth.native").unwrap(),
            group_id: AssetGroupId::from_str("eth").unwrap(),
            asset_class: AssetClass::Crypto,
            kind: InstrumentKind::NativeCoin,
            symbol: "ETH".to_string(),
            name: "Ether".to_string(),
            decimals: 18,
            issuer: None,
            traits: vec![AssetTrait::Fungible, AssetTrait::GasAsset],
            metadata: AssetMetadata::default(),
        }
    }

    fn ethereum_native_eth() -> AssetInstance {
        AssetInstance {
            id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            instrument_id: AssetInstrumentId::from_str("eth.native").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            standard: AssetStandard::Native,
            decimals: 18,
            contract: None,
            capabilities: vec![AssetCapability::Balance, AssetCapability::Transfer],
            metadata: AssetMetadata::default(),
        }
    }

    fn valid_chain_doc() -> ChainRegistryDocument {
        ChainRegistryDocument {
            version: LATEST_REGISTRY_VERSION,
            chains: vec![evm_chain()],
            networks: vec![ethereum_network()],
        }
    }

    fn valid_asset_doc() -> AssetRegistryDocument {
        AssetRegistryDocument {
            version: LATEST_REGISTRY_VERSION,
            asset_groups: vec![eth_group()],
            asset_instruments: vec![eth_native_instrument()],
            asset_instances: vec![ethereum_native_eth()],
        }
    }

    fn valid_registry() -> Registry {
        Registry::from_documents(valid_chain_doc(), valid_asset_doc()).unwrap()
    }

    #[test]
    fn rejects_unknown_chain_registry_version() {
        let mut chain_doc = valid_chain_doc();
        chain_doc.version = 99;
        let err = Registry::from_documents(chain_doc, valid_asset_doc()).unwrap_err();
        assert_eq!(err, RegistryError::UnsupportedVersion { version: 99 });
    }

    #[test]
    fn rejects_unknown_asset_registry_version() {
        let mut asset_doc = valid_asset_doc();
        asset_doc.version = 42;
        let err = Registry::from_documents(valid_chain_doc(), asset_doc).unwrap_err();
        assert_eq!(err, RegistryError::UnsupportedVersion { version: 42 });
    }

    #[test]
    fn rejects_duplicate_chain_id() {
        let mut chain_doc = valid_chain_doc();
        chain_doc.chains.push(evm_chain());
        let err = Registry::from_documents(chain_doc, valid_asset_doc()).unwrap_err();
        assert!(matches!(
            err,
            RegistryError::InvalidReference { ref message } if message == "duplicate chain id: evm"
        ));
    }

    #[test]
    fn rejects_duplicate_network_id() {
        let mut chain_doc = valid_chain_doc();
        chain_doc.networks.push(ethereum_network());
        let err = Registry::from_documents(chain_doc, valid_asset_doc()).unwrap_err();
        assert!(matches!(
            err,
            RegistryError::InvalidReference { ref message } if message == "duplicate network id: eip155:1"
        ));
    }

    #[test]
    fn rejects_duplicate_asset_group_id() {
        let mut asset_doc = valid_asset_doc();
        asset_doc.asset_groups.push(eth_group());
        let err = Registry::from_documents(valid_chain_doc(), asset_doc).unwrap_err();
        assert!(matches!(
            err,
            RegistryError::InvalidReference { ref message } if message == "duplicate asset group id: eth"
        ));
    }

    #[test]
    fn rejects_duplicate_asset_instrument_id() {
        let mut asset_doc = valid_asset_doc();
        asset_doc.asset_instruments.push(eth_native_instrument());
        let err = Registry::from_documents(valid_chain_doc(), asset_doc).unwrap_err();
        assert!(matches!(
            err,
            RegistryError::InvalidReference { ref message } if message == "duplicate asset instrument id: eth.native"
        ));
    }

    #[test]
    fn rejects_duplicate_asset_instance_id() {
        let mut asset_doc = valid_asset_doc();
        asset_doc.asset_instances.push(ethereum_native_eth());
        let err = Registry::from_documents(valid_chain_doc(), asset_doc).unwrap_err();
        assert!(matches!(
            err,
            RegistryError::InvalidReference { ref message } if message == "duplicate asset instance id: eip155:1/native:eth"
        ));
    }

    #[test]
    fn rejects_network_referencing_missing_chain() {
        let mut chain_doc = valid_chain_doc();
        chain_doc.networks[0].chain = ChainId::from_str("solana").unwrap();
        let err = Registry::from_documents(chain_doc, valid_asset_doc()).unwrap_err();
        assert!(matches!(err, RegistryError::MissingChain(id) if id.as_str() == "solana"));
    }

    #[test]
    fn rejects_instrument_referencing_missing_group() {
        let mut asset_doc = valid_asset_doc();
        asset_doc.asset_instruments[0].group_id = AssetGroupId::from_str("ghost").unwrap();
        let err = Registry::from_documents(valid_chain_doc(), asset_doc).unwrap_err();
        assert!(matches!(err, RegistryError::MissingAssetGroup(id) if id.as_str() == "ghost"));
    }

    #[test]
    fn rejects_instance_referencing_missing_instrument() {
        let mut asset_doc = valid_asset_doc();
        asset_doc.asset_instances[0].instrument_id =
            AssetInstrumentId::from_str("ghost.token").unwrap();
        let err = Registry::from_documents(valid_chain_doc(), asset_doc).unwrap_err();
        assert!(
            matches!(err, RegistryError::MissingAssetInstrument(id) if id.as_str() == "ghost.token")
        );
    }

    #[test]
    fn rejects_native_instance_belonging_to_other_network() {
        // Add a second network whose native_asset_instance_id points at an
        // instance belonging to the first network.
        let mut chain_doc = valid_chain_doc();
        chain_doc.networks.push(Network {
            id: NetworkId::from_str("eip155:8453").unwrap(),
            alias: None,
            chain_id: None,
            chain: ChainId::from_str("evm").unwrap(),
            name: "Base".to_string(),
            environment: NetworkEnvironment::Mainnet,
            // points at the Ethereum-native instance we already have
            native_asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            rpc: RpcConfig {
                default_url: "https://rpc.example.com".to_string(),
            },
            explorers: vec![],
            features: NetworkFeatures::default(),
        });
        let err = Registry::from_documents(chain_doc, valid_asset_doc()).unwrap_err();
        assert!(matches!(
            err,
            RegistryError::InvalidReference { ref message } if message.contains("belongs to another network")
        ));
    }

    #[test]
    fn rejects_network_with_unknown_native_instance() {
        let mut chain_doc = valid_chain_doc();
        chain_doc.networks[0].native_asset_instance_id =
            AssetInstanceId::from_str("eip155:1/native:ghost").unwrap();
        let err = Registry::from_documents(chain_doc, valid_asset_doc()).unwrap_err();
        assert!(
            matches!(err, RegistryError::MissingAssetInstance(id) if id.as_str() == "eip155:1/native:ghost")
        );
    }

    #[test]
    fn lookup_misses_return_typed_errors() {
        let registry = valid_registry();
        assert!(matches!(
            registry.network("eip155:999").unwrap_err(),
            RegistryError::MissingNetwork(_)
        ));
        assert!(matches!(
            registry.asset_group("ghost").unwrap_err(),
            RegistryError::MissingAssetGroup(_)
        ));
        assert!(matches!(
            registry.asset_instrument("ghost.token").unwrap_err(),
            RegistryError::MissingAssetInstrument(_)
        ));
        assert!(matches!(
            registry
                .asset_instance("eip155:1/native:ghost")
                .unwrap_err(),
            RegistryError::MissingAssetInstance(_)
        ));
    }

    #[test]
    fn lookup_with_empty_id_reports_invalid_reference() {
        let registry = valid_registry();
        for err in [
            registry.network("").unwrap_err(),
            registry.asset_group("").unwrap_err(),
            registry.asset_instrument("").unwrap_err(),
            registry.asset_instance("").unwrap_err(),
        ] {
            assert!(matches!(err, RegistryError::InvalidReference { .. }));
        }
    }

    #[test]
    fn asset_instances_for_group_rejects_unknown_group() {
        let registry = valid_registry();
        assert!(matches!(
            registry.asset_instances_for_group("ghost").unwrap_err(),
            RegistryError::MissingAssetGroup(_)
        ));
    }

    #[test]
    fn chain_lookup_returns_default_derivation_path() {
        let registry = valid_registry();
        let chain = registry
            .chain(&ChainId::from_str("evm").unwrap())
            .expect("evm chain present");
        assert_eq!(
            chain.default_derivation_path.as_deref(),
            Some("m/44'/60'/0'/0/0")
        );
    }

    #[test]
    fn chain_lookup_misses_return_typed_error() {
        let registry = valid_registry();
        assert!(matches!(
            registry
                .chain(&ChainId::from_str("ghost").unwrap())
                .unwrap_err(),
            RegistryError::MissingChain(id) if id.as_str() == "ghost"
        ));
    }

    #[test]
    fn asset_instances_for_group_returns_matching_instances() {
        let registry = valid_registry();
        let instances = registry.asset_instances_for_group("eth").unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id.as_str(), "eip155:1/native:eth");
    }

    #[test]
    fn instance_with_invalid_shape_is_rejected() {
        // Native asset with a contract — shape validation must reject it.
        let mut asset_doc = valid_asset_doc();
        asset_doc.asset_instances[0].contract = Some("0xabc".to_string());
        let err = Registry::from_documents(valid_chain_doc(), asset_doc).unwrap_err();
        assert!(matches!(
            err,
            RegistryError::InvalidReference { ref message } if message.contains("shape invalid")
        ));
    }
}
