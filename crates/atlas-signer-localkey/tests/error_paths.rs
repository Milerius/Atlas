//! Edge-case construction paths for `LocalKeySigner`. These tests
//! complement the happy paths in `raw_key.rs`, `hd.rs`, and `keystore.rs`.

use atlas_core::id::SignerId;
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

const TEST_KEY: [u8; 32] = [0x01u8; 32];
const TEST_PASSWORD: &str = "test-password-123";
const TEST_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn from_bytes_with_zero_key_errors() {
    // Locked-in behavior: alloy/k256 reject the all-zero scalar (it is not
    // a valid secp256k1 private key).
    let zero = [0u8; 32];
    let r = LocalKeySigner::from_bytes(SignerId::from_str("z").unwrap(), zero);
    assert!(r.is_err());
}

#[test]
fn from_bytes_accepts_a_known_nonzero_key() {
    let r = LocalKeySigner::from_bytes(SignerId::from_str("k").unwrap(), TEST_KEY);
    assert!(r.is_ok());
}

#[test]
fn from_keystore_path_loads_a_freshly_generated_keystore() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rng = rand::thread_rng();
    let _uuid = eth_keystore::encrypt_key(
        dir.path(),
        &mut rng,
        TEST_KEY,
        TEST_PASSWORD,
        Some("test-key"),
    )
    .expect("encrypt_key");
    let path = dir.path().join("test-key");

    let signer =
        LocalKeySigner::from_keystore_path(SignerId::from_str("kp").unwrap(), &path, TEST_PASSWORD)
            .expect("from_keystore_path must succeed with the correct password");
    assert!(signer.address().starts_with("0x"));
    assert_eq!(signer.address().len(), 42);
}

#[test]
fn from_keystore_path_wrong_password_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rng = rand::thread_rng();
    let _uuid =
        eth_keystore::encrypt_key(dir.path(), &mut rng, TEST_KEY, TEST_PASSWORD, Some("kf"))
            .expect("encrypt_key");
    let path = dir.path().join("kf");
    let r = LocalKeySigner::from_keystore_path(
        SignerId::from_str("kpw").unwrap(),
        &path,
        "wrong-password",
    );
    assert!(r.is_err());
}

#[test]
fn from_keystore_path_missing_file_errors() {
    let r = LocalKeySigner::from_keystore_path(
        SignerId::from_str("nope").unwrap(),
        std::path::Path::new("/no/such/keystore.json"),
        TEST_PASSWORD,
    );
    assert!(r.is_err());
}

#[test]
fn from_mnemonic_with_long_path_derives_distinct_addresses() {
    let a = LocalKeySigner::from_mnemonic(
        SignerId::from_str("a").unwrap(),
        TEST_MNEMONIC,
        "m/44'/60'/0'/0/0",
    )
    .unwrap();
    let b = LocalKeySigner::from_mnemonic(
        SignerId::from_str("b").unwrap(),
        TEST_MNEMONIC,
        "m/44'/60'/0'/0/1",
    )
    .unwrap();
    assert_ne!(
        a.address().to_lowercase(),
        b.address().to_lowercase(),
        "different child paths must produce different addresses"
    );
}

#[test]
fn from_mnemonic_with_garbled_path_errors() {
    let r = LocalKeySigner::from_mnemonic(
        SignerId::from_str("g").unwrap(),
        TEST_MNEMONIC,
        "m/not-a-segment/foo",
    );
    assert!(r.is_err());
}

#[test]
fn from_mnemonic_with_empty_path_errors() {
    // Empty path is rejected by bip32::DerivationPath.
    let r = LocalKeySigner::from_mnemonic(SignerId::from_str("e").unwrap(), TEST_MNEMONIC, "");
    assert!(r.is_err());
}

#[test]
fn address_starts_with_0x_for_known_test_key() {
    let signer = LocalKeySigner::from_bytes(SignerId::from_str("k").unwrap(), TEST_KEY).unwrap();
    let addr = signer.address();
    assert!(addr.starts_with("0x"));
    assert_eq!(addr.len(), 42);
    // CR-7: address must now use the EIP-55 mixed-case form. The known
    // address derived from [0x01; 32] is 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf.
    // Its lowercase variant differs from the EIP-55 form, so the canonical
    // string should not equal its own lowercase.
    assert_ne!(
        addr,
        addr.to_lowercase(),
        "address should be EIP-55 mixed-case"
    );
}

#[tokio::test]
async fn signer_id_round_trips_through_signer_provider() {
    use atlas_core::signing::SignerProvider;
    let id = SignerId::from_str("the-id").unwrap();
    let signer = LocalKeySigner::from_bytes(id.clone(), TEST_KEY).unwrap();
    assert_eq!(signer.id().as_str(), id.as_str());
}
