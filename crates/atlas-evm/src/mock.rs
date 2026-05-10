//! In-tree mock EVM chain service.
//!
//! [`MockEvmChainService`] is a single struct that implements all five
//! [`atlas_core::service`] traits with deterministic placeholder bytes and a
//! `0xmock` tx hash. It exists so the smoke flow + BDD scenarios can exercise
//! the full pipeline shape without an EVM RLP encoder or RPC dependency.
//!
//! Real EVM implementations live in [`crate::codec`], [`crate::reader`],
//! [`crate::fee_estimator`], [`crate::broadcaster`], and [`crate::service`].

use async_trait::async_trait;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetInstance;
use atlas_core::chain::Curve;
use atlas_core::error::ChainError;
use atlas_core::fee::{EvmFee, TransactionStatus};
use atlas_core::id::{AccountRef, AddressRef, NetworkId};
use atlas_core::service::{ChainBroadcaster, ChainCodec, ChainReader, ChainService, FeeEstimator};
use atlas_core::signing::{SigningPayloadKind, SigningRequest, SigningResponse};
use atlas_core::transaction::{
    BroadcastResult, SignedTransaction, TransferIntent, UnsignedTransaction,
};
use num_bigint::BigInt;

#[derive(Clone, Debug, Default)]
pub struct MockEvmChainService;

/// Mock context for [`MockEvmChainService::prepare_transfer`]. Mirrors the
/// shape of the real EVM `PrepareContext` so call sites don't need to
/// change when migrating from mock to real.
#[derive(Clone, Debug)]
pub struct MockEvmPrepareContext {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    pub chain_id: u64,
    pub nonce: u64,
    pub fee: EvmFee,
}

impl ChainCodec for MockEvmChainService {
    type PrepareContext = MockEvmPrepareContext;

    fn prepare_transfer(
        &self,
        ctx: MockEvmPrepareContext,
    ) -> Result<UnsignedTransaction, ChainError> {
        let expected_prefix = format!("{}/", ctx.network.as_str());
        if !ctx
            .intent
            .asset_instance_id
            .as_str()
            .starts_with(&expected_prefix)
        {
            return Err(ChainError::UnsupportedAssetInstance(
                ctx.intent.asset_instance_id,
            ));
        }
        let _ = (ctx.chain_id, ctx.nonce, ctx.fee);
        Ok(UnsignedTransaction {
            account: ctx.account,
            network: ctx.network,
            intent: ctx.intent,
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

    fn assemble_signed(
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
}

#[async_trait]
impl ChainReader for MockEvmChainService {
    async fn get_balance(
        &self,
        instance: &AssetInstance,
        _address: &AddressRef,
    ) -> Result<RawAmount, ChainError> {
        let one_unit = BigInt::from(10u64).pow(instance.decimals as u32);
        // `10^n` is non-negative for any `n: u8`, so `RawAmount::new`
        // cannot fail here.
        Ok(RawAmount::new(one_unit, instance.decimals).expect("10^n is non-negative"))
    }

    async fn get_nonce(
        &self,
        _network: &NetworkId,
        _address: &AddressRef,
    ) -> Result<u64, ChainError> {
        Ok(0)
    }

    async fn get_transaction_status(
        &self,
        _network: &NetworkId,
        hash: &str,
    ) -> Result<TransactionStatus, ChainError> {
        Ok(TransactionStatus::Confirmed {
            hash: hash.to_string(),
            block_number: 1,
            gas_used: BigInt::from(21_000u64),
        })
    }
}

#[async_trait]
impl FeeEstimator for MockEvmChainService {
    type Fee = EvmFee;

    async fn estimate_fee(
        &self,
        _intent: &TransferIntent,
        _sender: &AddressRef,
    ) -> Result<EvmFee, ChainError> {
        Ok(EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(2_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        })
    }
}

#[async_trait]
impl ChainBroadcaster for MockEvmChainService {
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

#[async_trait]
impl ChainService for MockEvmChainService {
    type PrepareContext = MockEvmPrepareContext;
    type Fee = EvmFee;

    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn atlas_core::signing::SignerProvider,
    ) -> Result<BroadcastResult, ChainError> {
        // Static literal: `AddressRef::new` only rejects empty / whitespace.
        let sender = AddressRef::new("0xmocksender").expect("static literal is non-empty");
        // Mock convention: derive the network from the CAIP-style asset
        // instance id. `split_once('/')` returns None when the instance id
        // has no `/` separator (a malformed input the registry would
        // never produce, but tests can construct directly).
        let (network_str, _) = intent
            .asset_instance_id
            .as_str()
            .split_once('/')
            .ok_or_else(|| {
                ChainError::UnsupportedAssetInstance(intent.asset_instance_id.clone())
            })?;
        let network = NetworkId::new(network_str)
            .map_err(|_| ChainError::UnsupportedAssetInstance(intent.asset_instance_id.clone()))?;

        let nonce = self.get_nonce(&network, &sender).await?;
        let fee = self.estimate_fee(&intent, &sender).await?;

        let unsigned = self.prepare_transfer(MockEvmPrepareContext {
            account,
            network,
            intent,
            chain_id: 1,
            nonce,
            fee,
        })?;

        let request = self.signing_request(&unsigned)?;
        let response = signer.sign(request).await?;
        let signed = self.assemble_signed(unsigned, response)?;
        self.broadcast(signed).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::asset::AssetStandard;
    use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
    use atlas_core::signing::MockSigner;
    use std::str::FromStr;

    fn intent(instance_id: &str) -> TransferIntent {
        TransferIntent {
            asset_instance_id: AssetInstanceId::new(instance_id).unwrap(),
            to: AddressRef::new("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        }
    }

    fn mock_ctx(network_str: &str, instance_id: &str) -> MockEvmPrepareContext {
        MockEvmPrepareContext {
            account: AccountRef::from_str("account-1").unwrap(),
            network: NetworkId::from_str(network_str).unwrap(),
            intent: intent(instance_id),
            chain_id: 1,
            nonce: 0,
            fee: EvmFee::Legacy {
                gas_price: BigInt::from(1u64),
                gas_limit: 21_000,
            },
        }
    }

    #[tokio::test]
    async fn mock_transfer_returns_0xmock() {
        let svc = MockEvmChainService;
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let result = svc
            .transfer(
                intent("eip155:1/native:eth"),
                AccountRef::from_str("account-1").unwrap(),
                &signer,
            )
            .await
            .unwrap();
        assert_eq!(result.tx_hash, "0xmock");
    }

    #[test]
    fn prepare_transfer_rejects_partial_network_prefix() {
        let svc = MockEvmChainService;
        let ctx = mock_ctx("eip155:1", "eip155:10/native:eth");
        let err = svc.prepare_transfer(ctx).unwrap_err();
        assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
    }

    #[tokio::test]
    async fn broadcast_rejects_empty_raw() {
        let svc = MockEvmChainService;
        let signed = SignedTransaction {
            network: NetworkId::from_str("eip155:1").unwrap(),
            raw: vec![],
        };
        let err = svc.broadcast(signed).await.unwrap_err();
        assert!(matches!(err, ChainError::BroadcastFailed(_)));
    }

    #[test]
    fn assemble_signed_rejects_submitted_variant() {
        let svc = MockEvmChainService;
        let unsigned = UnsignedTransaction {
            account: AccountRef::from_str("account-1").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            intent: intent("eip155:1/native:eth"),
            payload: b"mock".to_vec(),
        };
        let response = SigningResponse::SubmittedTransaction {
            signer: SignerId::from_str("mock").unwrap(),
            tx_hash: "0xabc".to_string(),
        };
        let err = svc.assemble_signed(unsigned, response).unwrap_err();
        assert!(matches!(err, ChainError::TransactionBuildFailed(_)));
    }

    #[tokio::test]
    async fn reader_returns_one_unit_balance() {
        let svc = MockEvmChainService;
        let instance = AssetInstance {
            id: AssetInstanceId::new("eip155:1/native:eth").unwrap(),
            instrument_id: atlas_core::id::AssetInstrumentId::new("eth.native").unwrap(),
            network: NetworkId::new("eip155:1").unwrap(),
            standard: AssetStandard::Native,
            decimals: 18,
            contract: None,
            capabilities: vec![],
            metadata: atlas_core::asset::AssetMetadata::default(),
        };
        let balance = svc
            .get_balance(&instance, &AddressRef::new("0xanyone").unwrap())
            .await
            .unwrap();
        assert_eq!(balance.value().to_string(), "1000000000000000000");
        assert_eq!(balance.decimals(), 18);
    }

    #[test]
    fn assemble_signed_passes_through_signed_transaction_variant() {
        let svc = MockEvmChainService;
        let unsigned = UnsignedTransaction {
            account: AccountRef::from_str("account-1").unwrap(),
            network: NetworkId::from_str("eip155:1").unwrap(),
            intent: intent("eip155:1/native:eth"),
            payload: b"mock".to_vec(),
        };
        let response = SigningResponse::SignedTransaction {
            signer: SignerId::from_str("mock").unwrap(),
            raw: vec![0xde, 0xad, 0xbe, 0xef],
        };
        let signed = svc.assemble_signed(unsigned, response).unwrap();
        assert_eq!(signed.raw, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[tokio::test]
    async fn get_transaction_status_returns_confirmed_with_input_hash() {
        let svc = MockEvmChainService;
        let net = NetworkId::from_str("eip155:1").unwrap();
        let status = svc.get_transaction_status(&net, "0xabc").await.unwrap();
        match status {
            TransactionStatus::Confirmed {
                hash,
                block_number,
                gas_used,
            } => {
                assert_eq!(hash, "0xabc");
                assert_eq!(block_number, 1);
                assert_eq!(gas_used.to_string(), "21000");
            }
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn transfer_with_malformed_instance_id_returns_unsupported_asset() {
        // No `/` separator → split_once returns None → mapped to
        // UnsupportedAssetInstance.
        let svc = MockEvmChainService;
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let intent = TransferIntent {
            asset_instance_id: AssetInstanceId::new("noslash").unwrap(),
            to: AddressRef::new("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        };
        let err = svc
            .transfer(intent, AccountRef::from_str("account-1").unwrap(), &signer)
            .await
            .unwrap_err();
        assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
    }

    #[tokio::test]
    async fn transfer_with_empty_network_segment_returns_unsupported_asset() {
        // Leading `/` → split_once succeeds with empty network_str →
        // NetworkId::new fails → mapped to UnsupportedAssetInstance.
        let svc = MockEvmChainService;
        let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
        let intent = TransferIntent {
            asset_instance_id: AssetInstanceId::new("/native:eth").unwrap(),
            to: AddressRef::new("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        };
        let err = svc
            .transfer(intent, AccountRef::from_str("account-1").unwrap(), &signer)
            .await
            .unwrap_err();
        assert!(matches!(err, ChainError::UnsupportedAssetInstance(_)));
    }
}
