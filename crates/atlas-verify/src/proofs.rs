//! Kani proofs for atlas-core.
//!
//! Kept narrow on purpose. BigInt and BTreeMap don't symbolic-execute well in
//! Kani today, so the tractable surface is small. Each proof here exercises a
//! totality property that can be expressed over bounded primitives.
//!
//! Wider proofs (RLP length prefixes, gas u64 arithmetic, nonce comparison)
//! land with the future atlas-evm crate where the surface is naturally bounded.
//!
//! Each proof carries an explicit `#[kani::unwind(N)]` so CBMC has a concrete
//! loop bound. Without it, Kani falls back to global `--default-unwind` (set
//! to 2 in the nightly job) and emits "loop not fully unwound" diagnostics.
//! Per-proof bounds are preferred for deterministic CI time — see
//! https://model-checking.github.io/kani/tutorial-loop-unwinding.html.

use atlas_core::error::RegistryError;
use atlas_core::registry::{
    AssetRegistryDocument, ChainRegistryDocument, Registry, LATEST_REGISTRY_VERSION,
};

fn empty_chain_doc(version: u32) -> ChainRegistryDocument {
    ChainRegistryDocument {
        version,
        chains: Vec::new(),
        networks: Vec::new(),
    }
}

fn empty_asset_doc(version: u32) -> AssetRegistryDocument {
    AssetRegistryDocument {
        version,
        asset_groups: Vec::new(),
        asset_instruments: Vec::new(),
        asset_instances: Vec::new(),
    }
}

/// Any chain-registry version other than `LATEST_REGISTRY_VERSION` must be
/// rejected with `RegistryError::UnsupportedVersion`. The check runs before
/// any collection processing, so empty Vecs keep this Kani-tractable.
#[kani::proof]
#[kani::unwind(2)]
fn registry_rejects_unsupported_chain_version() {
    let v: u32 = kani::any();
    kani::assume(v != LATEST_REGISTRY_VERSION);

    let result =
        Registry::from_documents(empty_chain_doc(v), empty_asset_doc(LATEST_REGISTRY_VERSION));
    assert!(matches!(
        result,
        Err(RegistryError::UnsupportedVersion { version }) if version == v
    ));
}

/// Same property for the asset-registry version. The chain version is checked
/// first, so we hold it to the supported value and let the asset version vary.
#[kani::proof]
#[kani::unwind(2)]
fn registry_rejects_unsupported_asset_version() {
    let v: u32 = kani::any();
    kani::assume(v != LATEST_REGISTRY_VERSION);

    let result =
        Registry::from_documents(empty_chain_doc(LATEST_REGISTRY_VERSION), empty_asset_doc(v));
    assert!(matches!(
        result,
        Err(RegistryError::UnsupportedVersion { version }) if version == v
    ));
}

/// Two empty documents at the supported version produce a Registry without
/// error. Acts as a sanity check that the validation path doesn't reject the
/// minimum-valid input.
#[kani::proof]
#[kani::unwind(2)]
fn registry_accepts_empty_documents_at_supported_version() {
    let result = Registry::from_documents(
        empty_chain_doc(LATEST_REGISTRY_VERSION),
        empty_asset_doc(LATEST_REGISTRY_VERSION),
    );
    assert!(result.is_ok());
}
