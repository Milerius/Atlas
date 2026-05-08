use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};

fn registry() -> Registry {
    let chain_doc: ChainRegistryDocument =
        serde_json::from_str(include_str!("fixtures/chain_registry.valid.json")).unwrap();
    let asset_doc: AssetRegistryDocument =
        serde_json::from_str(include_str!("fixtures/asset_registry.valid.json")).unwrap();
    Registry::from_documents(chain_doc, asset_doc).unwrap()
}

#[test]
fn usdc_group_resolves_to_ethereum_and_base_instances() {
    let registry = registry();
    let instances = registry.asset_instances_for_group("usdc").unwrap();
    let ids = instances
        .iter()
        .map(|instance| instance.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids.len(), 2);
    assert!(ids.iter().any(|id| id.starts_with("eip155:1/erc20:")));
    assert!(ids.iter().any(|id| id.starts_with("eip155:8453/erc20:")));
}

#[test]
fn eth_group_resolves_to_ethereum_and_base_native_instances() {
    let registry = registry();
    let instances = registry.asset_instances_for_group("eth").unwrap();
    let ids = instances
        .iter()
        .map(|instance| instance.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["eip155:1/native:eth", "eip155:8453/native:eth"]);
}

#[test]
fn exact_asset_instance_lookup_returns_base_usdc() {
    let registry = registry();
    let instance = registry
        .asset_instance("eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")
        .unwrap();

    assert_eq!(instance.network.as_str(), "eip155:8453");
    assert_eq!(instance.instrument_id.as_str(), "usdc.circle");
}
