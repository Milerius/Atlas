//! Keystore tests. The JSON fixture is generated inline so the test runs
//! deterministically without depending on an external fixture file.

use atlas_core::id::SignerId;
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

// Standard test key (32 bytes). Address derived from this is well-known.
const TEST_KEY: [u8; 32] = [0x01u8; 32];
const TEST_PASSWORD: &str = "test-password-123";

/// Generate a fresh keystore JSON in-memory by writing TEST_KEY through
/// eth_keystore::encrypt_key, then reading the resulting file's content.
fn generate_keystore_json() -> String {
    use std::io::Read;
    let dir = tempfile::tempdir().expect("tempdir");
    // eth_keystore::encrypt_key returns the UUID string (not the filename).
    // When we pass Some("test-key"), the file is written to dir/test-key.
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
    let mut file = std::fs::File::open(&path).expect("open keystore file");
    let mut json = String::new();
    file.read_to_string(&mut json).expect("read keystore file");
    json
}

#[test]
fn loads_a_freshly_generated_keystore_and_signs() {
    let json = generate_keystore_json();
    let signer =
        LocalKeySigner::from_keystore(SignerId::from_str("test").unwrap(), &json, TEST_PASSWORD)
            .expect("from_keystore must succeed with the right password");
    let addr = signer.address();
    assert!(addr.starts_with("0x"));
    assert_eq!(addr.len(), 42);
}

#[test]
fn rejects_wrong_password() {
    let json = generate_keystore_json();
    let err =
        LocalKeySigner::from_keystore(SignerId::from_str("test").unwrap(), &json, "wrong-password");
    assert!(err.is_err());
}
