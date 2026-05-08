use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};

pub fn registry() -> Registry {
    let chain_doc: ChainRegistryDocument =
        serde_json::from_str(include_str!("../fixtures/chain_registry.valid.json")).unwrap();
    let asset_doc: AssetRegistryDocument =
        serde_json::from_str(include_str!("../fixtures/asset_registry.valid.json")).unwrap();
    Registry::from_documents(chain_doc, asset_doc).unwrap()
}
