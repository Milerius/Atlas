use crate::{
    amount::RawAmount,
    id::{AccountRef, AssetInstanceId, NetworkId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransferIntent {
    #[serde(rename = "assetInstanceId")]
    pub asset_instance_id: AssetInstanceId,
    pub to: String,
    pub amount: RawAmount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnsignedTransaction {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub network: NetworkId,
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BroadcastResult {
    #[serde(rename = "txHash")]
    pub tx_hash: String,
}
