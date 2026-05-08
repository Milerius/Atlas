use crate::{
    chain::Curve,
    error::ChainError,
    id::{AccountRef, NetworkId},
    signing::{SigningPayloadKind, SigningRequest, SigningResponse},
    transaction::{BroadcastResult, SignedTransaction, TransferIntent, UnsignedTransaction},
};
use async_trait::async_trait;

#[async_trait]
pub trait ChainService: Send + Sync {
    async fn prepare_transfer(
        &self,
        account: AccountRef,
        network: NetworkId,
        intent: TransferIntent,
    ) -> Result<UnsignedTransaction, ChainError>;

    fn signing_request(&self, unsigned: &UnsignedTransaction)
        -> Result<SigningRequest, ChainError>;

    fn assemble_signed_transaction(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError>;

    async fn broadcast(&self, signed: SignedTransaction) -> Result<BroadcastResult, ChainError>;
}

#[derive(Clone, Debug, Default)]
pub struct MockEvmService;

#[async_trait]
impl ChainService for MockEvmService {
    async fn prepare_transfer(
        &self,
        account: AccountRef,
        network: NetworkId,
        intent: TransferIntent,
    ) -> Result<UnsignedTransaction, ChainError> {
        if !intent
            .asset_instance_id
            .as_str()
            .starts_with(network.as_str())
        {
            return Err(ChainError::UnsupportedAssetInstance(
                intent.asset_instance_id,
            ));
        }
        Ok(UnsignedTransaction {
            account,
            network,
            intent,
            payload: b"mock-unsigned-evm-transaction".to_vec(),
        })
    }

    fn signing_request(
        &self,
        unsigned: &UnsignedTransaction,
    ) -> Result<SigningRequest, ChainError> {
        Ok(SigningRequest {
            account: unsigned.account.clone(),
            network: unsigned.network.clone(),
            curve: Curve::Secp256k1,
            payload_kind: SigningPayloadKind::TransactionDigest,
            payload: b"mock-digest".to_vec(),
        })
    }

    fn assemble_signed_transaction(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError> {
        match response {
            SigningResponse::SignatureOnly { signature, .. } => {
                let mut raw = unsigned.payload;
                raw.extend(signature);
                Ok(SignedTransaction {
                    network: unsigned.network,
                    raw,
                })
            }
            SigningResponse::SignedTransaction { raw, .. } => Ok(SignedTransaction {
                network: unsigned.network,
                raw,
            }),
            SigningResponse::SubmittedTransaction { tx_hash, .. } => {
                Err(ChainError::TransactionBuildFailed(format!(
                    "mock service expected signed bytes, got submitted hash {tx_hash}"
                )))
            }
        }
    }

    async fn broadcast(&self, signed: SignedTransaction) -> Result<BroadcastResult, ChainError> {
        if signed.raw.is_empty() {
            return Err(ChainError::BroadcastFailed(
                "empty signed transaction".to_string(),
            ));
        }
        Ok(BroadcastResult {
            tx_hash: "0xmock".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        amount::RawAmount,
        id::{AccountRef, AssetInstanceId, NetworkId, SignerId},
        signing::{MockSigner, SignerProvider},
        transaction::TransferIntent,
    };
    use num_bigint::BigInt;
    use std::str::FromStr;

    #[tokio::test]
    async fn mock_evm_service_prepares_signs_and_broadcasts_exact_asset_instance() {
        let service = MockEvmService;
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let intent = TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:8453/erc20:0x8335").unwrap(),
            to: "0x0000000000000000000000000000000000000001".to_string(),
            amount: RawAmount::new(BigInt::from(100_000_000u64), 6),
        };

        let unsigned = service
            .prepare_transfer(
                AccountRef::from_str("account-1").unwrap(),
                NetworkId::from_str("eip155:8453").unwrap(),
                intent,
            )
            .await
            .unwrap();
        let request = service.signing_request(&unsigned).unwrap();
        let response = signer.sign(request).await.unwrap();
        let signed = service
            .assemble_signed_transaction(unsigned, response)
            .unwrap();
        let broadcast = service.broadcast(signed).await.unwrap();

        assert_eq!(broadcast.tx_hash, "0xmock");
    }
}
