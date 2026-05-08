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
        let expected_prefix = format!("{}/", network.as_str());
        if !intent
            .asset_instance_id
            .as_str()
            .starts_with(&expected_prefix)
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
        id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId},
        signing::{MockSigner, SignerProvider},
        transaction::{SignedTransaction, TransferIntent, UnsignedTransaction},
    };
    use num_bigint::BigInt;
    use std::str::FromStr;

    #[tokio::test]
    async fn mock_evm_service_prepares_signs_and_broadcasts_exact_asset_instance() {
        let service = MockEvmService;
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let intent = TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:8453/erc20:0x8335").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(100_000_000u64), 6).unwrap(),
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

    // Regression: a network id like "eip155:1" must not match an asset
    // instance whose CAIP path begins with "eip155:10/...". The separator
    // '/' must immediately follow the network id.
    #[tokio::test]
    async fn prepare_transfer_rejects_partial_network_prefix_match() {
        let service = MockEvmService;
        let intent = TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:10/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        };
        let err = service
            .prepare_transfer(
                AccountRef::from_str("account-1").unwrap(),
                NetworkId::from_str("eip155:1").unwrap(),
                intent,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
    }

    fn unsigned_for_base() -> UnsignedTransaction {
        UnsignedTransaction {
            account: AccountRef::from_str("account-1").unwrap(),
            network: NetworkId::from_str("eip155:8453").unwrap(),
            intent: TransferIntent {
                asset_instance_id: AssetInstanceId::from_str("eip155:8453/native:eth").unwrap(),
                to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
                amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
            },
            payload: b"mock-unsigned-evm-transaction".to_vec(),
        }
    }

    #[test]
    fn assemble_signed_transaction_passes_through_signed_transaction_variant() {
        let service = MockEvmService;
        let unsigned = unsigned_for_base();
        let signed = service
            .assemble_signed_transaction(
                unsigned,
                SigningResponse::SignedTransaction {
                    signer: SignerId::from_str("mock").unwrap(),
                    raw: b"raw-tx".to_vec(),
                },
            )
            .unwrap();
        assert_eq!(signed.raw, b"raw-tx");
    }

    #[test]
    fn assemble_signed_transaction_rejects_submitted_transaction() {
        let service = MockEvmService;
        let unsigned = unsigned_for_base();
        let err = service
            .assemble_signed_transaction(
                unsigned,
                SigningResponse::SubmittedTransaction {
                    signer: SignerId::from_str("mock").unwrap(),
                    tx_hash: "0xabc".to_string(),
                },
            )
            .unwrap_err();
        assert!(matches!(err, ChainError::TransactionBuildFailed(_)));
    }

    #[tokio::test]
    async fn broadcast_rejects_empty_signed_transaction() {
        let service = MockEvmService;
        let signed = SignedTransaction {
            network: NetworkId::from_str("eip155:8453").unwrap(),
            raw: vec![],
        };
        let err = service.broadcast(signed).await.unwrap_err();
        assert!(matches!(err, ChainError::BroadcastFailed(_)));
    }
}
