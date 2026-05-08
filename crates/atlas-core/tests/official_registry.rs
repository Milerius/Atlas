//! Integration tests for Atlas's official in-tree registry.
//!
//! These verify that the curated `registries/{chain,asset}_registry.json`
//! files at the repo root parse, validate, and resolve as expected.

use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};

fn official_registry() -> Registry {
    let chain_doc: ChainRegistryDocument =
        serde_json::from_str(CHAIN_REGISTRY_JSON).expect("official chain registry parses");
    let asset_doc: AssetRegistryDocument =
        serde_json::from_str(ASSET_REGISTRY_JSON).expect("official asset registry parses");
    Registry::from_documents(chain_doc, asset_doc).expect("official registry validates")
}

#[test]
fn official_registry_loads_and_validates() {
    let _registry = official_registry();
}

#[test]
fn official_registry_has_all_three_networks() {
    let registry = official_registry();

    let ethereum = registry.network("eip155:1").unwrap();
    assert_eq!(ethereum.chain.as_str(), "evm");
    assert_eq!(ethereum.name, "Ethereum");

    let base = registry.network("eip155:8453").unwrap();
    assert_eq!(base.chain.as_str(), "evm");
    assert_eq!(base.name, "Base");

    let solana = registry
        .network("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp")
        .unwrap();
    assert_eq!(solana.chain.as_str(), "solana");
    assert_eq!(solana.name, "Solana Mainnet");
}

#[test]
fn usdc_group_resolves_across_three_networks() {
    let registry = official_registry();
    let instances = registry.asset_instances_for_group("usdc").unwrap();
    let ids = instances.iter().map(|i| i.id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&"eip155:1/erc20:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"));
    assert!(ids.contains(&"eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"));
    assert!(ids.contains(
        &"solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/spl:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
    ));
}

#[test]
fn eth_group_resolves_to_ethereum_and_base_native() {
    let registry = official_registry();
    let instances = registry.asset_instances_for_group("eth").unwrap();
    let ids = instances.iter().map(|i| i.id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"eip155:1/native:eth"));
    assert!(ids.contains(&"eip155:8453/native:eth"));
}

#[test]
fn sol_group_resolves_to_solana_native() {
    let registry = official_registry();
    let instances = registry.asset_instances_for_group("sol").unwrap();
    let ids = instances.iter().map(|i| i.id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids.len(), 1);
    assert!(ids.contains(&"solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/native:sol"));
}

#[test]
fn official_solana_usdc_is_an_spl_instance_with_circle_mint() {
    let registry = official_registry();
    let instance = registry
        .asset_instance(
            "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/spl:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        )
        .unwrap();
    assert_eq!(instance.standard, atlas_core::asset::AssetStandard::Spl);
    assert_eq!(
        instance.contract.as_deref(),
        Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
    );
    assert_eq!(instance.decimals, 6);
    // Exercise the new AssetStandard::Spl arm of validate_shape so this
    // test catches regressions in the validation logic, not just the data.
    instance
        .validate_shape()
        .expect("official Solana USDC SPL instance validates");
}
