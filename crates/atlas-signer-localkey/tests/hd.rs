use atlas_core::id::SignerId;
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

// BIP-39 standard test mnemonic.
const TEST_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn bip32_path_m_44_60_0_0_0_derives_known_address() {
    // Standard Ethereum HD path; the canonical address for this mnemonic
    // is well-known: 0x9858EfFD232B4033E47d90003D41EC34EcaEda94.
    let signer = LocalKeySigner::from_mnemonic(
        SignerId::from_str("test").unwrap(),
        TEST_MNEMONIC,
        "m/44'/60'/0'/0/0",
    )
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
