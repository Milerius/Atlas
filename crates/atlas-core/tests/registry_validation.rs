use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};

fn parse_chain_registry() -> ChainRegistryDocument {
    serde_json::from_str(include_str!("fixtures/chain_registry.valid.json")).unwrap()
}

fn parse_asset_registry(path: &str) -> AssetRegistryDocument {
    serde_json::from_str(match path {
        "valid" => include_str!("fixtures/asset_registry.valid.json"),
        "missing_network" => include_str!("fixtures/asset_registry.invalid_missing_network.json"),
        "native_contract" => include_str!("fixtures/asset_registry.invalid_native_contract.json"),
        _ => panic!("unknown fixture key"),
    })
    .unwrap()
}

#[test]
fn valid_registries_load_and_validate() {
    let registry =
        Registry::from_documents(parse_chain_registry(), parse_asset_registry("valid")).unwrap();
    assert_eq!(registry.network("eip155:8453").unwrap().name, "Base");
    assert_eq!(registry.asset_group("usdc").unwrap().symbol, "USDC");
}

#[test]
fn asset_instance_with_missing_network_fails_validation() {
    let err = Registry::from_documents(
        parse_chain_registry(),
        parse_asset_registry("missing_network"),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "missing network: eip155:999");
}

#[test]
fn native_asset_with_contract_fails_validation() {
    let err = Registry::from_documents(
        parse_chain_registry(),
        parse_asset_registry("native_contract"),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid registry reference: asset instance eip155:1/native:eth shape invalid: invalid contract: native asset must not have a contract"
    );
}
