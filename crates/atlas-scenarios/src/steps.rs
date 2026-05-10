//! Step definitions for atlas-core BDD scenarios.
//!
//! Steps mutate the [`AtlasWorld`] via the helper methods defined in `world.rs`.

use crate::world::{run_transfer_pipeline, AtlasWorld};
use atlas_core::address::AddressError;
use atlas_core::amount::RawAmount;
use atlas_core::chain::AddressFormat;
use atlas_core::id::{AddressRef, AssetInstanceId, ChainId, NetworkId, SignerId};
use atlas_core::signing::MockSigner;
use atlas_evm::mock::MockEvmChainService;
use atlas_signer_localkey::LocalKeySigner;
use cucumber::{given, then, when};
use num_bigint::BigInt;
use std::str::FromStr;

/// Standard BIP-39 test mnemonic. The canonical Ethereum address at
/// `m/44'/60'/0'/0/0` for this mnemonic is well-known and used by
/// every wallet's smoke tests.
const TEST_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

// ── Given ──────────────────────────────────────────────────────────────────

#[given("a registry loaded from the valid fixtures")]
pub fn registry_loaded(world: &mut AtlasWorld) {
    world.load_valid_fixtures();
}

#[given("a mock EVM chain service")]
pub fn mock_service(world: &mut AtlasWorld) {
    world.service = Some(MockEvmChainService);
}

#[given(regex = r#"^a mock signer named "([^"]+)"$"#)]
pub fn mock_signer(world: &mut AtlasWorld, signer_id: String) {
    world.signer = Some(MockSigner::new(
        SignerId::from_str(&signer_id).expect("signer id"),
    ));
}

// ── When ───────────────────────────────────────────────────────────────────

#[when(regex = r#"^I resolve asset instances for group "([^"]+)"$"#)]
pub fn resolve_group(world: &mut AtlasWorld, group_id: String) {
    let registry = world.registry();
    match registry.asset_instances_for_group(&group_id) {
        Ok(instances) => {
            world.last_group_instances = instances.into_iter().cloned().collect();
        }
        Err(_) => {
            world.last_group_instances.clear();
        }
    }
}

#[when(regex = r#"^I look up asset instance "([^"]+)"$"#)]
pub fn lookup_instance(world: &mut AtlasWorld, instance_id: String) {
    let registry = world.registry();
    if let Ok(instance) = registry.asset_instance(&instance_id) {
        world.last_instance = Some(instance.clone());
    }
}

#[when(
    regex = r#"^I prepare a transfer of (\d+) base units to "([^"]+)" using instance "([^"]+)" on network "([^"]+)"$"#
)]
pub async fn prepare_and_run(
    world: &mut AtlasWorld,
    raw: u64,
    to: String,
    instance_str: String,
    network_str: String,
) {
    // Resolve instance to inherit decimals.
    let instance = world
        .registry()
        .asset_instance(&instance_str)
        .expect("instance must exist for transfer scenario")
        .clone();
    world.scratch_amount =
        Some(RawAmount::new(BigInt::from(raw), instance.decimals).expect("non-negative amount"));
    world.scratch_to = Some(to);
    world.scratch_asset_instance_id = Some(instance.id.clone());
    world.scratch_network_id = Some(NetworkId::from_str(&network_str).expect("network id"));

    run_transfer_pipeline(world).await;
}

// ── Then ───────────────────────────────────────────────────────────────────

#[then(regex = r#"^I get (\d+) instances$"#)]
pub fn assert_count(world: &mut AtlasWorld, n: usize) {
    assert_eq!(
        world.last_group_instances.len(),
        n,
        "expected {n} instances, got {}",
        world.last_group_instances.len()
    );
}

#[then(regex = r#"^one of the instances has id "([^"]+)"$"#)]
pub fn assert_instance_in_group(world: &mut AtlasWorld, expected_id: String) {
    let found = world
        .last_group_instances
        .iter()
        .any(|i| i.id.as_str() == expected_id);
    assert!(
        found,
        "expected an instance with id {expected_id} in {:?}",
        world
            .last_group_instances
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
    );
}

#[then(regex = r#"^the instance network is "([^"]+)"$"#)]
pub fn assert_instance_network(world: &mut AtlasWorld, expected: String) {
    let instance = world.last_instance.as_ref().expect("no instance resolved");
    assert_eq!(instance.network.as_str(), expected);
}

#[then(regex = r#"^the broadcast tx hash is "([^"]+)"$"#)]
pub fn assert_broadcast_hash(world: &mut AtlasWorld, expected: String) {
    let broadcast = world
        .last_broadcast
        .as_ref()
        .expect("no broadcast result; transfer must have failed");
    assert_eq!(broadcast.tx_hash, expected);
}

#[then("the transfer is rejected as unsupported asset instance")]
pub fn assert_unsupported_asset_instance(world: &mut AtlasWorld) {
    let err = world.last_error.as_ref().expect("no error captured");
    assert!(
        matches!(
            err,
            atlas_core::error::ChainError::UnsupportedAssetInstance(_)
        ),
        "expected UnsupportedAssetInstance, got {err:?}",
    );
}

#[then("the registry has 2 networks under the EVM chain")]
pub fn assert_two_evm_networks(world: &mut AtlasWorld) {
    // Implicit fixture-shape assertion: Ethereum + Base both reference chain "evm".
    let registry = world.registry();
    let ethereum = registry.network("eip155:1").expect("ethereum");
    let base = registry.network("eip155:8453").expect("base");
    assert_eq!(ethereum.chain.as_str(), "evm");
    assert_eq!(base.chain.as_str(), "evm");
}

// ── Signer wiring through the registry ─────────────────────────────────────

#[then(regex = r#"^the EVM chain default derivation path is "([^"]+)"$"#)]
pub fn assert_evm_default_derivation_path(world: &mut AtlasWorld, expected: String) {
    let chain = world
        .registry()
        .chain(&ChainId::from_str("evm").expect("evm chain id"))
        .expect("evm chain entry must exist in the valid fixtures");
    let path = chain
        .default_derivation_path
        .as_deref()
        .expect("EVM chain entry must carry a default_derivation_path");
    assert_eq!(path, expected);
}

#[when(
    "I derive a LocalKeySigner from the BIP-39 test mnemonic using the EVM chain's default path"
)]
pub fn derive_signer_via_registry_path(world: &mut AtlasWorld) {
    let chain = world
        .registry()
        .chain(&ChainId::from_str("evm").expect("evm chain id"))
        .expect("evm chain entry must exist");
    let path = chain
        .default_derivation_path
        .as_deref()
        .expect("EVM chain entry must carry a default_derivation_path");
    let signer = LocalKeySigner::from_mnemonic(
        SignerId::from_str("scenario-signer").expect("signer id"),
        TEST_MNEMONIC,
        path,
    )
    .expect("LocalKeySigner construction must succeed for the BIP-39 test mnemonic");
    world.last_signer_address = Some(signer.address());
}

#[then(regex = r#"^the signer address is "([^"]+)"$"#)]
pub fn assert_signer_address(world: &mut AtlasWorld, expected: String) {
    let actual = world
        .last_signer_address
        .as_deref()
        .expect("no signer address captured");
    // Compare lowercased so the assertion is independent of EIP-55
    // checksum rendering — the underlying bytes are what matter.
    assert_eq!(actual.to_lowercase(), expected.to_lowercase());
}

// ── Typed ids: CAIP and address-format validation ──────────────────────────

#[then(
    regex = r#"^the asset instance "([^"]+)" decomposes to network "([^"]+)", asset namespace "([^"]+)", asset reference "([^"]+)"$"#
)]
pub fn assert_caip19_decomposition(
    _world: &mut AtlasWorld,
    instance_id: String,
    expected_network: String,
    expected_namespace: String,
    expected_reference: String,
) {
    let id = AssetInstanceId::from_str(&instance_id)
        .expect("scenario instance id must be CAIP-19 valid");
    assert_eq!(id.network_id().as_str(), expected_network);
    assert_eq!(id.asset_namespace(), expected_namespace);
    assert_eq!(id.asset_reference(), expected_reference);
}

#[when(regex = r#"^I try to construct a NetworkId from "([^"]+)"$"#)]
pub fn try_construct_network_id(world: &mut AtlasWorld, raw: String) {
    world.last_network_id_result = Some(NetworkId::new(raw));
}

#[then("it surfaces a CAIP validation error")]
pub fn assert_caip_validation_error(world: &mut AtlasWorld) {
    let result = world
        .last_network_id_result
        .as_ref()
        .expect("no NetworkId construction attempt captured");
    let err = result.as_ref().expect_err("expected an error");
    assert!(
        matches!(err, atlas_core::id::IdError::Caip(_)),
        "expected IdError::Caip, got {err:?}"
    );
}

#[when(regex = r#"^I validate the address "([^"]+)" against the (EVM|Solana) format$"#)]
pub fn validate_address_for_format(world: &mut AtlasWorld, raw: String, format_label: String) {
    let format = match format_label.as_str() {
        "EVM" => AddressFormat::EvmAddress,
        "Solana" => AddressFormat::SolanaPubkey,
        other => panic!("unknown address format label: {other}"),
    };
    let address = AddressRef::new(raw).expect("scenario address must be non-empty");
    world.last_address_validation = Some(address.validate_for(&format));
}

#[then("validation succeeds")]
pub fn assert_address_validation_succeeds(world: &mut AtlasWorld) {
    let result = world
        .last_address_validation
        .as_ref()
        .expect("no address validation captured");
    result.as_ref().unwrap_or_else(|e| {
        panic!("expected validation to succeed, got: {e:?}");
    });
}

#[then("validation fails with an InvalidFormat error")]
pub fn assert_address_validation_fails(world: &mut AtlasWorld) {
    let result = world
        .last_address_validation
        .as_ref()
        .expect("no address validation captured");
    let err = result.as_ref().expect_err("expected an error");
    assert!(
        matches!(err, AddressError::InvalidFormat { .. }),
        "expected AddressError::InvalidFormat, got {err:?}"
    );
}
