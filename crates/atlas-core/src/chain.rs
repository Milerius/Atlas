use crate::id::{AssetInstanceId, ChainId, NetworkId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    pub id: ChainId,
    pub name: String,
    pub family: ChainFamily,
    #[serde(rename = "addressFormat")]
    pub address_format: AddressFormat,
    #[serde(rename = "defaultCurve")]
    pub default_curve: Curve,
    #[serde(rename = "supportedStandards")]
    pub supported_standards: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<ChainCapability>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub id: NetworkId,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default, rename = "chainId")]
    pub chain_id: Option<String>,
    pub chain: ChainId,
    pub name: String,
    pub environment: NetworkEnvironment,
    #[serde(rename = "nativeAssetInstanceId")]
    pub native_asset_instance_id: AssetInstanceId,
    pub rpc: RpcConfig,
    #[serde(default)]
    pub explorers: Vec<Explorer>,
    #[serde(default)]
    pub features: NetworkFeatures,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainFamily {
    AccountBased,
    Utxo,
    ObjectBased,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressFormat {
    EvmAddress,
    SolanaPubkey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    Secp256k1,
    Ed25519,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainCapability {
    Balance,
    Transfer,
    Approve,
    ContractCall,
    Broadcast,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkEnvironment {
    Mainnet,
    Testnet,
    Devnet,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RpcConfig {
    #[serde(rename = "defaultUrl")]
    pub default_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Explorer {
    pub name: String,
    pub tx: String,
    pub address: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NetworkFeatures {
    #[serde(default)]
    pub eip1559: bool,
    #[serde(default)]
    pub erc20: bool,
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
            rpc: RpcConfig { default_url: "https://mainnet.base.org".to_string() },
            explorers: vec![],
            features: NetworkFeatures::default(),
        };

        assert_eq!(network.chain.as_str(), "evm");
    }
}
