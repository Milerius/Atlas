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
