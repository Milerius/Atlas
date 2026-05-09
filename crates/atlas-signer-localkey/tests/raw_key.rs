use alloy_primitives::B256;
use atlas_core::chain::Curve;
use atlas_core::id::SignerId;
use atlas_core::signing::{SignerProvider, SigningPayloadKind, SigningRequest, SigningResponse};
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

const TEST_KEY: [u8; 32] = [
    0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e, 0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32, 0x06,
    0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06,
];

fn make_signer() -> LocalKeySigner {
    LocalKeySigner::from_bytes(SignerId::from_str("test-key").unwrap(), TEST_KEY).unwrap()
}

#[tokio::test]
async fn signs_a_32_byte_digest() {
    let signer = make_signer();
    let digest = B256::repeat_byte(0xab);
    let request = SigningRequest {
        account: atlas_core::id::AccountRef::from_str("account-1").unwrap(),
        network: atlas_core::id::NetworkId::from_str("eip155:1").unwrap(),
        curve: Curve::Secp256k1,
        payload_kind: SigningPayloadKind::TransactionDigest,
        payload: digest.to_vec(),
    };
    let response = signer.sign(request).await.unwrap();
    match response {
        SigningResponse::SignatureOnly {
            signature,
            public_key,
            ..
        } => {
            assert_eq!(signature.len(), 65);
            assert_eq!(public_key.len(), 65);
            assert_eq!(public_key[0], 0x04); // uncompressed marker
        }
        other => panic!("expected SignatureOnly, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_wrong_curve() {
    let signer = make_signer();
    let request = SigningRequest {
        account: atlas_core::id::AccountRef::from_str("account-1").unwrap(),
        network: atlas_core::id::NetworkId::from_str("eip155:1").unwrap(),
        curve: Curve::Ed25519,
        payload_kind: SigningPayloadKind::TransactionDigest,
        payload: vec![0u8; 32],
    };
    let err = signer.sign(request).await.unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::SigningError::UnsupportedCurve(_)
    ));
}

#[tokio::test]
async fn rejects_short_digest() {
    let signer = make_signer();
    let request = SigningRequest {
        account: atlas_core::id::AccountRef::from_str("account-1").unwrap(),
        network: atlas_core::id::NetworkId::from_str("eip155:1").unwrap(),
        curve: Curve::Secp256k1,
        payload_kind: SigningPayloadKind::TransactionDigest,
        payload: vec![0u8; 16], // too short
    };
    let err = signer.sign(request).await.unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::SigningError::UnsupportedPayload(_)
    ));
}

#[test]
fn address_is_eip55_format() {
    let signer = make_signer();
    let addr = signer.address();
    assert!(addr.starts_with("0x"));
    assert_eq!(addr.len(), 42);
}
