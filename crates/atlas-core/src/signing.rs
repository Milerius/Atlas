use crate::{
    chain::Curve,
    error::SigningError,
    id::{AccountRef, NetworkId, SignerId},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignerRef {
    pub id: SignerId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SigningRequest {
    pub account: AccountRef,
    pub network: NetworkId,
    pub curve: Curve,
    #[serde(rename = "payloadKind")]
    pub payload_kind: SigningPayloadKind,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningPayloadKind {
    TransactionDigest,
    UnsignedTransaction,
    Message,
    TypedData,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningResponse {
    SignatureOnly {
        signer: SignerId,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    },
    SignedTransaction {
        signer: SignerId,
        raw: Vec<u8>,
    },
    SubmittedTransaction {
        signer: SignerId,
        tx_hash: String,
    },
}

#[async_trait]
pub trait SignerProvider: Send + Sync {
    fn id(&self) -> &SignerId;

    async fn sign(&self, request: SigningRequest) -> Result<SigningResponse, SigningError>;
}

#[derive(Clone, Debug)]
pub struct MockSigner {
    id: SignerId,
}

impl MockSigner {
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
