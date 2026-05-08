use crate::{
    error::AssetError,
    id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, NetworkId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetGroup {
    pub id: AssetGroupId,
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub metadata: AssetMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetInstrument {
    pub id: AssetInstrumentId,
    #[serde(rename = "groupId")]
    pub group_id: AssetGroupId,
    #[serde(rename = "assetClass")]
    pub asset_class: AssetClass,
    pub kind: InstrumentKind,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub traits: Vec<AssetTrait>,
    #[serde(default)]
    pub metadata: AssetMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetInstance {
    pub id: AssetInstanceId,
    #[serde(rename = "instrumentId")]
    pub instrument_id: AssetInstrumentId,
    pub network: NetworkId,
    pub standard: AssetStandard,
    pub decimals: u8,
    #[serde(default)]
    pub contract: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<AssetCapability>,
    #[serde(default)]
    pub metadata: AssetMetadata,
}

impl AssetInstance {
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
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Crypto,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    NativeCoin,
    FungibleToken,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStandard {
    Native,
    Erc20,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetTrait {
    Fungible,
    Transferable,
    GasAsset,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetCapability {
    Balance,
    Transfer,
    Approve,
    PayGas,
    Swap,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetMetadata {
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
