use crate::{
    asset::{AssetGroup, AssetInstance, AssetInstrument},
    chain::{Chain, Network},
    error::RegistryError,
    id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, NetworkId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LATEST_REGISTRY_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainRegistryDocument {
    pub version: u32,
    pub chains: Vec<Chain>,
    pub networks: Vec<Network>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetRegistryDocument {
    pub version: u32,
    #[serde(rename = "assetGroups")]
    pub asset_groups: Vec<AssetGroup>,
    #[serde(rename = "assetInstruments")]
    pub asset_instruments: Vec<AssetInstrument>,
    #[serde(rename = "assetInstances")]
    pub asset_instances: Vec<AssetInstance>,
}

#[derive(Clone, Debug)]
pub struct Registry {
    chains: BTreeMap<String, Chain>,
    networks: BTreeMap<String, Network>,
    asset_groups: BTreeMap<String, AssetGroup>,
    asset_instruments: BTreeMap<String, AssetInstrument>,
    asset_instances: BTreeMap<String, AssetInstance>,
}

impl Registry {
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

    pub fn network(&self, id: &str) -> Result<&Network, RegistryError> {
        self.networks.get(id).ok_or_else(|| missing_network(id))
    }

    pub fn asset_group(&self, id: &str) -> Result<&AssetGroup, RegistryError> {
        self.asset_groups
            .get(id)
            .ok_or_else(|| missing_asset_group(id))
    }

    pub fn asset_instrument(&self, id: &str) -> Result<&AssetInstrument, RegistryError> {
        self.asset_instruments
            .get(id)
            .ok_or_else(|| missing_asset_instrument(id))
    }

    pub fn asset_instance(&self, id: &str) -> Result<&AssetInstance, RegistryError> {
        self.asset_instances
            .get(id)
            .ok_or_else(|| missing_asset_instance(id))
    }

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
