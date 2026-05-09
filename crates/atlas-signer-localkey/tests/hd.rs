use atlas_core::id::{ChainId, SignerId};
use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

// BIP-39 standard test mnemonic.
const TEST_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn official_registry() -> Registry {
    let chain_doc: ChainRegistryDocument = serde_json::from_str(CHAIN_REGISTRY_JSON).unwrap();
    let asset_doc: AssetRegistryDocument = serde_json::from_str(ASSET_REGISTRY_JSON).unwrap();
    Registry::from_documents(chain_doc, asset_doc).unwrap()
}

#[test]
fn registry_default_evm_path_derives_known_address() {
    // The HD path comes from the official chain registry, not a magic
    // string at the call site. Atlas's recommended path for the EVM
    // chain family is `m/44'/60'/0'/0/0`; the canonical address for the
    // BIP-39 test mnemonic on that path is well-known:
    // 0x9858EfFD232B4033E47d90003D41EC34EcaEda94.
    let registry = official_registry();
    let evm_chain = registry.chain(&ChainId::from_str("evm").unwrap()).unwrap();
    let path = evm_chain
        .default_derivation_path
        .as_deref()
        .expect("EVM chain entry must carry a default derivation path");
    assert_eq!(path, "m/44'/60'/0'/0/0");

    let signer =
        LocalKeySigner::from_mnemonic(SignerId::from_str("test").unwrap(), TEST_MNEMONIC, path)
            .unwrap();
    assert_eq!(
        signer.address().to_lowercase(),
        "0x9858effd232b4033e47d90003d41ec34ecaeda94"
    );
}

#[test]
fn rejects_invalid_mnemonic() {
    let err = LocalKeySigner::from_mnemonic(
        SignerId::from_str("test").unwrap(),
        "not a valid mnemonic at all here",
        "m/44'/60'/0'/0/0",
    );
    assert!(err.is_err());
}

#[test]
fn rejects_invalid_path() {
    let err = LocalKeySigner::from_mnemonic(
        SignerId::from_str("test").unwrap(),
        TEST_MNEMONIC,
        "not-a-path",
    );
    assert!(err.is_err());
}
