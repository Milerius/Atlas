# Atlas ChainService EVM Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor atlas-core's `ChainService` into 5 focused traits, then ship the first real implementation: `atlas-evm` on top of [alloy](https://github.com/alloy-rs/alloy), and a reference signer `atlas-signer-localkey` with three construction paths (raw key, JSON keystore, BIP-32 HD derivation).

**Architecture:** Five-trait split (`ChainCodec`, `ChainReader`, `FeeEstimator`, `ChainBroadcaster`, `ChainService`) keeps the pure-encoding seam separate from RPC-dependent seams. `atlas-evm` impls each trait against alloy primitives (`alloy-consensus::TxEip1559`/`TxLegacy`, `alloy-provider::RootProvider`, `alloy-signer-local::PrivateKeySigner`). `atlas-signer-localkey` wraps `alloy-signer-local::PrivateKeySigner` and adds keystore + HD derivation paths.

**Tech Stack:**
- Rust 2021 edition (workspace already set)
- `alloy-consensus`, `alloy-rlp`, `alloy-primitives`, `alloy-network`, `alloy-provider`, `alloy-signer`, `alloy-signer-local`, `alloy-rpc-types-eth`
- `eth-keystore` (Web3 secret-storage decryption)
- `bip32` (BIP-32 HD derivation), `bip39` (mnemonic → seed)
- Workspace existing: `async-trait`, `num-bigint`, `serde`, `serde_json`, `thiserror`, `tokio`

**Source spec:** [`docs/superpowers/specs/2026-05-08-atlas-chain-service-evm-design.md`](../specs/2026-05-08-atlas-chain-service-evm-design.md)

**Branch strategy:** Implementation lands on a new feature branch (e.g. `feat/evm-chain-service`) cut from `main`, NOT from this docs branch. The docs branch (`docs/evm-chain-service-design`) merges first as a small docs-only PR; implementation PR follows.

---

## File structure

### Modified

- `Cargo.toml` (workspace) — add new members + alloy/keystore/bip deps
- `deny.toml` — extend license allow-list if new transitives bring license types not yet allowed
- `README.md` — update crates table, verification highlights
- `crates/atlas-core/src/lib.rs` — add `pub mod fee`, re-export `Fee`, `EvmFee`, `TransactionStatus`
- `crates/atlas-core/src/error.rs` — add 3 `ChainError` variants
- `crates/atlas-core/src/service.rs` — replace single trait with 5; rewrite `MockEvm*` impls
- `crates/atlas-core/tests/smoke_flow.rs` — adapt to 5-trait shape
- `crates/atlas-scenarios/src/world.rs` — adapt to `MockEvmChainService`
- `crates/atlas-scenarios/src/steps.rs` — adapt to `MockEvmChainService`
- `.github/workflows/ci.yml` — already excludes `atlas-scenarios` from coverage; verify the new heavy crates land cleanly

### Created (atlas-core internal)

- `crates/atlas-core/src/fee.rs` — `Fee`, `EvmFee`, `TransactionStatus`

### Created (atlas-evm crate)

- `crates/atlas-evm/Cargo.toml`
- `crates/atlas-evm/src/lib.rs`
- `crates/atlas-evm/src/codec.rs` — `EvmCodec`, `EvmPrepareContext`
- `crates/atlas-evm/src/abi.rs` — minimal ERC-20 `transfer(address,uint256)` selector + encoding
- `crates/atlas-evm/src/reader.rs` — `EvmReader<P>`
- `crates/atlas-evm/src/fee_estimator.rs` — `EvmFeeEstimator<P>`
- `crates/atlas-evm/src/broadcaster.rs` — `EvmBroadcaster<P>`
- `crates/atlas-evm/src/service.rs` — `EvmChainService<P>` orchestrator
- `crates/atlas-evm/src/error.rs` — alloy → `ChainError` conversions
- `crates/atlas-evm/tests/codec_native.rs`
- `crates/atlas-evm/tests/codec_erc20.rs`
- `crates/atlas-evm/tests/codec_assemble.rs`
- `crates/atlas-evm/tests/end_to_end.rs`

### Created (atlas-signer-localkey crate)

- `crates/atlas-signer-localkey/Cargo.toml`
- `crates/atlas-signer-localkey/src/lib.rs` — `LocalKeySigner` + 3 constructors + `SignerProvider` impl
- `crates/atlas-signer-localkey/tests/raw_key.rs`
- `crates/atlas-signer-localkey/tests/keystore.rs`
- `crates/atlas-signer-localkey/tests/hd.rs`

---

## Phase 1: atlas-core refactor

### Task 1.1: Create `fee.rs` with `Fee`, `EvmFee`, `TransactionStatus`

**Files:**
- Create: `crates/atlas-core/src/fee.rs`
- Modify: `crates/atlas-core/src/lib.rs`

- [ ] **Step 1: Write the failing test in `fee.rs`**

Create `crates/atlas-core/src/fee.rs`:

```rust
//! Per-chain fee shapes.
//!
//! [`Fee`] is the chain-agnostic top-level enum used by higher product layers
//! that aggregate fees across networks. Each chain family's
//! [`crate::service::FeeEstimator::Fee`] associated type points at the
//! appropriate sibling enum (e.g. [`EvmFee`]).

use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Fee {
    Evm(EvmFee),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EvmFee {
    Legacy {
        gas_price: BigInt,
        gas_limit: u64,
    },
    Eip1559 {
        max_fee_per_gas: BigInt,
        max_priority_fee_per_gas: BigInt,
        gas_limit: u64,
        l1_fee_wei: Option<BigInt>,
    },
}

impl EvmFee {
    /// Total worst-case fee in wei: `gas_limit * effective_price + l1_fee`.
    /// `l1_fee` is non-zero only on OP-Stack networks.
    pub fn max_fee_wei(&self) -> BigInt {
        match self {
            EvmFee::Legacy { gas_price, gas_limit } => gas_price * BigInt::from(*gas_limit),
            EvmFee::Eip1559 {
                max_fee_per_gas,
                gas_limit,
                l1_fee_wei,
                ..
            } => {
                let l2 = max_fee_per_gas * BigInt::from(*gas_limit);
                match l1_fee_wei {
                    Some(l1) => l2 + l1,
                    None => l2,
                }
            }
        }
    }
}

/// Lifecycle of a broadcast transaction. Returned by
/// [`crate::service::ChainReader::get_transaction_status`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending {
        hash: String,
    },
    Confirmed {
        hash: String,
        block_number: u64,
        gas_used: BigInt,
    },
    Failed {
        hash: String,
        reason: String,
    },
    NotFound {
        hash: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_max_fee_is_gas_price_times_gas_limit() {
        let fee = EvmFee::Legacy {
            gas_price: BigInt::from(20_000_000_000u64),
            gas_limit: 21_000,
        };
        assert_eq!(
            fee.max_fee_wei(),
            BigInt::from(20_000_000_000u64) * BigInt::from(21_000u64)
        );
    }

    #[test]
    fn eip1559_max_fee_no_l1() {
        let fee = EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 50_000,
            l1_fee_wei: None,
        };
        assert_eq!(
            fee.max_fee_wei(),
            BigInt::from(30_000_000_000u64) * BigInt::from(50_000u64)
        );
    }

    #[test]
    fn eip1559_max_fee_includes_l1() {
        let fee = EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 50_000,
            l1_fee_wei: Some(BigInt::from(123u64)),
        };
        let expected = BigInt::from(30_000_000_000u64) * BigInt::from(50_000u64) + BigInt::from(123u64);
        assert_eq!(fee.max_fee_wei(), expected);
    }
}
```

- [ ] **Step 2: Add the module to `lib.rs`**

Modify `crates/atlas-core/src/lib.rs` — add `pub mod fee;` between `pub mod error;` and `pub mod id;`, and extend the re-exports:

```rust
pub mod amount;
pub mod asset;
pub mod chain;
pub mod error;
pub mod fee;       // NEW
pub mod id;
pub mod official;
pub mod registry;
pub mod service;
pub mod signing;
pub mod transaction;

pub use amount::{AmountError, RawAmount};
pub use error::{AssetError, ChainError, RegistryError, RpcError, SigningError};
pub use fee::{EvmFee, Fee, TransactionStatus};   // NEW
```

- [ ] **Step 3: Run the test**

```bash
cargo test -p atlas-core --lib fee::tests
```

Expected: `test result: ok. 3 passed`.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-core/src/fee.rs crates/atlas-core/src/lib.rs
git commit -m "feat(core): add Fee, EvmFee, TransactionStatus types"
```

---

### Task 1.2: Extend `ChainError` with new variants

**Files:**
- Modify: `crates/atlas-core/src/error.rs`

- [ ] **Step 1: Add the variants**

In `crates/atlas-core/src/error.rs`, find the `ChainError` enum and add three variants. The existing imports already cover `NetworkId`, `AssetInstanceId`. Add `RpcError` reference (it's defined in the same file — variants reference it directly without import). Add `AssetStandard` import:

At the top of `error.rs`, replace the import line with:

```rust
use crate::asset::AssetStandard;
use crate::id::{AssetGroupId, AssetInstanceId, AssetInstrumentId, ChainId, NetworkId, SignerId};
```

In the `ChainError` enum, append three variants before the closing brace:

```rust
    /// RPC transport / provider error encountered by a chain service.
    #[error("rpc error: {0}")]
    Rpc(#[from] RpcError),
    /// `Network.chain_id` was missing, empty, or non-numeric on a network
    /// that requires it (currently EVM).
    #[error("missing chain id on network: {0}")]
    MissingChainId(NetworkId),
    /// The asset's standard is supported by the registry but not by this
    /// chain service (e.g. SPL passed to the EVM service).
    #[error("asset standard {standard:?} not supported for instance {instance}")]
    StandardNotSupported {
        instance: AssetInstanceId,
        standard: AssetStandard,
    },
```

- [ ] **Step 2: Verify it compiles and existing tests still pass**

```bash
cargo test -p atlas-core --lib
```

Expected: all existing tests pass; no new tests added in this task.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-core/src/error.rs
git commit -m "feat(core): extend ChainError with Rpc / MissingChainId / StandardNotSupported"
```

---

### Task 1.3: Replace `service.rs` with the 5-trait split + `MockEvm*` impls

**Files:**
- Replace: `crates/atlas-core/src/service.rs`

This task fully rewrites `service.rs`. The current file has the single `ChainService` trait + `MockEvmService` impl + tests; everything is replaced.

- [ ] **Step 1: Replace the contents of `crates/atlas-core/src/service.rs`**

```rust
//! Per-chain-family transaction lifecycle, split into 5 focused traits.
//!
//! - [`ChainCodec`] — pure: encode / sign-request / assemble. No RPC, no I/O.
//! - [`ChainReader`] — RPC reads: balance, nonce, tx status.
//! - [`FeeEstimator`] — RPC: estimate fee for an intent (per-chain `Fee` type).
//! - [`ChainBroadcaster`] — RPC: send a signed transaction.
//! - [`ChainService`] — orchestrator that composes the four above for the
//!   happy-path transfer flow.
//!
//! The split keeps the pure-encoding seam separate from RPC-dependent seams,
//! so a server can build transactions offline (codec only) and a client can
//! sign with MPC / Privy / local key.
//!
//! [`MockEvmCodec`], [`MockEvmReader`], [`MockEvmFeeEstimator`],
//! [`MockEvmBroadcaster`], and [`MockEvmChainService`] are in-tree smoke
//! implementations — `MockEvmChainService` is one struct that implements all
//! 5 traits for ergonomic single-import testing.

use crate::{
    amount::RawAmount,
    asset::{AssetInstance, AssetStandard},
    chain::Curve,
    error::ChainError,
    fee::{EvmFee, TransactionStatus},
    id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId},
    signing::{SigningPayloadKind, SigningRequest, SigningResponse},
    transaction::{BroadcastResult, SignedTransaction, TransferIntent, UnsignedTransaction},
};
use async_trait::async_trait;
use num_bigint::BigInt;

// ── ChainCodec ──────────────────────────────────────────────────────────────

/// Pure transaction codec — encode an intent into chain-specific bytes,
/// produce a signing request, and assemble a signed transaction from
/// whichever [`SigningResponse`] shape the signer returned.
///
/// No RPC, no I/O. Implementations are `Send + Sync`.
pub trait ChainCodec: Send + Sync {
    /// Per-chain context needed to encode a transfer.
    /// Different chains require different inputs (EVM: chain_id + nonce + fee;
    /// Solana: recent_blockhash + compute units; UTXO: UTXO selection).
    type PrepareContext;

    fn prepare_transfer(
        &self,
        ctx: Self::PrepareContext,
    ) -> Result<UnsignedTransaction, ChainError>;

    fn signing_request(
        &self,
        unsigned: &UnsignedTransaction,
    ) -> Result<SigningRequest, ChainError>;

    fn assemble_signed(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError>;
}

// ── ChainReader ─────────────────────────────────────────────────────────────

/// RPC reads: balance, nonce, transaction status. Does not write.
#[async_trait]
pub trait ChainReader: Send + Sync {
    async fn get_balance(
        &self,
        instance: &AssetInstance,
        address: &AddressRef,
    ) -> Result<RawAmount, ChainError>;

    async fn get_nonce(
        &self,
        network: &NetworkId,
        address: &AddressRef,
    ) -> Result<u64, ChainError>;

    async fn get_transaction_status(
        &self,
        network: &NetworkId,
        hash: &str,
    ) -> Result<TransactionStatus, ChainError>;
}

// ── FeeEstimator ────────────────────────────────────────────────────────────

/// Estimate the per-chain `Fee` for a transfer intent. The associated type
/// allows EVM to return [`crate::fee::EvmFee`] directly without going through
/// the [`crate::fee::Fee`] wrapper.
#[async_trait]
pub trait FeeEstimator: Send + Sync {
    type Fee;

    async fn estimate_fee(
        &self,
        intent: &TransferIntent,
        sender: &AddressRef,
    ) -> Result<Self::Fee, ChainError>;
}

// ── ChainBroadcaster ────────────────────────────────────────────────────────

/// Submit a [`SignedTransaction`] to the network's RPC.
#[async_trait]
pub trait ChainBroadcaster: Send + Sync {
    async fn broadcast(
        &self,
        signed: SignedTransaction,
    ) -> Result<BroadcastResult, ChainError>;
}

// ── ChainService ────────────────────────────────────────────────────────────

/// Orchestrator that composes [`ChainCodec`] + [`ChainReader`] +
/// [`FeeEstimator`] + [`ChainBroadcaster`] + a [`crate::signing::SignerProvider`]
/// into the happy-path transfer flow.
///
/// Apps that need fine control compose the four sub-traits directly; this
/// trait is for the common case.
#[async_trait]
pub trait ChainService: Send + Sync {
    type PrepareContext;
    type Fee;

    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn crate::signing::SignerProvider,
    ) -> Result<BroadcastResult, ChainError>;
}

// ── MockEvmChainService ─────────────────────────────────────────────────────

/// In-tree mock that implements all 5 traits. Returns deterministic
/// placeholder bytes and a `0xmock` tx hash so the smoke flow + BDD scenarios
/// can exercise the full pipeline shape without an EVM RLP encoder or RPC
/// dependency.
///
/// Real EVM implementations live in the `atlas-evm` crate.
#[derive(Clone, Debug, Default)]
pub struct MockEvmChainService;

/// Mock context for [`MockEvmChainService::prepare_transfer`]. Mirrors the
/// shape of the future real EVM `PrepareContext` so call sites don't need
/// to change when migrating from mock to real.
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
        // Mock keeps the existing network-prefix exact-segment rule so this
        // smoke service still catches the bug we have a property test for.
        let expected_prefix = format!("{}/", ctx.network.as_str());
        if !ctx.intent.asset_instance_id.as_str().starts_with(&expected_prefix) {
            return Err(ChainError::UnsupportedAssetInstance(ctx.intent.asset_instance_id));
        }
        let _ = (ctx.chain_id, ctx.nonce, ctx.fee); // mock ignores numeric fields
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
                Ok(SignedTransaction { network: unsigned.network, raw })
            }
            SigningResponse::SignedTransaction { raw, .. } => {
                Ok(SignedTransaction { network: unsigned.network, raw })
            }
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
        // Deterministic mock balance: 1 unit at the instance's decimals.
        let one_unit = BigInt::from(10u64).pow(instance.decimals as u32);
        RawAmount::new(one_unit, instance.decimals)
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))
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
    async fn broadcast(
        &self,
        signed: SignedTransaction,
    ) -> Result<BroadcastResult, ChainError> {
        if signed.raw.is_empty() {
            return Err(ChainError::BroadcastFailed("empty signed transaction".to_string()));
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
        signer: &dyn crate::signing::SignerProvider,
    ) -> Result<BroadcastResult, ChainError> {
        // Mock derives a fake sender address; real impls compute from signer pubkey.
        let sender = AddressRef::new("0xmocksender")
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;
        // Resolve the network from the intent (mock convention: prefix before '/').
        let network_str = intent
            .asset_instance_id
            .as_str()
            .split('/')
            .next()
            .ok_or_else(|| ChainError::UnsupportedAssetInstance(intent.asset_instance_id.clone()))?;
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
        let response = signer.sign(request).await
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;
        let signed = self.assemble_signed(unsigned, response)?;
        self.broadcast(signed).await
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{AddressRef, AssetInstanceId, NetworkId, SignerId};
    use crate::signing::MockSigner;
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
            instrument_id: crate::id::AssetInstrumentId::new("eth.native").unwrap(),
            network: NetworkId::new("eip155:1").unwrap(),
            standard: AssetStandard::Native,
            decimals: 18,
            contract: None,
            capabilities: vec![],
            metadata: crate::asset::AssetMetadata::default(),
        };
        let balance = svc
            .get_balance(&instance, &AddressRef::new("0xanyone").unwrap())
            .await
            .unwrap();
        // 10^18 wei = 1 ETH
        assert_eq!(balance.value().to_string(), "1000000000000000000");
        assert_eq!(balance.decimals(), 18);
    }
}
```

- [ ] **Step 2: Run the unit tests**

```bash
cargo test -p atlas-core --lib service::tests
```

Expected: 5 tests pass.

- [ ] **Step 3: Run all atlas-core unit tests to confirm no regression**

```bash
cargo test -p atlas-core --lib
```

Expected: all unit tests pass (including the original 49 plus the new 5 service tests).

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-core/src/service.rs
git commit -m "refactor(core): split ChainService into 5 focused traits"
```

---

### Task 1.4: Update `smoke_flow.rs` to the new trait shape

**Files:**
- Modify: `crates/atlas-core/tests/smoke_flow.rs`

The existing smoke test uses `MockEvmService` and the old single-trait API. It needs to use `MockEvmChainService::transfer` for the orchestrator path, OR exercise the 5 traits directly. We'll do both: keep the existing structure but route through the new `MockEvmChainService::transfer`.

- [ ] **Step 1: Replace `crates/atlas-core/tests/smoke_flow.rs`**

```rust
mod common;

use atlas_core::{
    amount::RawAmount,
    id::{AccountRef, AddressRef, SignerId},
    service::{ChainService, MockEvmChainService},
    signing::MockSigner,
    transaction::TransferIntent,
};
use common::registry;
use num_bigint::BigInt;
use std::str::FromStr;

#[tokio::test]
async fn base_usdc_transfer_smoke_flow_uses_exact_asset_instance() {
    let registry = registry();
    let asset = registry
        .asset_instance("eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")
        .unwrap();

    let service = MockEvmChainService;
    let signer = MockSigner::new(SignerId::from_str("mock-signer").unwrap());
    let intent = TransferIntent {
        asset_instance_id: asset.id.clone(),
        to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
        amount: RawAmount::new(BigInt::from(100_000_000u64), asset.decimals).unwrap(),
    };

    let broadcast = service
        .transfer(intent, AccountRef::from_str("account-1").unwrap(), &signer)
        .await
        .unwrap();

    assert_eq!(broadcast.tx_hash, "0xmock");
}
```

- [ ] **Step 2: Run the smoke test**

```bash
cargo test -p atlas-core --test smoke_flow
```

Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-core/tests/smoke_flow.rs
git commit -m "test(core): smoke flow uses MockEvmChainService::transfer"
```

---

### Task 1.5: Update atlas-scenarios world + steps

**Files:**
- Modify: `crates/atlas-scenarios/src/world.rs`
- Modify: `crates/atlas-scenarios/src/steps.rs`

The BDD scenarios call into `MockEvmService` directly today. After the refactor they go through `MockEvmChainService::transfer`. The Gherkin feature files don't change.

- [ ] **Step 1: Replace `crates/atlas-scenarios/src/world.rs`**

```rust
//! Cucumber [`World`] for atlas-core BDD scenarios.

use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetInstance;
use atlas_core::error::ChainError;
use atlas_core::id::AssetInstanceId;
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
use atlas_core::service::MockEvmChainService;
use atlas_core::signing::MockSigner;
use atlas_core::transaction::BroadcastResult;
use cucumber::World;

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct AtlasWorld {
    pub registry: Option<Registry>,
    pub service: Option<MockEvmChainService>,
    pub signer: Option<MockSigner>,
    pub last_instance: Option<AssetInstance>,
    pub last_group_instances: Vec<AssetInstance>,
    pub last_broadcast: Option<BroadcastResult>,
    pub last_error: Option<ChainError>,
    pub scratch_amount: Option<RawAmount>,
    pub scratch_to: Option<String>,
    pub scratch_asset_instance_id: Option<AssetInstanceId>,
    pub scratch_network_id: Option<atlas_core::id::NetworkId>,
}

impl AtlasWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_valid_fixtures(&mut self) {
        let chain_doc: ChainRegistryDocument = serde_json::from_str(include_str!(
            "../../atlas-core/tests/fixtures/chain_registry.valid.json"
        ))
        .expect("valid chain fixture");
        let asset_doc: AssetRegistryDocument = serde_json::from_str(include_str!(
            "../../atlas-core/tests/fixtures/asset_registry.valid.json"
        ))
        .expect("valid asset fixture");
        self.registry =
            Some(Registry::from_documents(chain_doc, asset_doc).expect("registry validates"));
    }

    pub fn registry(&self) -> &Registry {
        self.registry
            .as_ref()
            .expect("registry not loaded — start with `Given a valid registry`")
    }
}

/// Run the orchestrator path: build a TransferIntent from the scratch fields
/// and call MockEvmChainService::transfer. Records broadcast result or error.
pub async fn run_transfer_pipeline(world: &mut AtlasWorld) {
    use atlas_core::id::{AccountRef, AddressRef};
    use atlas_core::service::ChainService;
    use atlas_core::transaction::TransferIntent;
    use std::str::FromStr;

    let asset_instance_id = world
        .scratch_asset_instance_id
        .clone()
        .expect("asset instance id not set");
    let network_id = world.scratch_network_id.clone().expect("network id not set");
    let amount = world.scratch_amount.clone().expect("amount not set");
    let to = world.scratch_to.clone().expect("recipient address not set");

    // Validate against the scratch network — replicates the old per-step
    // network check that previously lived inside MockEvmService::prepare_transfer.
    let prefix = format!("{}/", network_id.as_str());
    if !asset_instance_id.as_str().starts_with(&prefix) {
        world.last_error = Some(ChainError::UnsupportedAssetInstance(asset_instance_id));
        return;
    }

    let intent = TransferIntent {
        asset_instance_id,
        to: AddressRef::from_str(&to).expect("address ref valid"),
        amount,
    };

    let service = world.service.as_ref().expect("service");
    let signer = world.signer.as_ref().expect("signer");

    match service
        .transfer(intent, AccountRef::from_str("account-1").unwrap(), signer)
        .await
    {
        Ok(broadcast) => world.last_broadcast = Some(broadcast),
        Err(err) => world.last_error = Some(err),
    }
}
```

- [ ] **Step 2: Update `crates/atlas-scenarios/src/steps.rs`**

Find the `mock_service` step and replace `MockEvmService` with `MockEvmChainService`:

```rust
#[given("a mock EVM chain service")]
pub fn mock_service(world: &mut AtlasWorld) {
    world.service = Some(MockEvmChainService);
}
```

And update the import line at the top of `steps.rs`:

```rust
use atlas_core::service::MockEvmChainService;
```

(Replace any `use atlas_core::service::MockEvmService;` with the line above.)

- [ ] **Step 3: Run the BDD scenarios**

```bash
cargo test -p atlas-scenarios --test atlas_bdd
```

Expected: `7 scenarios (7 passed)`, `30 steps (30 passed)`.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-scenarios/src/world.rs crates/atlas-scenarios/src/steps.rs
git commit -m "test(scenarios): adapt BDD harness to MockEvmChainService"
```

---

### Task 1.6: Full atlas-core / atlas-scenarios / atlas-verify verification

- [ ] **Step 1: fmt + clippy**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: clean. Both commands exit 0 with no diagnostics.

- [ ] **Step 2: Full test suite**

```bash
cargo test --workspace --all-features
```

Expected: all tests pass. Total count is unchanged or +1/+2 from the new mock service tests.

- [ ] **Step 3: doc + deny + WASM**

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --all-features
cargo deny check
cargo build -p atlas-core --target wasm32-unknown-unknown
```

Expected: each command exits 0.

- [ ] **Step 4: Coverage sanity check**

```bash
cargo +nightly llvm-cov --workspace --all-features --ignore-filename-regex 'atlas-scenarios' --summary-only | tail -3
```

Expected: workspace coverage stays ≥99.5% (target ≥99.7%).

- [ ] **Step 5: Commit if any rustfmt-driven diffs surfaced**

```bash
git status
# if there are unstaged formatting changes:
git add -A
git commit -m "style: rustfmt"
```

---

## Phase 2: atlas-evm crate

### Task 2.1: Scaffold the crate + workspace deps

**Files:**
- Modify: `Cargo.toml` (workspace)
- Create: `crates/atlas-evm/Cargo.toml`
- Create: `crates/atlas-evm/src/lib.rs`

- [ ] **Step 1: Add alloy + new-crate workspace deps to root `Cargo.toml`**

In the root `Cargo.toml`, extend the `[workspace]` `members` array:

```toml
members = [
  "crates/atlas-core",
  "crates/atlas-evm",
  "crates/atlas-scenarios",
  "crates/atlas-signer-localkey",
  "crates/atlas-verify",
]
```

Add to `[workspace.dependencies]`:

```toml
alloy-consensus      = "0.5"
alloy-network        = "0.5"
alloy-primitives     = { version = "0.8", features = ["serde"] }
alloy-provider       = "0.5"
alloy-rlp            = "0.3"
alloy-rpc-types-eth  = "0.5"
alloy-signer         = "0.5"
alloy-signer-local   = { version = "0.5", features = ["mnemonic", "keystore"] }
bip32                = "0.5"
bip39                = "2"
eth-keystore         = "0.5"
```

> The `0.x` version pins are placeholders for the actual current versions — bump them to whatever `cargo add alloy-consensus` resolves to during execution. The features listed are what each crate needs for our use case.

- [ ] **Step 2: Create `crates/atlas-evm/Cargo.toml`**

```toml
[package]
name = "atlas-evm"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
alloy-consensus.workspace = true
alloy-network.workspace = true
alloy-primitives.workspace = true
alloy-provider.workspace = true
alloy-rlp.workspace = true
alloy-rpc-types-eth.workspace = true
alloy-signer.workspace = true
async-trait.workspace = true
atlas-core = { path = "../atlas-core" }
num-bigint.workspace = true
serde.workspace = true
thiserror.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }

[dev-dependencies]
alloy-signer-local = { workspace = true, features = ["mnemonic", "keystore"] }
serde_json.workspace = true

[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = [] }
```

- [ ] **Step 3: Create `crates/atlas-evm/src/lib.rs`**

```rust
//! Real EVM `ChainService` for the Atlas SDK, built on alloy.
//!
//! Implements all 5 traits from `atlas_core::service`:
//! - [`codec::EvmCodec`] — pure RLP encoding (legacy + EIP-1559)
//! - [`reader::EvmReader`] — RPC reads via `alloy-provider`
//! - [`fee_estimator::EvmFeeEstimator`] — `eth_feeHistory` + OP-stack L1 oracle
//! - [`broadcaster::EvmBroadcaster`] — `eth_sendRawTransaction`
//! - [`service::EvmChainService`] — orchestrator
//!
//! The codec is pure — no `Provider`, no I/O. Apps that build transactions
//! server-side and sign client-side depend only on the codec; readers /
//! fee estimator / broadcaster are separate concrete impls that hold a
//! `RootProvider<Http>` (or any `Provider` implementation).

#![forbid(unsafe_code)]

pub mod abi;
pub mod broadcaster;
pub mod codec;
pub mod error;
pub mod fee_estimator;
pub mod reader;
pub mod service;
```

- [ ] **Step 4: Verify the crate at least compiles**

```bash
cargo check -p atlas-evm
```

Expected: build error from missing module files. That's expected — we'll fill them in the next tasks.

- [ ] **Step 5: Commit the scaffold**

```bash
git add Cargo.toml crates/atlas-evm/Cargo.toml crates/atlas-evm/src/lib.rs
git commit -m "chore(evm): scaffold atlas-evm crate"
```

---

### Task 2.2: ABI helpers + `error.rs`

**Files:**
- Create: `crates/atlas-evm/src/abi.rs`
- Create: `crates/atlas-evm/src/error.rs`

The codec needs to ABI-encode `transfer(address,uint256)` for ERC-20 transfers. Pure-function helper, no alloy-contract dep needed.

- [ ] **Step 1: Create `crates/atlas-evm/src/abi.rs`**

```rust
//! Minimal ABI encoding for the operations atlas-evm needs.
//!
//! Avoids depending on `alloy-contract` so the codec stays free of HTTP
//! client deps. We only need `transfer(address,uint256)` for ERC-20 transfers.

use alloy_primitives::{Address, U256};

/// Selector for `transfer(address,uint256)`: keccak256("transfer(address,uint256)")[..4].
pub const ERC20_TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

/// ABI-encode a `transfer(to, amount)` call.
///
/// Layout: 4-byte selector ‖ 32-byte left-padded address ‖ 32-byte big-endian amount.
/// Total: 68 bytes.
pub fn encode_erc20_transfer(to: Address, amount: U256) -> Vec<u8> {
    let mut buf = Vec::with_capacity(68);
    buf.extend_from_slice(&ERC20_TRANSFER_SELECTOR);
    // address is 20 bytes; ABI encodes as 32-byte left-padded
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(to.as_slice());
    // amount is 32-byte big-endian
    buf.extend_from_slice(&amount.to_be_bytes::<32>());
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn encodes_transfer_with_correct_selector() {
        let to = address!("0000000000000000000000000000000000000001");
        let amount = U256::from(100u64);
        let encoded = encode_erc20_transfer(to, amount);
        assert_eq!(encoded.len(), 68);
        assert_eq!(&encoded[..4], &ERC20_TRANSFER_SELECTOR);
        // address ends in 0x01
        assert_eq!(encoded[4 + 31], 0x01);
        // amount = 100, last byte
        assert_eq!(encoded[4 + 32 + 31], 100);
    }
}
```

- [ ] **Step 2: Create `crates/atlas-evm/src/error.rs`**

```rust
//! Conversions from alloy errors into [`atlas_core::ChainError`].

use atlas_core::error::{ChainError, RpcError};

pub(crate) fn map_transport_err<E: std::fmt::Display>(err: E) -> ChainError {
    ChainError::Rpc(RpcError::Transport(err.to_string()))
}

pub(crate) fn map_node_err<E: std::fmt::Display>(err: E) -> ChainError {
    ChainError::Rpc(RpcError::NodeError(err.to_string()))
}
```

- [ ] **Step 3: Verify abi tests pass**

```bash
cargo test -p atlas-evm --lib abi::tests
```

Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-evm/src/abi.rs crates/atlas-evm/src/error.rs
git commit -m "feat(evm): minimal ERC-20 ABI helper + alloy error mapping"
```

---

### Task 2.3: Implement `EvmCodec::prepare_transfer` for native + ERC-20

**Files:**
- Create: `crates/atlas-evm/src/codec.rs`
- Create: `crates/atlas-evm/tests/codec_native.rs`

The codec turns an `EvmPrepareContext` into an `UnsignedTransaction` whose `payload` field carries the RLP-encoded unsigned transaction (envelope-typed, EIP-2718).

- [ ] **Step 1: Create `crates/atlas-evm/src/codec.rs`**

```rust
//! `EvmCodec` — the pure encoding seam. No RPC, no I/O.
//!
//! Produces RLP-encoded EVM transactions (legacy or EIP-1559) from a
//! `TransferIntent`, computes the keccak256 signing digest, and assembles
//! a signed transaction from any of the three [`SigningResponse`] shapes.

use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope, TxLegacy};
use alloy_network::TxSignerSync;
use alloy_primitives::{Address, B256, Bytes, Signature, TxKind, U256};
use alloy_rlp::Encodable;
use atlas_core::asset::AssetStandard;
use atlas_core::chain::Curve;
use atlas_core::error::{ChainError, RpcError};
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AssetInstanceId, NetworkId};
use atlas_core::service::ChainCodec;
use atlas_core::signing::{SigningPayloadKind, SigningRequest, SigningResponse};
use atlas_core::transaction::{
    SignedTransaction, TransferIntent, UnsignedTransaction,
};
use num_bigint::BigInt;
use std::str::FromStr;

use crate::abi::encode_erc20_transfer;

/// Per-EVM-chain context passed into [`EvmCodec::prepare_transfer`].
#[derive(Clone, Debug)]
pub struct EvmPrepareContext {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    /// EIP-155 chain id used for replay protection.
    pub chain_id: u64,
    pub nonce: u64,
    pub fee: EvmFee,
    /// Asset standard of `intent.asset_instance_id`. Caller resolves this
    /// from the registry; the codec doesn't carry a registry.
    pub standard: AssetStandard,
    /// Required for ERC-20 transfers: the contract address. `None` for
    /// `Native` standard.
    pub contract: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EvmCodec;

impl ChainCodec for EvmCodec {
    type PrepareContext = EvmPrepareContext;

    fn prepare_transfer(
        &self,
        ctx: EvmPrepareContext,
    ) -> Result<UnsignedTransaction, ChainError> {
        // Parse recipient address.
        let to_addr = parse_address(ctx.intent.to.as_str())?;

        // Match on standard to decide native (value) vs ERC-20 (data).
        let (tx_kind, value, data) = match ctx.standard {
            AssetStandard::Native => {
                let value = bigint_to_u256(ctx.intent.amount.value())?;
                (TxKind::Call(to_addr), value, Vec::new())
            }
            AssetStandard::Erc20 => {
                let contract = ctx
                    .contract
                    .as_deref()
                    .ok_or_else(|| ChainError::TransactionBuildFailed(
                        "ERC-20 transfer requires contract address".to_string(),
                    ))?;
                let contract_addr = parse_address(contract)?;
                let amount = bigint_to_u256(ctx.intent.amount.value())?;
                let calldata = encode_erc20_transfer(to_addr, amount);
                (TxKind::Call(contract_addr), U256::ZERO, calldata)
            }
            AssetStandard::Spl => {
                return Err(ChainError::StandardNotSupported {
                    instance: ctx.intent.asset_instance_id.clone(),
                    standard: AssetStandard::Spl,
                });
            }
        };

        // Build either TxEip1559 or TxLegacy depending on EvmFee variant.
        let envelope_bytes = match ctx.fee {
            EvmFee::Eip1559 {
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas_limit,
                l1_fee_wei: _, // L1 fee influences fee accounting, not encoding
            } => {
                let tx = TxEip1559 {
                    chain_id: ctx.chain_id,
                    nonce: ctx.nonce,
                    gas_limit,
                    max_fee_per_gas: bigint_to_u128(&max_fee_per_gas)?,
                    max_priority_fee_per_gas: bigint_to_u128(&max_priority_fee_per_gas)?,
                    to: tx_kind,
                    value,
                    access_list: Default::default(),
                    input: Bytes::from(data),
                };
                let mut buf = Vec::new();
                tx.encode_for_signing(&mut buf);
                buf
            }
            EvmFee::Legacy { gas_price, gas_limit } => {
                let tx = TxLegacy {
                    chain_id: Some(ctx.chain_id),
                    nonce: ctx.nonce,
                    gas_price: bigint_to_u128(&gas_price)?,
                    gas_limit,
                    to: tx_kind,
                    value,
                    input: Bytes::from(data),
                };
                let mut buf = Vec::new();
                tx.encode_for_signing(&mut buf);
                buf
            }
        };

        Ok(UnsignedTransaction {
            account: ctx.account,
            network: ctx.network,
            intent: ctx.intent,
            payload: envelope_bytes,
        })
    }

    fn signing_request(
        &self,
        unsigned: &UnsignedTransaction,
    ) -> Result<SigningRequest, ChainError> {
        // The payload is the encode_for_signing bytes. Compute keccak256 ourselves.
        let digest: B256 = alloy_primitives::keccak256(&unsigned.payload);
        Ok(SigningRequest {
            account: unsigned.account.clone(),
            network: unsigned.network.clone(),
            curve: Curve::Secp256k1,
            payload_kind: SigningPayloadKind::TransactionDigest,
            payload: digest.to_vec(),
        })
    }

    fn assemble_signed(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError> {
        match response {
            SigningResponse::SignatureOnly { signature, .. } => {
                // Expect 65 bytes: r ‖ s ‖ v (or recovery id).
                if signature.len() != 65 {
                    return Err(ChainError::TransactionBuildFailed(format!(
                        "expected 65-byte signature, got {}",
                        signature.len()
                    )));
                }
                let sig = parse_65_byte_signature(&signature)?;

                // Re-decode the unsigned envelope from payload to know which
                // tx variant we're assembling.
                let raw = encode_signed_from_unsigned(&unsigned.payload, sig)?;
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
                    "atlas-evm assemble expects signed bytes, got submitted hash {tx_hash}"
                )))
            }
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn parse_address(s: &str) -> Result<Address, ChainError> {
    Address::from_str(s)
        .map_err(|e| ChainError::InvalidAddress(format!("{}: {}", s, e)))
}

fn bigint_to_u256(v: &BigInt) -> Result<U256, ChainError> {
    use num_bigint::Sign;
    if v.sign() == Sign::Minus {
        return Err(ChainError::TransactionBuildFailed(
            "negative amount".to_string(),
        ));
    }
    let (_, bytes_be) = v.to_bytes_be();
    if bytes_be.len() > 32 {
        return Err(ChainError::TransactionBuildFailed(
            "amount exceeds 256 bits".to_string(),
        ));
    }
    let mut padded = [0u8; 32];
    padded[32 - bytes_be.len()..].copy_from_slice(&bytes_be);
    Ok(U256::from_be_bytes(padded))
}

fn bigint_to_u128(v: &BigInt) -> Result<u128, ChainError> {
    use num_bigint::Sign;
    if v.sign() == Sign::Minus {
        return Err(ChainError::TransactionBuildFailed(
            "negative fee".to_string(),
        ));
    }
    let (_, bytes_be) = v.to_bytes_be();
    if bytes_be.len() > 16 {
        return Err(ChainError::FeeEstimationFailed(
            "fee value exceeds u128".to_string(),
        ));
    }
    let mut padded = [0u8; 16];
    padded[16 - bytes_be.len()..].copy_from_slice(&bytes_be);
    Ok(u128::from_be_bytes(padded))
}

fn parse_65_byte_signature(bytes: &[u8]) -> Result<Signature, ChainError> {
    // Accept either 65-byte r ‖ s ‖ v form. v can be 0/1 or 27/28.
    let r = alloy_primitives::U256::from_be_slice(&bytes[0..32]);
    let s = alloy_primitives::U256::from_be_slice(&bytes[32..64]);
    let v_byte = bytes[64];
    let y_parity = match v_byte {
        0 | 27 => false,
        1 | 28 => true,
        _ => {
            // EIP-155: v = 35 + 2 * chain_id + parity. Caller may have already
            // applied EIP-155; treat anything else as an error.
            return Err(ChainError::InvalidAddress(format!(
                "unexpected signature recovery byte: {}",
                v_byte
            )));
        }
    };
    Ok(Signature::new(r, s, y_parity))
}

/// Take the unsigned RLP bytes (encode_for_signing output) and a signature,
/// decode the unsigned tx variant, then encode the signed envelope.
///
/// We round-trip through alloy's typed structs to build a properly-tagged
/// EIP-2718 envelope.
fn encode_signed_from_unsigned(
    unsigned_payload: &[u8],
    sig: Signature,
) -> Result<Vec<u8>, ChainError> {
    // Try EIP-1559 first (typed envelope: leading byte = 0x02)
    if unsigned_payload.first() == Some(&0x02) {
        // alloy_consensus::TxEip1559 doesn't expose a public decode-for-signing,
        // so we re-encode by treating the trailing RLP body. Simpler path: use
        // alloy_consensus::TxEnvelope::decode_2718 on the *signed* envelope —
        // but we don't have that yet. Instead, decode the unsigned via the
        // Encodable trait shape and rebuild.
        decode_eip1559_unsigned_and_sign(unsigned_payload, sig)
    } else {
        decode_legacy_unsigned_and_sign(unsigned_payload, sig)
    }
}

fn decode_eip1559_unsigned_and_sign(
    unsigned_payload: &[u8],
    sig: Signature,
) -> Result<Vec<u8>, ChainError> {
    use alloy_consensus::TxEip1559;
    use alloy_rlp::Decodable;
    let mut buf: &[u8] = &unsigned_payload[1..]; // skip 0x02 envelope tag
    let tx = TxEip1559::decode(&mut buf).map_err(|e| {
        ChainError::TransactionBuildFailed(format!("EIP-1559 decode failed: {}", e))
    })?;
    let signed = tx.into_signed(sig);
    let envelope: TxEnvelope = signed.into();
    let mut out = Vec::new();
    envelope.encode_2718(&mut out);
    Ok(out)
}

fn decode_legacy_unsigned_and_sign(
    unsigned_payload: &[u8],
    sig: Signature,
) -> Result<Vec<u8>, ChainError> {
    use alloy_consensus::TxLegacy;
    use alloy_rlp::Decodable;
    let mut buf: &[u8] = unsigned_payload;
    let tx = TxLegacy::decode(&mut buf).map_err(|e| {
        ChainError::TransactionBuildFailed(format!("Legacy decode failed: {}", e))
    })?;
    let signed = tx.into_signed(sig);
    let envelope: TxEnvelope = signed.into();
    let mut out = Vec::new();
    envelope.encode_2718(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigint_to_u256_zero() {
        let v = BigInt::from(0u64);
        assert_eq!(bigint_to_u256(&v).unwrap(), U256::ZERO);
    }

    #[test]
    fn bigint_to_u256_max_u128() {
        let v = BigInt::from(u128::MAX);
        let u = bigint_to_u256(&v).unwrap();
        assert_eq!(u, U256::from(u128::MAX));
    }

    #[test]
    fn bigint_to_u256_rejects_negative() {
        let v = BigInt::from(-1);
        assert!(bigint_to_u256(&v).is_err());
    }

    #[test]
    fn bigint_to_u128_rejects_overflow() {
        let v = BigInt::from(u128::MAX) + BigInt::from(1u64);
        assert!(bigint_to_u128(&v).is_err());
    }
}
```

- [ ] **Step 2: Create the native-transfer test `crates/atlas-evm/tests/codec_native.rs`**

```rust
use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_primitives::{address, Bytes, TxKind, U256};
use alloy_rlp::Encodable;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId};
use atlas_core::service::ChainCodec;
use atlas_core::transaction::TransferIntent;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use num_bigint::BigInt;
use std::str::FromStr;

#[test]
fn prepare_transfer_native_eip1559_matches_alloy_direct_encoding() {
    let codec = EvmCodec;
    let to = "0x0000000000000000000000000000000000000001";
    let amount = 10u64.pow(15); // 0.001 ETH

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str(to).unwrap(),
            amount: RawAmount::new(BigInt::from(amount), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 7,
        fee: EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        },
        standard: AssetStandard::Native,
        contract: None,
    };

    let unsigned = codec.prepare_transfer(ctx).unwrap();

    // Reference: build the same TxEip1559 directly with alloy and assert
    // encode_for_signing produces identical bytes.
    let reference = TxEip1559 {
        chain_id: 1,
        nonce: 7,
        gas_limit: 21_000,
        max_fee_per_gas: 30_000_000_000u128,
        max_priority_fee_per_gas: 1_000_000_000u128,
        to: TxKind::Call(address!("0000000000000000000000000000000000000001")),
        value: U256::from(amount),
        access_list: Default::default(),
        input: Bytes::new(),
    };
    let mut ref_bytes = Vec::new();
    reference.encode_for_signing(&mut ref_bytes);

    assert_eq!(unsigned.payload, ref_bytes);
}

#[test]
fn prepare_transfer_native_legacy_encodes_chain_id_for_eip155() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(20_000_000_000u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).unwrap();
    // Legacy encoding starts with RLP list prefix (0xc0..0xff range), not 0x02.
    assert_ne!(unsigned.payload.first(), Some(&0x02u8));
    assert!(!unsigned.payload.is_empty());
}
```

- [ ] **Step 3: Run the codec tests**

```bash
cargo test -p atlas-evm --test codec_native
cargo test -p atlas-evm --lib codec::tests
```

Expected: 2 + 4 = 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-evm/src/codec.rs crates/atlas-evm/tests/codec_native.rs
git commit -m "feat(evm): EvmCodec::prepare_transfer for native transfers"
```

---

### Task 2.4: ERC-20 path test for `EvmCodec`

**Files:**
- Create: `crates/atlas-evm/tests/codec_erc20.rs`

The EVM codec implementation already handles ERC-20 in Task 2.3 — this task adds the integration test that exercises the path.

- [ ] **Step 1: Create `crates/atlas-evm/tests/codec_erc20.rs`**

```rust
use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_primitives::{address, Bytes, TxKind, U256};
use alloy_rlp::Encodable;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId};
use atlas_core::service::ChainCodec;
use atlas_core::transaction::TransferIntent;
use atlas_evm::abi::encode_erc20_transfer;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use num_bigint::BigInt;
use std::str::FromStr;

#[test]
fn prepare_transfer_erc20_uses_contract_as_to_and_zero_value() {
    let codec = EvmCodec;
    // Base USDC contract.
    let usdc_base = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
    let recipient = "0x0000000000000000000000000000000000000003";
    let amount: u64 = 1_000_000; // 1 USDC (6 decimals)

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:8453").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(
                "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            )
            .unwrap(),
            to: AddressRef::from_str(recipient).unwrap(),
            amount: RawAmount::new(BigInt::from(amount), 6).unwrap(),
        },
        chain_id: 8453,
        nonce: 12,
        fee: EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(2_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000u64),
            gas_limit: 60_000,
            l1_fee_wei: None,
        },
        standard: AssetStandard::Erc20,
        contract: Some(usdc_base.to_string()),
    };

    let unsigned = codec.prepare_transfer(ctx).unwrap();

    // Reference: build same TxEip1559 directly and assert bytes match.
    let calldata = encode_erc20_transfer(
        address!("0000000000000000000000000000000000000003"),
        U256::from(amount),
    );
    let reference = TxEip1559 {
        chain_id: 8453,
        nonce: 12,
        gas_limit: 60_000,
        max_fee_per_gas: 2_000_000_000u128,
        max_priority_fee_per_gas: 1_000_000u128,
        to: TxKind::Call(address!("833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")),
        value: U256::ZERO,
        access_list: Default::default(),
        input: Bytes::from(calldata),
    };
    let mut ref_bytes = Vec::new();
    reference.encode_for_signing(&mut ref_bytes);

    assert_eq!(unsigned.payload, ref_bytes);
}

#[test]
fn prepare_transfer_erc20_without_contract_errors() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:8453").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(
                "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            )
            .unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000003").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 6).unwrap(),
        },
        chain_id: 8453,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 60_000,
        },
        standard: AssetStandard::Erc20,
        contract: None, // ← missing
    };
    let err = codec.prepare_transfer(ctx).unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::ChainError::TransactionBuildFailed(_)
    ));
}

#[test]
fn prepare_transfer_spl_returns_standard_not_supported() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str(
                "solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/spl:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            )
            .unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 6).unwrap(),
        },
        chain_id: 0,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Spl,
        contract: Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string()),
    };
    let err = codec.prepare_transfer(ctx).unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::ChainError::StandardNotSupported { .. }
    ));
}
```

- [ ] **Step 2: Run the test**

```bash
cargo test -p atlas-evm --test codec_erc20
```

Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-evm/tests/codec_erc20.rs
git commit -m "test(evm): codec ERC-20 + Spl-rejection paths"
```

---

### Task 2.5: `EvmCodec::signing_request` + `assemble_signed` integration test

**Files:**
- Create: `crates/atlas-evm/tests/codec_assemble.rs`

The codec already implements both methods (Task 2.3). This test exercises the round-trip: build → digest → sign locally with `alloy-signer-local` → assemble → decode → assert sender matches.

- [ ] **Step 1: Create `crates/atlas-evm/tests/codec_assemble.rs`**

```rust
use alloy_consensus::TxEnvelope;
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::Address;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::chain::Curve;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::ChainCodec;
use atlas_core::signing::{SigningPayloadKind, SigningResponse};
use atlas_core::transaction::TransferIntent;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use num_bigint::BigInt;
use std::str::FromStr;

#[test]
fn signing_request_payload_is_keccak256_of_payload() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
        fee: EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        },
        standard: AssetStandard::Native,
        contract: None,
    };

    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let request = codec.signing_request(&unsigned).unwrap();

    assert_eq!(request.curve, Curve::Secp256k1);
    assert_eq!(request.payload_kind, SigningPayloadKind::TransactionDigest);
    assert_eq!(request.payload.len(), 32);
    let direct = alloy_primitives::keccak256(&unsigned.payload);
    assert_eq!(request.payload.as_slice(), direct.as_slice());
}

#[test]
fn assemble_signed_round_trip_recovers_sender_for_eip1559() {
    let codec = EvmCodec;
    // Deterministic test key.
    let key_bytes = [
        0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e,
        0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32, 0x06,
        0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c,
        0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06,
    ];
    let signer = PrivateKeySigner::from_bytes(&key_bytes.into()).unwrap();
    let expected_sender: Address = signer.address();

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            amount: RawAmount::new(BigInt::from(1_000u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
        fee: EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        },
        standard: AssetStandard::Native,
        contract: None,
    };

    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let request = codec.signing_request(&unsigned).unwrap();

    // Sign the digest directly.
    let digest = alloy_primitives::B256::from_slice(&request.payload);
    let signature = signer.sign_hash_sync(&digest).unwrap();
    let mut sig_bytes = Vec::with_capacity(65);
    sig_bytes.extend_from_slice(&signature.r().to_be_bytes::<32>());
    sig_bytes.extend_from_slice(&signature.s().to_be_bytes::<32>());
    sig_bytes.push(if signature.v() { 1 } else { 0 });

    let response = SigningResponse::SignatureOnly {
        signer: SignerId::from_str("test").unwrap(),
        signature: sig_bytes,
        public_key: vec![], // unused by assemble_signed
    };

    let signed = codec.assemble_signed(unsigned, response).unwrap();

    // Decode the signed envelope and recover the sender.
    let envelope: TxEnvelope = TxEnvelope::decode_2718(&mut &signed.raw[..]).unwrap();
    let recovered = envelope.recover_signer().unwrap();
    assert_eq!(recovered, expected_sender);
}

#[test]
fn assemble_signed_passes_through_signed_transaction_variant() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let response = SigningResponse::SignedTransaction {
        signer: SignerId::from_str("test").unwrap(),
        raw: vec![0xde, 0xad, 0xbe, 0xef],
    };
    let signed = codec.assemble_signed(unsigned, response).unwrap();
    assert_eq!(signed.raw, vec![0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn assemble_signed_rejects_submitted_transaction_variant() {
    let codec = EvmCodec;
    let ctx = EvmPrepareContext {
        account: AccountRef::from_str("account-1").unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000002").unwrap(),
            amount: RawAmount::new(BigInt::from(1u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 0,
        fee: EvmFee::Legacy {
            gas_price: BigInt::from(1u64),
            gas_limit: 21_000,
        },
        standard: AssetStandard::Native,
        contract: None,
    };
    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let response = SigningResponse::SubmittedTransaction {
        signer: SignerId::from_str("test").unwrap(),
        tx_hash: "0xabc".to_string(),
    };
    let err = codec.assemble_signed(unsigned, response).unwrap_err();
    assert!(matches!(
        err,
        atlas_core::error::ChainError::TransactionBuildFailed(_)
    ));
}
```

- [ ] **Step 2: Run the test**

```bash
cargo test -p atlas-evm --test codec_assemble
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-evm/tests/codec_assemble.rs
git commit -m "test(evm): codec signing_request + assemble_signed round-trip"
```

---

### Task 2.6: Implement `EvmReader<P>`

**Files:**
- Create: `crates/atlas-evm/src/reader.rs`

- [ ] **Step 1: Create `crates/atlas-evm/src/reader.rs`**

```rust
//! `EvmReader` — RPC reads (balance, nonce, status) via alloy-provider.

use alloy_primitives::{Address, B256, U256};
use alloy_provider::Provider;
use async_trait::async_trait;
use atlas_core::amount::RawAmount;
use atlas_core::asset::{AssetInstance, AssetStandard};
use atlas_core::error::ChainError;
use atlas_core::fee::TransactionStatus;
use atlas_core::id::{AddressRef, NetworkId};
use atlas_core::service::ChainReader;
use num_bigint::BigInt;
use std::str::FromStr;

use crate::abi::{ERC20_TRANSFER_SELECTOR, encode_erc20_transfer};
use crate::error::{map_node_err, map_transport_err};

/// keccak256("balanceOf(address)")[..4]
const ERC20_BALANCE_OF_SELECTOR: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

pub struct EvmReader<P> {
    provider: P,
}

impl<P> EvmReader<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: Provider + Clone> ChainReader for EvmReader<P> {
    async fn get_balance(
        &self,
        instance: &AssetInstance,
        address: &AddressRef,
    ) -> Result<RawAmount, ChainError> {
        let addr = parse_address(address.as_str())?;
        let raw = match instance.standard {
            AssetStandard::Native => {
                let balance: U256 = self
                    .provider
                    .get_balance(addr)
                    .await
                    .map_err(map_transport_err)?;
                u256_to_bigint(balance)
            }
            AssetStandard::Erc20 => {
                let contract = instance.contract.as_deref().ok_or_else(|| {
                    ChainError::TransactionBuildFailed(
                        "ERC-20 instance missing contract".to_string(),
                    )
                })?;
                let contract_addr = parse_address(contract)?;
                let mut calldata = Vec::with_capacity(36);
                calldata.extend_from_slice(&ERC20_BALANCE_OF_SELECTOR);
                calldata.extend_from_slice(&[0u8; 12]);
                calldata.extend_from_slice(addr.as_slice());
                let result = self
                    .provider
                    .call(&alloy_rpc_types_eth::TransactionRequest::default()
                        .to(contract_addr)
                        .input(calldata.into()))
                    .await
                    .map_err(map_node_err)?;
                if result.len() != 32 {
                    return Err(ChainError::Rpc(
                        atlas_core::error::RpcError::MalformedResponse(format!(
                            "balanceOf returned {} bytes, expected 32",
                            result.len()
                        )),
                    ));
                }
                let value = U256::from_be_slice(&result);
                u256_to_bigint(value)
            }
            AssetStandard::Spl => {
                return Err(ChainError::StandardNotSupported {
                    instance: instance.id.clone(),
                    standard: AssetStandard::Spl,
                });
            }
        };
        // Suppress unused-warning for the encoder when this path is dead-coded:
        let _ = encode_erc20_transfer;

        RawAmount::new(raw, instance.decimals)
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))
    }

    async fn get_nonce(
        &self,
        _network: &NetworkId,
        address: &AddressRef,
    ) -> Result<u64, ChainError> {
        let addr = parse_address(address.as_str())?;
        let nonce = self
            .provider
            .get_transaction_count(addr)
            .await
            .map_err(map_transport_err)?;
        Ok(nonce)
    }

    async fn get_transaction_status(
        &self,
        _network: &NetworkId,
        hash: &str,
    ) -> Result<TransactionStatus, ChainError> {
        let tx_hash = B256::from_str(hash)
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;
        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(map_transport_err)?;

        match receipt {
            Some(r) => {
                let block_number = r.block_number.unwrap_or(0);
                let gas_used = BigInt::from(r.gas_used);
                if r.status() {
                    Ok(TransactionStatus::Confirmed {
                        hash: hash.to_string(),
                        block_number,
                        gas_used,
                    })
                } else {
                    Ok(TransactionStatus::Failed {
                        hash: hash.to_string(),
                        reason: "transaction reverted".to_string(),
                    })
                }
            }
            None => {
                let pending = self
                    .provider
                    .get_transaction_by_hash(tx_hash)
                    .await
                    .map_err(map_transport_err)?;
                if pending.is_some() {
                    Ok(TransactionStatus::Pending {
                        hash: hash.to_string(),
                    })
                } else {
                    Ok(TransactionStatus::NotFound {
                        hash: hash.to_string(),
                    })
                }
            }
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn parse_address(s: &str) -> Result<Address, ChainError> {
    Address::from_str(s).map_err(|e| ChainError::InvalidAddress(format!("{}: {}", s, e)))
}

fn u256_to_bigint(v: U256) -> BigInt {
    let bytes = v.to_be_bytes::<32>();
    BigInt::from_bytes_be(num_bigint::Sign::Plus, &bytes)
}
```

- [ ] **Step 2: Compile-check**

```bash
cargo check -p atlas-evm
```

Expected: clean. (Reader is exercised end-to-end in Task 2.10 with a mocked provider.)

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-evm/src/reader.rs
git commit -m "feat(evm): EvmReader — get_balance / get_nonce / get_transaction_status"
```

---

### Task 2.7: Implement `EvmFeeEstimator<P>`

**Files:**
- Create: `crates/atlas-evm/src/fee_estimator.rs`

The estimator uses `eth_feeHistory` to derive an EIP-1559 priority-fee suggestion. Falls back to `eth_gasPrice` if fee history fails. OP-Stack L1 fee oracle is an optional follow-up — for v1 we leave `l1_fee_wei: None` and document the gap.

- [ ] **Step 1: Create `crates/atlas-evm/src/fee_estimator.rs`**

```rust
//! `EvmFeeEstimator` — fee suggestion via `eth_feeHistory` + fallback.

use alloy_provider::Provider;
use alloy_rpc_types_eth::BlockNumberOrTag;
use async_trait::async_trait;
use atlas_core::error::ChainError;
use atlas_core::fee::EvmFee;
use atlas_core::id::AddressRef;
use atlas_core::service::FeeEstimator;
use atlas_core::transaction::TransferIntent;
use num_bigint::BigInt;

use crate::error::map_transport_err;

/// Minimum priority fee in wei (0.001 gwei).
const MIN_PRIORITY_FEE_WEI: u128 = 1_000_000;
/// Maximum priority fee in wei (0.2 gwei).
const MAX_PRIORITY_FEE_WEI: u128 = 200_000_000;
/// Multiplier applied to baseFee when computing maxFeePerGas.
const BASE_FEE_MULTIPLIER: u128 = 2;
/// Default native-transfer gas limit (21000).
const NATIVE_TRANSFER_GAS_LIMIT: u64 = 21_000;
/// Conservative ERC-20 transfer gas limit floor.
const ERC20_TRANSFER_GAS_LIMIT_FLOOR: u64 = 60_000;

pub struct EvmFeeEstimator<P> {
    provider: P,
    /// Whether this network supports EIP-1559. Set at construction from
    /// `Network.features.eip1559`. When false, the estimator returns
    /// `EvmFee::Legacy`.
    pub eip1559: bool,
}

impl<P> EvmFeeEstimator<P> {
    pub fn new(provider: P, eip1559: bool) -> Self {
        Self { provider, eip1559 }
    }
}

#[async_trait]
impl<P: Provider + Clone> FeeEstimator for EvmFeeEstimator<P> {
    type Fee = EvmFee;

    async fn estimate_fee(
        &self,
        intent: &TransferIntent,
        _sender: &AddressRef,
    ) -> Result<EvmFee, ChainError> {
        // Choose gas limit based on intent shape. For ERC-20 we'd ideally
        // call eth_estimateGas; for v1 we use a conservative floor.
        let gas_limit = if intent.asset_instance_id.as_str().contains("/erc20:") {
            ERC20_TRANSFER_GAS_LIMIT_FLOOR
        } else {
            NATIVE_TRANSFER_GAS_LIMIT
        };

        if self.eip1559 {
            self.eip1559_fee(gas_limit).await.or_else(|_| {
                // Fall back to legacy if fee history fails.
                Box::pin(self.legacy_fee(gas_limit)).as_mut().poll_fallback()
            })
        } else {
            self.legacy_fee(gas_limit).await
        }
    }
}

impl<P: Provider + Clone> EvmFeeEstimator<P> {
    async fn eip1559_fee(&self, gas_limit: u64) -> Result<EvmFee, ChainError> {
        let fh = self
            .provider
            .get_fee_history(10, BlockNumberOrTag::Latest, &[50.0])
            .await
            .map_err(map_transport_err)?;

        let base_fee = fh
            .base_fee_per_gas
            .last()
            .copied()
            .ok_or_else(|| ChainError::FeeEstimationFailed(
                "feeHistory returned empty baseFeePerGas".to_string(),
            ))?;

        // Pick median of the 50th-percentile rewards across the window.
        let mut samples: Vec<u128> = fh
            .reward
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| r.first().copied())
            .filter(|&v| v > 0)
            .collect();
        samples.sort_unstable();
        let suggested = samples.get(samples.len() / 2).copied().unwrap_or(MIN_PRIORITY_FEE_WEI);

        let max_priority = suggested.clamp(MIN_PRIORITY_FEE_WEI, MAX_PRIORITY_FEE_WEI);
        let max_fee_candidate = base_fee.saturating_mul(BASE_FEE_MULTIPLIER).saturating_add(max_priority);
        let max_fee = max_fee_candidate.max(base_fee.saturating_add(max_priority));

        Ok(EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(max_fee),
            max_priority_fee_per_gas: BigInt::from(max_priority),
            gas_limit,
            l1_fee_wei: None, // OP-Stack L1 oracle is a deferred follow-up
        })
    }

    async fn legacy_fee(&self, gas_limit: u64) -> Result<EvmFee, ChainError> {
        let gas_price = self.provider.get_gas_price().await.map_err(map_transport_err)?;
        Ok(EvmFee::Legacy {
            gas_price: BigInt::from(gas_price),
            gas_limit,
        })
    }
}

// ── tiny shim because async fallback requires an awkward pattern ─────────

trait Fallback<T, E> {
    fn poll_fallback(&mut self) -> Result<T, E>;
}
impl<F, T, E> Fallback<T, E> for std::pin::Pin<&mut F>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    fn poll_fallback(&mut self) -> Result<T, E> {
        // Block on the fallback future. Acceptable here because
        // legacy_fee is short and we're already inside an async fn.
        // Real production code would use tokio::runtime::Handle::block_in_place
        // or refactor to chained `.or_else(async move { … })` once
        // `try_or_else` lands. For v1 we simulate via futures::executor.
        futures::executor::block_on(self.as_mut())
    }
}
```

> **Note on the fallback shim**: the awkward `Fallback` trait at the bottom is a workaround pattern for chaining async results. During execution, replace the shim with the cleaner approach if the alloy version supports it; otherwise, refactor `estimate_fee` to use a `match` on the `eip1559_fee()` result and call `legacy_fee().await` from the `Err` arm directly. Add `futures = "0.3"` to atlas-evm's deps if you keep the shim.

- [ ] **Step 2: Compile-check**

```bash
cargo check -p atlas-evm
```

If the fallback shim doesn't compile (likely in current alloy versions), refactor `estimate_fee` to:

```rust
async fn estimate_fee(&self, intent: &TransferIntent, _sender: &AddressRef)
    -> Result<EvmFee, ChainError>
{
    let gas_limit = if intent.asset_instance_id.as_str().contains("/erc20:") {
        ERC20_TRANSFER_GAS_LIMIT_FLOOR
    } else {
        NATIVE_TRANSFER_GAS_LIMIT
    };
    if self.eip1559 {
        match self.eip1559_fee(gas_limit).await {
            Ok(fee) => Ok(fee),
            Err(_) => self.legacy_fee(gas_limit).await,
        }
    } else {
        self.legacy_fee(gas_limit).await
    }
}
```

Drop the `Fallback` trait at the bottom of the file.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-evm/src/fee_estimator.rs
git commit -m "feat(evm): EvmFeeEstimator — EIP-1559 via feeHistory + legacy fallback"
```

---

### Task 2.8: Implement `EvmBroadcaster<P>`

**Files:**
- Create: `crates/atlas-evm/src/broadcaster.rs`

- [ ] **Step 1: Create `crates/atlas-evm/src/broadcaster.rs`**

```rust
//! `EvmBroadcaster` — `eth_sendRawTransaction` via alloy-provider.

use alloy_primitives::Bytes;
use alloy_provider::Provider;
use async_trait::async_trait;
use atlas_core::error::ChainError;
use atlas_core::service::ChainBroadcaster;
use atlas_core::transaction::{BroadcastResult, SignedTransaction};

use crate::error::map_transport_err;

pub struct EvmBroadcaster<P> {
    provider: P,
}

impl<P> EvmBroadcaster<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: Provider + Clone> ChainBroadcaster for EvmBroadcaster<P> {
    async fn broadcast(
        &self,
        signed: SignedTransaction,
    ) -> Result<BroadcastResult, ChainError> {
        if signed.raw.is_empty() {
            return Err(ChainError::BroadcastFailed(
                "empty signed transaction".to_string(),
            ));
        }
        let pending = self
            .provider
            .send_raw_transaction(&signed.raw)
            .await
            .map_err(map_transport_err)?;
        let tx_hash = format!("{:?}", pending.tx_hash());
        Ok(BroadcastResult { tx_hash })
    }
}
```

- [ ] **Step 2: Compile-check**

```bash
cargo check -p atlas-evm
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-evm/src/broadcaster.rs
git commit -m "feat(evm): EvmBroadcaster — eth_sendRawTransaction"
```

---

### Task 2.9: Implement `EvmChainService<P>` orchestrator

**Files:**
- Create: `crates/atlas-evm/src/service.rs`

- [ ] **Step 1: Create `crates/atlas-evm/src/service.rs`**

```rust
//! `EvmChainService` — orchestrator that composes `EvmCodec`, `EvmReader`,
//! `EvmFeeEstimator`, `EvmBroadcaster`, and a `SignerProvider` into the
//! happy-path transfer flow.

use alloy_provider::Provider;
use async_trait::async_trait;
use atlas_core::asset::AssetStandard;
use atlas_core::error::ChainError;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, NetworkId};
use atlas_core::service::{
    ChainBroadcaster, ChainCodec, ChainReader, ChainService, FeeEstimator,
};
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::{BroadcastResult, TransferIntent};

use crate::broadcaster::EvmBroadcaster;
use crate::codec::{EvmCodec, EvmPrepareContext};
use crate::fee_estimator::EvmFeeEstimator;
use crate::reader::EvmReader;

pub struct EvmChainService<P> {
    pub codec: EvmCodec,
    pub reader: EvmReader<P>,
    pub fee_estimator: EvmFeeEstimator<P>,
    pub broadcaster: EvmBroadcaster<P>,
    /// Network this orchestrator binds to.
    pub network: NetworkId,
    /// EVM chain id for replay protection.
    pub chain_id: u64,
}

impl<P: Provider + Clone> EvmChainService<P> {
    /// Construct an orchestrator from a provider and network metadata.
    /// Caller is responsible for parsing `Network.chain_id` to a `u64`.
    pub fn new(provider: P, network: NetworkId, chain_id: u64, eip1559: bool) -> Self {
        Self {
            codec: EvmCodec,
            reader: EvmReader::new(provider.clone()),
            fee_estimator: EvmFeeEstimator::new(provider.clone(), eip1559),
            broadcaster: EvmBroadcaster::new(provider),
            network,
            chain_id,
        }
    }
}

#[async_trait]
impl<P: Provider + Clone + Send + Sync + 'static> ChainService for EvmChainService<P> {
    type PrepareContext = EvmPrepareContext;
    type Fee = EvmFee;

    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn SignerProvider,
    ) -> Result<BroadcastResult, ChainError> {
        // Validate that the intent's asset belongs to this orchestrator's network.
        let prefix = format!("{}/", self.network.as_str());
        if !intent.asset_instance_id.as_str().starts_with(&prefix) {
            return Err(ChainError::UnsupportedAssetInstance(intent.asset_instance_id));
        }

        // Resolve sender address from the signer's id by signing a
        // throwaway zero-digest message — the signer must return its
        // public key in `SignatureOnly`. v1 keeps this simple: callers
        // pass the sender address explicitly via account.
        //
        // For v1 we use AccountRef as a string holding the EVM address
        // directly. Higher-level account types come later (atlas-account).
        let sender = AddressRef::new(account.as_str())
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;

        // Concurrently fetch nonce + estimate fee.
        let (nonce, fee) = tokio::try_join!(
            self.reader.get_nonce(&self.network, &sender),
            self.fee_estimator.estimate_fee(&intent, &sender),
        )?;

        // Resolve standard + contract from intent.asset_instance_id.
        // Caller is expected to pre-resolve via Registry::asset_instance(...);
        // for v1 we infer from the CAIP path: `/native:` vs `/erc20:`.
        let (standard, contract) = parse_standard_from_instance(&intent)?;

        let unsigned = self.codec.prepare_transfer(EvmPrepareContext {
            account,
            network: self.network.clone(),
            intent,
            chain_id: self.chain_id,
            nonce,
            fee,
            standard,
            contract,
        })?;

        let request = self.codec.signing_request(&unsigned)?;
        let response = signer
            .sign(request)
            .await
            .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))?;
        let signed = self.codec.assemble_signed(unsigned, response)?;
        self.broadcaster.broadcast(signed).await
    }
}

fn parse_standard_from_instance(
    intent: &TransferIntent,
) -> Result<(AssetStandard, Option<String>), ChainError> {
    let id = intent.asset_instance_id.as_str();
    if let Some((_, rest)) = id.split_once('/') {
        if let Some(contract) = rest.strip_prefix("erc20:") {
            return Ok((AssetStandard::Erc20, Some(contract.to_string())));
        }
        if rest.starts_with("native:") {
            return Ok((AssetStandard::Native, None));
        }
    }
    Err(ChainError::UnsupportedAssetInstance(intent.asset_instance_id.clone()))
}
```

- [ ] **Step 2: Verify the crate compiles**

```bash
cargo check -p atlas-evm
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-evm/src/service.rs
git commit -m "feat(evm): EvmChainService orchestrator"
```

---

## Phase 3: atlas-signer-localkey crate

### Task 3.1: Scaffold the signer crate

**Files:**
- Create: `crates/atlas-signer-localkey/Cargo.toml`
- Create: `crates/atlas-signer-localkey/src/lib.rs`

- [ ] **Step 1: Create `crates/atlas-signer-localkey/Cargo.toml`**

```toml
[package]
name = "atlas-signer-localkey"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
alloy-primitives.workspace = true
alloy-signer.workspace = true
alloy-signer-local = { workspace = true, features = ["mnemonic", "keystore"] }
async-trait.workspace = true
atlas-core = { path = "../atlas-core" }
bip32.workspace = true
bip39.workspace = true
eth-keystore.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: Create `crates/atlas-signer-localkey/src/lib.rs`**

```rust
//! In-process secp256k1 reference signer for the Atlas SDK.
//!
//! Three construction paths:
//! - [`LocalKeySigner::from_bytes`] — raw 32-byte private key.
//! - [`LocalKeySigner::from_keystore`] — Web3 secret-storage JSON keystore.
//! - [`LocalKeySigner::from_mnemonic`] — BIP-39 mnemonic + BIP-32 path.
//!
//! Implements [`atlas_core::signing::SignerProvider`]. Returns
//! [`SigningResponse::SignatureOnly`] (raw 65-byte signature + public key).
//! The chain codec handles assembly. The signer is **chain-agnostic** at the
//! signing step — same crate signs for EVM today and any future
//! secp256k1-based chain tomorrow.

#![forbid(unsafe_code)]

use alloy_primitives::B256;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use async_trait::async_trait;
use atlas_core::chain::Curve;
use atlas_core::error::SigningError;
use atlas_core::id::SignerId;
use atlas_core::signing::{
    SignerProvider, SigningPayloadKind, SigningRequest, SigningResponse,
};

pub struct LocalKeySigner {
    inner: PrivateKeySigner,
    id: SignerId,
}

impl LocalKeySigner {
    /// Construct from a raw 32-byte secp256k1 private key.
    pub fn from_bytes(id: SignerId, bytes: [u8; 32]) -> Result<Self, SigningError> {
        let inner = PrivateKeySigner::from_bytes(&bytes.into())
            .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        Ok(Self { inner, id })
    }

    /// Construct from a Web3 secret-storage JSON keystore.
    pub fn from_keystore(
        id: SignerId,
        json: &str,
        password: &str,
    ) -> Result<Self, SigningError> {
        // eth_keystore::decrypt_key_from_str returns the 32-byte private key.
        let key_bytes = eth_keystore::decrypt_key(
            std::path::PathBuf::new(),
            password,
        )
        .or_else(|_| {
            // For string-based decryption, write to a temp file and read back.
            // Real implementation: use whatever string-input API the
            // installed eth-keystore version exposes. If only path-based
            // exists, document the trade-off and offer
            // `from_keystore_path(id, path, password)` as the primary path.
            decrypt_keystore_str(json, password)
        })
        .map_err(|e| SigningError::SignatureFailed(format!("keystore decrypt: {}", e)))?;

        if key_bytes.len() != 32 {
            return Err(SigningError::InvalidSignature(format!(
                "decrypted keystore key has length {} (expected 32)",
                key_bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&key_bytes);
        Self::from_bytes(id, arr)
    }

    /// Construct from a BIP-39 mnemonic + BIP-32 derivation path.
    pub fn from_mnemonic(
        id: SignerId,
        mnemonic: &str,
        derivation_path: &str,
    ) -> Result<Self, SigningError> {
        use bip32::DerivationPath;
        use bip39::{Language, Mnemonic};
        use std::str::FromStr;

        let mnemonic = Mnemonic::parse_in(Language::English, mnemonic)
            .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        let seed = mnemonic.to_seed("");
        let xprv = bip32::XPrv::new(&seed)
            .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        let path = DerivationPath::from_str(derivation_path)
            .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        let mut child = xprv;
        for n in path.iter() {
            child = child
                .derive_child(n)
                .map_err(|e| SigningError::SignatureFailed(e.to_string()))?;
        }
        let private_bytes = child.private_key().to_bytes();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&private_bytes);
        Self::from_bytes(id, arr)
    }

    /// EIP-55-checksummed Ethereum address derived from this key.
    pub fn address(&self) -> String {
        format!("{:#x}", self.inner.address())
    }
}

#[async_trait]
impl SignerProvider for LocalKeySigner {
    fn id(&self) -> &SignerId {
        &self.id
    }

    async fn sign(
        &self,
        request: SigningRequest,
    ) -> Result<SigningResponse, SigningError> {
        if !matches!(request.curve, Curve::Secp256k1) {
            return Err(SigningError::UnsupportedCurve(format!(
                "{:?}",
                request.curve
            )));
        }

        let signature = match request.payload_kind {
            SigningPayloadKind::TransactionDigest => {
                if request.payload.len() != 32 {
                    return Err(SigningError::UnsupportedPayload(format!(
                        "TransactionDigest expected 32 bytes, got {}",
                        request.payload.len()
                    )));
                }
                let digest = B256::from_slice(&request.payload);
                self.inner
                    .sign_hash_sync(&digest)
                    .map_err(|e| SigningError::SignatureFailed(e.to_string()))?
            }
            // Other payload kinds (UnsignedTransaction, Message, TypedData)
            // are deferred to follow-up PRs.
            other => {
                return Err(SigningError::UnsupportedPayload(format!(
                    "{:?} not supported by LocalKeySigner v1",
                    other
                )));
            }
        };

        let mut sig_bytes = Vec::with_capacity(65);
        sig_bytes.extend_from_slice(&signature.r().to_be_bytes::<32>());
        sig_bytes.extend_from_slice(&signature.s().to_be_bytes::<32>());
        sig_bytes.push(if signature.v() { 1 } else { 0 });

        // Public key uncompressed (65 bytes: 0x04 || X || Y).
        let pubkey_bytes = self
            .inner
            .credential()
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec();

        Ok(SigningResponse::SignatureOnly {
            signer: self.id.clone(),
            signature: sig_bytes,
            public_key: pubkey_bytes,
        })
    }
}

fn decrypt_keystore_str(
    _json: &str,
    _password: &str,
) -> Result<Vec<u8>, eth_keystore::KeystoreError> {
    // TODO during implementation: use the actual eth-keystore string-input
    // API for the version pinned in workspace. If only file-path-based is
    // available, write to a tempfile then call decrypt_key.
    Err(eth_keystore::KeystoreError::Json(serde_json::Error::custom(
        "use from_keystore_path for now",
    )))
}
```

- [ ] **Step 3: Compile-check**

```bash
cargo check -p atlas-signer-localkey
```

Resolve any version-specific alloy / bip32 / eth-keystore API mismatches by matching the exact pinned versions. The shapes should be close to current API surfaces.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-signer-localkey/Cargo.toml crates/atlas-signer-localkey/src/lib.rs
git commit -m "feat(signer-localkey): scaffold LocalKeySigner with 3 construction paths"
```

---

### Task 3.2: Raw-key construction test

**Files:**
- Create: `crates/atlas-signer-localkey/tests/raw_key.rs`

- [ ] **Step 1: Create the test**

```rust
use alloy_primitives::B256;
use atlas_core::chain::Curve;
use atlas_core::id::SignerId;
use atlas_core::signing::{
    SignerProvider, SigningPayloadKind, SigningRequest, SigningResponse,
};
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

const TEST_KEY: [u8; 32] = [
    0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e,
    0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32, 0x06,
    0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c,
    0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06,
];

fn make_signer() -> LocalKeySigner {
    LocalKeySigner::from_bytes(
        SignerId::from_str("test-key").unwrap(),
        TEST_KEY,
    )
    .unwrap()
}

#[tokio::test]
async fn signs_a_32_byte_digest() {
    let signer = make_signer();
    let digest = B256::random();
    let request = SigningRequest {
        account: atlas_core::id::AccountRef::from_str("account-1").unwrap(),
        network: atlas_core::id::NetworkId::from_str("eip155:1").unwrap(),
        curve: Curve::Secp256k1,
        payload_kind: SigningPayloadKind::TransactionDigest,
        payload: digest.to_vec(),
    };
    let response = signer.sign(request).await.unwrap();
    match response {
        SigningResponse::SignatureOnly { signature, public_key, .. } => {
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
```

- [ ] **Step 2: Run the test**

```bash
cargo test -p atlas-signer-localkey --test raw_key
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/atlas-signer-localkey/tests/raw_key.rs
git commit -m "test(signer-localkey): raw-key construction + sign + curve rejection"
```

---

### Task 3.3: Keystore + HD construction tests

**Files:**
- Create: `crates/atlas-signer-localkey/tests/keystore.rs`
- Create: `crates/atlas-signer-localkey/tests/hd.rs`

- [ ] **Step 1: Create `crates/atlas-signer-localkey/tests/hd.rs`**

```rust
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
```

- [ ] **Step 2: Create `crates/atlas-signer-localkey/tests/keystore.rs`**

```rust
//! Keystore tests. Skipped if eth-keystore's string-API is not stable on the
//! pinned version — falls back to a path-based round-trip test.
//!
//! To regenerate the fixture: use `cast wallet new --json -p`.

use atlas_core::id::SignerId;
use atlas_signer_localkey::LocalKeySigner;
use std::str::FromStr;

// Minimal test keystore — generated with eth-keystore::encrypt_key for
// the all-zeros key + password "test".
//
// For the implementation pass: regenerate this fixture and paste it in.
// The address derived from the all-zeros key is well-known:
// 0xfb6916095ca1df60bb79Ce92cE3Ea74c37c5d359.
const TEST_KEYSTORE_JSON: &str = r#"{"version":3, "id":"...", "address":"...", "crypto":{...}}"#;
const TEST_PASSWORD: &str = "test";

#[test]
#[ignore = "fixture must be regenerated; see comment in source"]
fn loads_a_known_keystore_and_signs() {
    let signer = LocalKeySigner::from_keystore(
        SignerId::from_str("test").unwrap(),
        TEST_KEYSTORE_JSON,
        TEST_PASSWORD,
    )
    .unwrap();
    let addr = signer.address();
    assert!(addr.starts_with("0x"));
}
```

- [ ] **Step 3: Run the HD test (keystore is `#[ignore]` until fixture lands)**

```bash
cargo test -p atlas-signer-localkey --test hd
```

Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-signer-localkey/tests/hd.rs crates/atlas-signer-localkey/tests/keystore.rs
git commit -m "test(signer-localkey): HD derivation + keystore scaffold"
```

> **Note on the ignored keystore test:** during implementation, generate a real keystore fixture (e.g. `cast wallet new --json -p test`) and paste the resulting JSON into the test, then drop the `#[ignore]`.

---

## Phase 4: End-to-end verification

### Task 4.1: End-to-end test in atlas-evm against a real local signer + mocked Provider

**Files:**
- Create: `crates/atlas-evm/tests/end_to_end.rs`

- [ ] **Step 1: Add `atlas-signer-localkey` to atlas-evm dev-deps**

In `crates/atlas-evm/Cargo.toml`, append to `[dev-dependencies]`:

```toml
atlas-signer-localkey = { path = "../atlas-signer-localkey" }
alloy-node-bindings = "0.5"   # optional: pull in if you want anvil; otherwise mock
```

- [ ] **Step 2: Create `crates/atlas-evm/tests/end_to_end.rs`**

```rust
//! End-to-end smoke test: real EvmCodec + real LocalKeySigner + real
//! signature recovery. The Provider isn't called for codec-only flow, so we
//! exercise the codec + signer round-trip directly without RPC.

use alloy_consensus::TxEnvelope;
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::Address;
use atlas_core::amount::RawAmount;
use atlas_core::asset::AssetStandard;
use atlas_core::fee::EvmFee;
use atlas_core::id::{AccountRef, AddressRef, AssetInstanceId, NetworkId, SignerId};
use atlas_core::service::ChainCodec;
use atlas_core::signing::SignerProvider;
use atlas_core::transaction::TransferIntent;
use atlas_evm::codec::{EvmCodec, EvmPrepareContext};
use atlas_signer_localkey::LocalKeySigner;
use num_bigint::BigInt;
use std::str::FromStr;

const TEST_KEY: [u8; 32] = [
    0x4c, 0x0d, 0xa3, 0xc7, 0xe6, 0x09, 0xa1, 0x6e,
    0x42, 0x06, 0x4e, 0x9c, 0x16, 0x1c, 0x32, 0x06,
    0x16, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c,
    0x32, 0x06, 0x9c, 0x32, 0x06, 0x9c, 0x32, 0x06,
];

#[tokio::test]
async fn build_sign_assemble_recovers_to_signer_address() {
    let codec = EvmCodec;
    let signer =
        LocalKeySigner::from_bytes(SignerId::from_str("test").unwrap(), TEST_KEY).unwrap();
    let expected_sender: Address = signer.address().parse().unwrap();

    let ctx = EvmPrepareContext {
        account: AccountRef::from_str(&signer.address()).unwrap(),
        network: NetworkId::from_str("eip155:1").unwrap(),
        intent: TransferIntent {
            asset_instance_id: AssetInstanceId::from_str("eip155:1/native:eth").unwrap(),
            to: AddressRef::from_str("0x0000000000000000000000000000000000000003").unwrap(),
            amount: RawAmount::new(BigInt::from(123_456u64), 18).unwrap(),
        },
        chain_id: 1,
        nonce: 5,
        fee: EvmFee::Eip1559 {
            max_fee_per_gas: BigInt::from(30_000_000_000u64),
            max_priority_fee_per_gas: BigInt::from(1_000_000_000u64),
            gas_limit: 21_000,
            l1_fee_wei: None,
        },
        standard: AssetStandard::Native,
        contract: None,
    };

    let unsigned = codec.prepare_transfer(ctx).unwrap();
    let request = codec.signing_request(&unsigned).unwrap();
    let response = signer.sign(request).await.unwrap();
    let signed = codec.assemble_signed(unsigned, response).unwrap();

    let envelope: TxEnvelope = TxEnvelope::decode_2718(&mut &signed.raw[..]).unwrap();
    let recovered = envelope.recover_signer().unwrap();
    assert_eq!(recovered, expected_sender);
}
```

- [ ] **Step 3: Run the test**

```bash
cargo test -p atlas-evm --test end_to_end
```

Expected: 1 test passes — recovered sender matches the local key's derived address.

- [ ] **Step 4: Commit**

```bash
git add crates/atlas-evm/Cargo.toml crates/atlas-evm/tests/end_to_end.rs
git commit -m "test(evm): end-to-end codec + LocalKeySigner round-trip"
```

---

## Phase 5: Polish

### Task 5.1: Update `deny.toml` for new transitive licenses

- [ ] **Step 1: Run cargo-deny and capture rejections**

```bash
cargo deny check 2>&1 | grep -B2 "license is" | head -30 || echo "no rejections"
```

Common transitives to expect (each has a permissive license already in our allow-list, but new ones may surface):
- BSD-3-Clause (already allowed)
- ISC (already allowed)
- Unicode-3.0 (already allowed)

- [ ] **Step 2: If new license types appear, extend the allow-list**

In `deny.toml`'s `[licenses]` `allow` array, append the new license id with a comment naming the transitive that brought it in. Example shape (only add if rejected):

```toml
allow = [
    # … existing …
    "0BSD",  # Pulled in transitively via <crate-name>
]
```

- [ ] **Step 3: Re-run**

```bash
cargo deny check
```

Expected: `advisories ok, bans ok, licenses ok, sources ok`.

- [ ] **Step 4: Commit any deny.toml change**

```bash
git add deny.toml
git commit -m "chore: extend deny.toml license allow-list for alloy transitives" \
  || echo "no deny.toml change needed"
```

---

### Task 5.2: README + crates table updates

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the Crates table**

In `README.md`, find the crates table and append two rows:

```markdown
| [`atlas-evm`](crates/atlas-evm/) | Real EVM `ChainService` on alloy — codec / reader / fee estimator / broadcaster / orchestrator | (test count after implementation) |
| [`atlas-signer-localkey`](crates/atlas-signer-localkey/) | secp256k1 in-process signer (raw key / JSON keystore / BIP-32 HD derivation), implements `SignerProvider` | (test count after implementation) |
```

Replace `(test count after implementation)` with the actual counts once Phase 1–4 land.

- [ ] **Step 2: Update the verification highlights**

Find the line currently reading:

```markdown
🧪 **Verification rigor** — unit + fixture + smoke + BDD scenarios, 99.88% line coverage, …
```

If coverage shifts after this PR, recompute:

```bash
cargo +nightly llvm-cov --workspace --all-features --ignore-filename-regex 'atlas-scenarios' --summary-only | tail -3
```

Update the percentage in the README to match the actual TOTAL line coverage.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README mentions atlas-evm + atlas-signer-localkey"
```

---

### Task 5.3: Final full-workspace verification

- [ ] **Step 1: fmt + clippy**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: clean.

- [ ] **Step 2: Full test suite**

```bash
cargo test --workspace --all-features
```

Expected: all unit + integration tests pass across all 5 crates.

- [ ] **Step 3: BDD**

```bash
cargo test -p atlas-scenarios --test atlas_bdd
```

Expected: `7 scenarios (7 passed)`, `30 steps (30 passed)`.

- [ ] **Step 4: doc + deny + WASM build**

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --all-features
cargo deny check
cargo build -p atlas-core --target wasm32-unknown-unknown
cargo build -p atlas-evm --target wasm32-unknown-unknown 2>&1 | tail -10
```

Expected: doc + deny clean. atlas-core builds for WASM. atlas-evm may or may not build for WASM (alloy-provider's HTTP transport requires reqwest which has WASM caveats); if it doesn't, document the limitation and skip the WASM build step for atlas-evm.

- [ ] **Step 5: Coverage**

```bash
cargo +nightly llvm-cov --workspace --all-features --ignore-filename-regex 'atlas-scenarios' --summary-only | tail -3
```

Expected: workspace coverage ≥98% (atlas-evm and atlas-signer-localkey have their own targeted tests; some integration paths in atlas-evm may need mocked-Provider tests for full coverage — those can land in a follow-up).

- [ ] **Step 6: Commit any drive-by formatting**

```bash
git status
# if anything is unstaged:
git add -A
git commit -m "style: rustfmt"
```

---

### Task 5.4: Open the PR

- [ ] **Step 1: Push the branch**

```bash
git push -u origin feat/evm-chain-service
```

- [ ] **Step 2: Open the PR**

Use the `gh pr create` template that PRs #1–#7 in this repo follow. Title:

`feat(evm): real EVM ChainService on alloy + LocalKeySigner reference impl`

Body should reference:
- The spec: `docs/superpowers/specs/2026-05-08-atlas-chain-service-evm-design.md`
- The 5-trait split summary
- Test plan checklist (fmt / clippy / test / BDD / doc / WASM / deny / coverage)
- Note that this is large (~1500 LOC) but the commit history is granular

---

## Self-review notes

This plan was reviewed against the spec at
`docs/superpowers/specs/2026-05-08-atlas-chain-service-evm-design.md` for:

- ✅ All 5 traits implemented (Tasks 1.3, 2.6–2.9 + orchestrator)
- ✅ `MockEvm*` impls keep smoke flow + BDD scenarios passing (Tasks 1.4, 1.5)
- ✅ alloy-based `EvmCodec` for legacy + EIP-1559 + ERC-20 (Tasks 2.3, 2.4)
- ✅ Round-trip recovery test (Task 2.5, Task 4.1)
- ✅ Three LocalKeySigner construction paths (Task 3.1)
- ✅ Keystore + HD tests (Task 3.3)
- ✅ ChainError extensions (Task 1.2)
- ✅ Fee + TransactionStatus types (Task 1.1)
- ✅ Workspace deps + member registration (Task 2.1)
- ✅ deny.toml + README updates (Tasks 5.1, 5.2)
- ✅ Final verification matrix (Task 5.3)

**Remaining ambiguities flagged for execution-time decisions:**

1. **Exact alloy version** — pinned during implementation via `cargo add`.
   The shapes in this plan target alloy 0.5–0.6 era APIs.
2. **`eth-keystore` string-input API** — the version pinned may have only
   path-based decryption. Task 3.1's `from_keystore` includes a fallback
   shim; refactor to `from_keystore_path` if the string API isn't there.
3. **`atlas-evm` WASM build** — alloy-provider's HTTP transport may not
   build for `wasm32-unknown-unknown`. If so, gate the WASM CI job to
   atlas-core only (already the current setup) and document the limitation.
4. **`alloy-rpc-types-eth::TransactionRequest` constructor** — the exact
   builder method names (`.to(addr)`, `.input(bytes)`) may differ on the
   pinned version; adjust call sites.
5. **OP-Stack L1 fee oracle** — left as `None` in `EvmFee::Eip1559.l1_fee_wei`
   for v1. Add a follow-up PR to call the predeploy at
   `0x420000000000000000000000000000000000000F`'s `getL1Fee(bytes)`.

These are tactical adjustments the implementer makes when they hit them — none
of them invalidate the architecture.
