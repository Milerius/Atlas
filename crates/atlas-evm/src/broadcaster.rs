//! `EvmBroadcaster` — `eth_sendRawTransaction` via alloy-provider.

use alloy_provider::Provider;
use async_trait::async_trait;
use atlas_core::error::ChainError;
use atlas_core::service::ChainBroadcaster;
use atlas_core::transaction::{BroadcastResult, SignedTransaction};

use crate::error::map_transport_err;

pub struct EvmBroadcaster<P> {
    provider: P,
}

impl<P> EvmBroadcaster<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: Provider + Clone> ChainBroadcaster for EvmBroadcaster<P> {
    async fn broadcast(&self, signed: SignedTransaction) -> Result<BroadcastResult, ChainError> {
        if signed.raw.is_empty() {
            return Err(ChainError::BroadcastFailed(
                "empty signed transaction".to_string(),
            ));
        }
        let pending = self
            .provider
            .send_raw_transaction(&signed.raw)
            .await
            .map_err(map_transport_err)?;
        let tx_hash = format!("{:#x}", pending.tx_hash());
        Ok(BroadcastResult { tx_hash })
    }
}
