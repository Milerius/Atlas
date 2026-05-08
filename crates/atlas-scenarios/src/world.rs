//! Cucumber [`World`] for atlas-core BDD scenarios.

use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetInstance;
use atlas_core::error::ChainError;
use atlas_core::id::AssetInstanceId;
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
use atlas_core::service::{ChainService, MockEvmService};
use atlas_core::signing::MockSigner;
use atlas_core::transaction::BroadcastResult;
use cucumber::World;

/// State carried across steps in a single scenario.
#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct AtlasWorld {
    /// Validated registry built from the standard valid fixtures.
    pub registry: Option<Registry>,
    /// Mock chain service installed by `Given a mock EVM chain service`.
    pub service: Option<MockEvmService>,
    /// Mock signer installed by `Given a mock signer named ...`.
    pub signer: Option<MockSigner>,
    /// Most-recent resolved instance, if a step asked for one.
    pub last_instance: Option<AssetInstance>,
    /// Most-recent group resolution result, if any.
    pub last_group_instances: Vec<AssetInstance>,
    /// Most-recent broadcast result on the happy path.
    pub last_broadcast: Option<BroadcastResult>,
    /// Most-recent error seen on the failure path.
    pub last_error: Option<ChainError>,
    /// Scratch amount used while assembling a transfer intent.
    pub scratch_amount: Option<RawAmount>,
    /// Scratch recipient address.
    pub scratch_to: Option<String>,
    /// Scratch override for the asset instance id used in the next prepare.
    pub scratch_asset_instance_id: Option<AssetInstanceId>,
    /// Scratch override for the network id used in the next prepare.
    pub scratch_network_id: Option<atlas_core::id::NetworkId>,
}

impl AtlasWorld {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the standard valid fixtures shipped with atlas-core's tests
    /// directory and install the resulting registry.
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

    pub fn service(&self) -> &MockEvmService {
        self.service
            .as_ref()
            .expect("chain service not installed — add `Given a mock EVM chain service`")
    }

    pub fn signer(&self) -> &MockSigner {
        self.signer
            .as_ref()
            .expect("signer not installed — add `Given a mock signer named ...`")
    }
}

/// Run the asynchronous transfer pipeline using the world's installed
/// service and signer plus the scratch fields, recording either the
/// broadcast result or the chain error.
pub async fn run_transfer_pipeline(world: &mut AtlasWorld) {
    use atlas_core::id::AccountRef;
    use atlas_core::signing::SignerProvider as _;
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

    let intent = TransferIntent {
        asset_instance_id,
        to: atlas_core::id::AddressRef::from_str(&to).expect("address ref valid"),
        amount,
    };

    let service = world.service.as_ref().expect("service");
    let signer = world.signer.as_ref().expect("signer");

    let unsigned_result = service
        .prepare_transfer(
            AccountRef::from_str("account-1").expect("account ref"),
            network_id,
            intent,
        )
        .await;

    match unsigned_result {
        Ok(unsigned) => {
            let request = service.signing_request(&unsigned).expect("signing request");
            let response = signer.sign(request).await.expect("mock sign");
            let signed = service
                .assemble_signed_transaction(unsigned, response)
                .expect("assemble");
            let broadcast = service.broadcast(signed).await.expect("broadcast");
            world.last_broadcast = Some(broadcast);
        }
        Err(err) => {
            world.last_error = Some(err);
        }
    }
}
