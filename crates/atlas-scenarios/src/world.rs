//! Cucumber [`World`] for atlas-core BDD scenarios.

use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetInstance;
use atlas_core::error::ChainError;
use atlas_core::id::AssetInstanceId;
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
use atlas_core::signing::MockSigner;
use atlas_core::transaction::BroadcastResult;
use atlas_evm::mock::MockEvmChainService;
use cucumber::World;

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct AtlasWorld {
    pub registry: Option<Registry>,
    pub service: Option<MockEvmChainService>,
    pub signer: Option<MockSigner>,
    pub last_instance: Option<AssetInstance>,
    pub last_group_instances: Vec<AssetInstance>,
    pub last_broadcast: Option<BroadcastResult>,
    pub last_error: Option<ChainError>,
    pub scratch_amount: Option<RawAmount>,
    pub scratch_to: Option<String>,
    pub scratch_asset_instance_id: Option<AssetInstanceId>,
    pub scratch_network_id: Option<atlas_core::id::NetworkId>,
}

impl AtlasWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_valid_fixtures(&mut self) {
        let chain_doc: ChainRegistryDocument = serde_json::from_str(include_str!(
            "../../atlas-core/tests/fixtures/chain_registry.valid.json"
        ))
        .expect("valid chain fixture");
        let asset_doc: AssetRegistryDocument = serde_json::from_str(include_str!(
            "../../atlas-core/tests/fixtures/asset_registry.valid.json"
        ))
        .expect("valid asset fixture");
        self.registry =
            Some(Registry::from_documents(chain_doc, asset_doc).expect("registry validates"));
    }

    pub fn registry(&self) -> &Registry {
        self.registry
            .as_ref()
            .expect("registry not loaded — start with `Given a valid registry`")
    }
}

/// Run the orchestrator path: build a TransferIntent from the scratch fields
/// and call MockEvmChainService::transfer. Records broadcast result or error.
pub async fn run_transfer_pipeline(world: &mut AtlasWorld) {
    use atlas_core::id::{AccountRef, AddressRef};
    use atlas_core::service::ChainService;
    use atlas_core::transaction::TransferIntent;
    use std::str::FromStr;

    let asset_instance_id = world
        .scratch_asset_instance_id
        .clone()
        .expect("asset instance id not set");
    let network_id = world
        .scratch_network_id
        .clone()
        .expect("network id not set");
    let amount = world.scratch_amount.clone().expect("amount not set");
    let to = world.scratch_to.clone().expect("recipient address not set");

    // Validate against the scratch network — replicates the old per-step
    // network check that previously lived inside the old mock service.
    let prefix = format!("{}/", network_id.as_str());
    if !asset_instance_id.as_str().starts_with(&prefix) {
        world.last_error = Some(ChainError::UnsupportedAssetInstance(asset_instance_id));
        return;
    }

    let intent = TransferIntent {
        asset_instance_id,
        to: AddressRef::from_str(&to).expect("address ref valid"),
        amount,
    };

    let service = world.service.as_ref().expect("service");
    let signer = world.signer.as_ref().expect("signer");

    match service
        .transfer(intent, AccountRef::from_str("account-1").unwrap(), signer)
        .await
    {
        Ok(broadcast) => world.last_broadcast = Some(broadcast),
        Err(err) => world.last_error = Some(err),
    }
}
