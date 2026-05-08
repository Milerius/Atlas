//! Provider-neutral signing boundary.
//!
//! Atlas does not pick a custody model. The same [`SignerProvider`] trait
//! covers MPC, local keys, hardware wallets, account-abstraction relays,
//! Privy, and future signers. The trade-off is that a signer may return
//! one of three response shapes — see [`SigningResponse`] — and the
//! chain service has to know how to assemble a final transaction from
//! whichever shape it gets.

use crate::{
    chain::Curve,
    error::SigningError,
    id::{AccountRef, NetworkId, SignerId},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Stable reference to a configured signer.
///
/// Used by higher layers (account configuration, routing tables) to
/// point at a [`SignerProvider`] without holding the provider object.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignerRef {
    /// The signer-provider id this reference resolves to.
    pub id: SignerId,
}

/// Input handed to a [`SignerProvider`] when atlas-core needs a
/// signature.
///
/// `payload` is opaque bytes whose interpretation is given by
/// `payload_kind`. The signer must support both `curve` and
/// `payload_kind`, otherwise it returns
/// [`SigningError::UnsupportedCurve`] or
/// [`SigningError::UnsupportedPayload`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SigningRequest {
    /// Account the signature should be attributed to.
    pub account: AccountRef,
    /// Network the signature is bound to (chain-specific encoding,
    /// replay protection).
    pub network: NetworkId,
    /// Elliptic curve (`secp256k1` for EVM, `ed25519` for Solana, …).
    pub curve: Curve,
    /// Shape of the bytes in `payload`.
    #[serde(rename = "payloadKind")]
    pub payload_kind: SigningPayloadKind,
    /// Bytes the signer must sign over.
    pub payload: Vec<u8>,
}

/// Tag for what's inside a [`SigningRequest::payload`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningPayloadKind {
    /// 32-byte digest of an encoded transaction (the EVM common case).
    TransactionDigest,
    /// Full unsigned transaction bytes (some MPC providers / Solana
    /// expect this).
    UnsignedTransaction,
    /// Arbitrary message bytes (e.g. EIP-191 personal sign).
    Message,
    /// Structured typed data (e.g. EIP-712).
    TypedData,
}

/// What a signer returns. Three shapes cover the realistic custody
/// landscape — atlas-core does not force one model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningResponse {
    /// Signer returned just a raw signature; the chain service is
    /// responsible for assembling the final transaction.
    SignatureOnly {
        /// Identifier of the signer that produced the signature.
        signer: SignerId,
        /// Raw signature bytes (curve-specific encoding).
        signature: Vec<u8>,
        /// Public key bytes corresponding to the signature.
        public_key: Vec<u8>,
    },
    /// Signer returned an already-encoded signed transaction. Chain
    /// service should pass this straight to broadcast.
    SignedTransaction {
        /// Identifier of the signer that produced the transaction.
        signer: SignerId,
        /// Raw signed-transaction bytes ready for broadcast.
        raw: Vec<u8>,
    },
    /// Signer broadcast the transaction itself (custodial / relay
    /// providers). Chain service skips broadcast and surfaces the hash.
    SubmittedTransaction {
        /// Identifier of the signer that submitted the transaction.
        signer: SignerId,
        /// Hash of the submitted transaction returned by the signer.
        tx_hash: String,
    },
}

/// Provider-neutral signing trait. Implementations live in adapter
/// crates (`atlas-signer-localkey`, future MPC / Privy / 4337 adapters).
///
/// Implementations must be `Send + Sync` so they can be shared across
/// async tasks.
#[async_trait]
pub trait SignerProvider: Send + Sync {
    /// Stable identifier of this signer (matches the `signer` field on
    /// [`SigningResponse`] variants).
    fn id(&self) -> &SignerId;

    /// Sign the requested payload, returning whichever
    /// [`SigningResponse`] shape this provider emits.
    async fn sign(&self, request: SigningRequest) -> Result<SigningResponse, SigningError>;
}

/// In-tree mock signer for tests and the smoke flow. Returns a
/// deterministic [`SigningResponse::SignatureOnly`] containing the
/// literal bytes `b"mock-signature"` and `b"mock-public-key"`.
///
/// Real signers ship in adapter crates (none yet — see
/// `README` for what's deferred).
#[derive(Clone, Debug)]
pub struct MockSigner {
    id: SignerId,
}

impl MockSigner {
    /// Build a [`MockSigner`] with the given id.
    pub fn new(id: SignerId) -> Self {
        Self { id }
    }
}

#[async_trait]
impl SignerProvider for MockSigner {
    fn id(&self) -> &SignerId {
        &self.id
    }

    async fn sign(&self, request: SigningRequest) -> Result<SigningResponse, SigningError> {
        if request.payload.is_empty() {
            return Err(SigningError::UnsupportedPayload(
                "empty payload".to_string(),
            ));
        }
        Ok(SigningResponse::SignatureOnly {
            signer: self.id.clone(),
            signature: b"mock-signature".to_vec(),
            public_key: b"mock-public-key".to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn signing_request(payload: Vec<u8>) -> SigningRequest {
        SigningRequest {
            account: AccountRef::from_str("account-1").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            curve: Curve::Secp256k1,
            payload_kind: SigningPayloadKind::TransactionDigest,
            payload,
        }
    }

    #[test]
    fn mock_signer_exposes_configured_id() {
        let id = SignerId::from_str("mock-signer").unwrap();
        let signer = MockSigner::new(id.clone());
        assert_eq!(signer.id(), &id);
    }

    #[tokio::test]
    async fn mock_signer_rejects_empty_payload() {
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let err = signer.sign(signing_request(vec![])).await.unwrap_err();
        assert!(matches!(err, SigningError::UnsupportedPayload(_)));
    }

    #[test]
    fn signing_response_variants_serialize_in_snake_case() {
        let signer = SignerId::from_str("mock").unwrap();
        let signature_only = SigningResponse::SignatureOnly {
            signer: signer.clone(),
            signature: vec![1],
            public_key: vec![2],
        };
        let signed_tx = SigningResponse::SignedTransaction {
            signer: signer.clone(),
            raw: vec![3],
        };
        let submitted = SigningResponse::SubmittedTransaction {
            signer,
            tx_hash: "0xabc".to_string(),
        };
        assert!(serde_json::to_string(&signature_only)
            .unwrap()
            .contains("signature_only"));
        assert!(serde_json::to_string(&signed_tx)
            .unwrap()
            .contains("signed_transaction"));
        assert!(serde_json::to_string(&submitted)
            .unwrap()
            .contains("submitted_transaction"));
    }

    #[test]
    fn signer_ref_round_trips_through_serde() {
        let signer_ref = SignerRef {
            id: SignerId::from_str("mpc").unwrap(),
        };
        let json = serde_json::to_string(&signer_ref).unwrap();
        let decoded: SignerRef = serde_json::from_str(&json).unwrap();
        assert_eq!(signer_ref, decoded);
    }
}
