# Atlas ChainService — EVM (alloy) + LocalKeySigner

**Status:** Design • **Author:** Atlas core • **Date:** 2026-05-08

## 1. Goal

Replace `atlas-core::service::ChainService` (single fat trait) with a focused
five-trait split, then ship the first real implementation: `atlas-evm` on top of
[alloy](https://github.com/alloy-rs/alloy), and a reference signer
`atlas-signer-localkey`. After this work, downstream apps can build, sign, and
broadcast real EVM transfers end-to-end without depending on a mock.

The split is driven by one principle: **anything that needs the network is a
separate seam from anything that's pure encoding**. That makes the
"server-builds-tx, client-signs-with-MPC" deployment fall out naturally — the
codec runs without RPC, the readers/fee-estimator/broadcaster run with RPC, and
the signer is a separate trait already.

### Non-goals (deferred to follow-ups; tracked in [`ROADMAP.md`](../../../ROADMAP.md))

- Real Solana / Sui / UTXO `ChainService` implementations.
- MPC, Privy, ERC-4337 account-abstraction signer adapters.
- ERC-20 approval and arbitrary contract-call intents (`ApproveIntent`,
  `ContractCallIntent`).
- `AddressRef` per-chain format validation (EIP-55 / base58 / chain-aware).
- WalletConnect, EIP-712 typed-data signing, message signing.
- Multi-key / threshold signers, MPC quorums.

## 2. Why alloy (and not `tw_evm` or roll-our-own)

[`tw_evm`](https://github.com/trustwallet/wallet-core/tree/master/rust/tw_evm)
is a serious option — Trust Wallet's Rust crate is battle-tested and powers the
KMP project's EVM signing through JNI. We're not using it because:

| | `tw_evm` (Trust Wallet) | `alloy` |
|---|---|---|
| API shape | Protobuf `SigningInput` / `SigningOutput`, codegen-driven | Native Rust types, modular crates |
| FFI orientation | Designed around C ABI | Pure Rust |
| MPC support | 3-phase: `sign` / `preimage_hashes` / `compile` | 3-phase: `signature_hash` / `into_signed` |
| Coverage | EVM + 24 other chains | EVM ecosystem only |
| Modularity | Monorepo crate | `alloy-consensus`, `alloy-rlp`, `alloy-signer`, `alloy-provider` etc. |
| Maintenance | Trust Wallet team | Foundry team (current Rust EVM standard) |

Both support the externally-signed pattern Atlas needs. Alloy wins on idiomatic
Rust, smaller dep surface, and natural alignment with our `SignerProvider`
boundary. Rolling our own RLP / signing math is a non-starter for time-to-real.

## 3. Architecture

Three crates touched by this PR:

```
atlas-core (refactor)
  ├── trait ChainCodec       (pure: encode / sign-request / assemble)
  ├── trait ChainReader      (RPC: balance, nonce, status)
  ├── trait FeeEstimator     (RPC: estimate_fee — assoc Fee type)
  ├── trait ChainBroadcaster (RPC: send signed)
  ├── trait ChainService     (orchestrator: composes the 4 above)
  ├── trait SignerProvider   (existing, unchanged contract)
  └── MockEvm{Codec, Reader, FeeEstimator, Broadcaster, ChainService}

atlas-evm (new — real EVM impl on alloy)
  ├── EvmCodec            (alloy_consensus::TxEip1559 / TxLegacy + RLP)
  ├── EvmReader           (RootProvider<Http>)
  ├── EvmFeeEstimator     (eth_feeHistory + OP-Stack L1 oracle)
  ├── EvmBroadcaster      (eth_sendRawTransaction)
  └── EvmChainService     (orchestrator)

atlas-signer-localkey (new — in-process secp256k1 signer)
  └── LocalKeySigner      (raw / JSON keystore / BIP-32 HD)
```

### Why per-chain associated types

`ChainCodec::PrepareContext` is per-chain because the input fields differ
materially: EVM needs `chain_id` + `nonce` + `EvmFee`, Solana needs
`recent_blockhash` + compute-unit settings, UTXO needs UTXO selection. Forcing
all of these into a chain-agnostic struct with `Option<…>` fields would push
validation off the type system and into runtime. Same reasoning for
`FeeEstimator::Fee`.

`UnsignedTransaction`, `SignedTransaction`, `SigningRequest`, `SigningResponse`,
`BroadcastResult` stay chain-agnostic. The `payload: Vec<u8>` inside them is
opaque to atlas-core; each codec interprets and re-encodes them.

## 4. Trait contracts

```rust
// atlas-core/src/service.rs (refactored)

pub trait ChainCodec: Send + Sync {
    type PrepareContext;

    /// Encode a transfer intent into chain-specific unsigned bytes.
    /// Pure — no RPC, no I/O.
    fn prepare_transfer(
        &self,
        ctx: Self::PrepareContext,
    ) -> Result<UnsignedTransaction, ChainError>;

    /// Produce the signing request a SignerProvider should sign over.
    /// For EVM this is the keccak256 digest with `payload_kind = TransactionDigest`.
    fn signing_request(
        &self,
        unsigned: &UnsignedTransaction,
    ) -> Result<SigningRequest, ChainError>;

    /// Combine the unsigned transaction with whichever SigningResponse shape
    /// the signer returned, producing broadcast-ready bytes.
    fn assemble_signed(
        &self,
        unsigned: UnsignedTransaction,
        response: SigningResponse,
    ) -> Result<SignedTransaction, ChainError>;
}

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

#[async_trait]
pub trait FeeEstimator: Send + Sync {
    type Fee;

    async fn estimate_fee(
        &self,
        intent: &TransferIntent,
        sender: &AddressRef,
    ) -> Result<Self::Fee, ChainError>;
}

#[async_trait]
pub trait ChainBroadcaster: Send + Sync {
    async fn broadcast(
        &self,
        signed: SignedTransaction,
    ) -> Result<BroadcastResult, ChainError>;
}

#[async_trait]
pub trait ChainService: Send + Sync {
    type PrepareContext;
    type Fee;

    /// Convenience entry point — fetch nonce, estimate fee, prepare,
    /// request signing, assemble, broadcast. Apps that want fine control
    /// compose the four sub-traits directly.
    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn SignerProvider,
    ) -> Result<BroadcastResult, ChainError>;
}
```

`TransactionStatus` is new and chain-agnostic:

```rust
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
```

## 5. Fee types

`Fee` is a chain-agnostic bag in `atlas-core`. Each chain's
`FeeEstimator::Fee` associated type points at the appropriate variant; the
top-level enum exists for product-layer code that aggregates fees across
chains.

```rust
// atlas-core/src/fee.rs (new module)

pub enum Fee {
    Evm(EvmFee),
    // future: Solana(SolanaFee), Sui(SuiFee), …
}

pub enum EvmFee {
    Legacy {
        gas_price: BigInt,
        gas_limit: u64,
    },
    Eip1559 {
        max_fee_per_gas: BigInt,
        max_priority_fee_per_gas: BigInt,
        gas_limit: u64,
        /// OP-Stack L1 data fee (set on Base / Optimism / etc.).
        l1_fee_wei: Option<BigInt>,
    },
}

impl EvmFee {
    /// Total worst-case fee in wei: `gas_limit * effective_gas_price + l1_fee`.
    pub fn max_fee_wei(&self) -> BigInt { ... }
}
```

`atlas-evm`'s `EvmFeeEstimator::Fee = EvmFee` (no `Fee::Evm` wrapper at the
trait boundary — the wrapper is for higher-layer aggregation only).

## 6. atlas-evm — alloy bridge

```rust
// atlas-evm/src/codec.rs

pub struct EvmCodec;

pub struct EvmPrepareContext {
    pub account: AccountRef,
    pub network: NetworkId,
    pub intent: TransferIntent,
    pub chain_id: u64,
    pub nonce: u64,
    pub fee: EvmFee,
}

impl ChainCodec for EvmCodec {
    type PrepareContext = EvmPrepareContext;

    fn prepare_transfer(&self, ctx: EvmPrepareContext)
        -> Result<UnsignedTransaction, ChainError>
    {
        // 1. Resolve native vs ERC-20 from ctx.intent.asset_instance_id
        //    (matches AssetStandard variant in the registry-resolved instance).
        // 2. Build alloy_consensus::TxEip1559 (or TxLegacy) from ctx fields:
        //      - to: parse intent.to as Address
        //      - value: 0 for ERC-20, intent.amount.value for native
        //      - input: ABI-encoded transfer(to, amount) for ERC-20, empty for native
        //      - chain_id, nonce, gas_*, max_*: from ctx.fee
        // 3. RLP-encode for signing → bytes go in UnsignedTransaction.payload.
        // 4. Return UnsignedTransaction { account, network, intent, payload }.
    }

    fn signing_request(&self, unsigned: &UnsignedTransaction)
        -> Result<SigningRequest, ChainError>
    {
        // Decode TxEnvelope from unsigned.payload, compute keccak256(signature_hash).
        // Return SigningRequest {
        //   account: unsigned.account, network: unsigned.network,
        //   curve: Curve::Secp256k1,
        //   payload_kind: SigningPayloadKind::TransactionDigest,
        //   payload: digest.to_vec(),
        // }
    }

    fn assemble_signed(&self, unsigned: UnsignedTransaction, response: SigningResponse)
        -> Result<SignedTransaction, ChainError>
    {
        // SigningResponse::SignatureOnly { signature, .. }
        //   → Parse alloy_primitives::Signature from 65 bytes (r,s,v).
        //   → tx.into_signed(sig); Signed::encoded_2718() → SignedTransaction.raw.
        // SigningResponse::SignedTransaction { raw, .. }
        //   → Use raw bytes directly (signer is responsible for correct encoding).
        // SigningResponse::SubmittedTransaction { .. }
        //   → ChainError::TransactionBuildFailed("signer broadcast already").
    }
}
```

```rust
// atlas-evm/src/reader.rs

pub struct EvmReader<P: alloy_provider::Provider> {
    provider: P,
}

#[async_trait]
impl<P: alloy_provider::Provider + Clone> ChainReader for EvmReader<P> {
    async fn get_balance(&self, instance: &AssetInstance, address: &AddressRef)
        -> Result<RawAmount, ChainError>
    {
        match instance.standard {
            AssetStandard::Native => {
                let balance = self.provider.get_balance(parse_address(address)?).await?;
                RawAmount::new(balance_to_bigint(balance), instance.decimals)
                    .map_err(|e| ChainError::TransactionBuildFailed(e.to_string()))
            }
            AssetStandard::Erc20 => {
                // eth_call balanceOf(address) on instance.contract
            }
            AssetStandard::Spl => Err(ChainError::UnsupportedAssetInstance(instance.id.clone())),
        }
    }
    // get_nonce, get_transaction_status: thin alloy wrappers
}
```

`EvmFeeEstimator`, `EvmBroadcaster` similarly wrap alloy primitives:
`eth_feeHistory` for EIP-1559 priority-fee suggestion (median of 50th-percentile
rewards over a sliding window of 10 blocks, clamped between
`MIN_PRIORITY_FEE = 0.001 gwei` and `MAX_PRIORITY_FEE = 0.2 gwei`),
`max_fee_per_gas = base_fee * 2 + max_priority_fee_per_gas`.

OP-Stack L1 fee oracle (`getL1Fee(bytes)` on the predeploy at
`0x420000000000000000000000000000000000000F`) is consulted when
`Network.features.op_stack_l1_fee == true`. Falls back to legacy
(`eth_gasPrice`) if `eth_feeHistory` returns empty data.

```rust
// atlas-evm/src/service.rs — orchestrator

pub struct EvmChainService<P: alloy_provider::Provider + Clone> {
    pub codec: EvmCodec,
    pub reader: EvmReader<P>,
    pub fee_estimator: EvmFeeEstimator<P>,
    pub broadcaster: EvmBroadcaster<P>,
    /// Network this orchestrator is bound to. One service per network;
    /// apps spinning up multiple networks construct one EvmChainService each.
    pub network: NetworkId,
    /// EVM chain id (numeric form), parsed from `Network.chain_id` at
    /// construction. Used directly in EIP-155 replay protection.
    pub chain_id: u64,
}

#[async_trait]
impl<P: alloy_provider::Provider + Clone> ChainService for EvmChainService<P> {
    type PrepareContext = EvmPrepareContext;
    type Fee = EvmFee;

    async fn transfer(
        &self,
        intent: TransferIntent,
        account: AccountRef,
        signer: &dyn SignerProvider,
    ) -> Result<BroadcastResult, ChainError> {
        // 1. Validate intent.asset_instance_id belongs to self.network
        //    (returns ChainError::UnsupportedAssetInstance otherwise — same
        //    rule MockEvmService enforces today).
        // 2. Derive the sender address from the signer's public key. atlas-core
        //    doesn't know how, so callers either pass the address explicitly
        //    or use a higher-level helper that does the derivation per-curve.
        //    For this orchestrator method the sender is taken from the signer
        //    via a small chain-aware helper exposed by atlas-evm.
        // 3. Concurrent: fetch nonce + estimate fee.
        // 4. Codec::prepare_transfer with full EvmPrepareContext.
        // 5. Codec::signing_request → signer.sign → Codec::assemble_signed.
        // 6. Broadcaster::broadcast.
    }
}
```

## 7. atlas-signer-localkey

```rust
pub struct LocalKeySigner {
    inner: alloy_signer_local::PrivateKeySigner,
    id: SignerId,
}

impl LocalKeySigner {
    /// Raw 32-byte secp256k1 private key.
    pub fn from_bytes(id: SignerId, bytes: [u8; 32]) -> Result<Self, SigningError>;

    /// Web3 secret-storage JSON keystore (scrypt KDF, AES-128-CTR cipher).
    pub fn from_keystore(id: SignerId, json: &str, password: &str)
        -> Result<Self, SigningError>;

    /// BIP-39 mnemonic + BIP-32 derivation path
    /// (e.g. "m/44'/60'/0'/0/0" for the first Ethereum account).
    pub fn from_mnemonic(id: SignerId, mnemonic: &str, derivation_path: &str)
        -> Result<Self, SigningError>;

    /// Derived address (EIP-55 checksummed). Useful when the caller only
    /// has the signer and needs to know its public address.
    pub fn address(&self) -> String;
}

#[async_trait]
impl SignerProvider for LocalKeySigner {
    fn id(&self) -> &SignerId { &self.id }

    async fn sign(&self, request: SigningRequest)
        -> Result<SigningResponse, SigningError>
    {
        // Validate request.curve == Curve::Secp256k1.
        // Validate request.payload_kind ∈ { TransactionDigest, Message }.
        // For TransactionDigest: sign the 32-byte digest directly.
        // For Message: prepend EIP-191 prefix, hash, sign.
        // Return SigningResponse::SignatureOnly {
        //   signer: self.id.clone(),
        //   signature: 65-byte (r ‖ s ‖ v),
        //   public_key: 65-byte uncompressed pubkey,
        // }
    }
}
```

The signer is **chain-agnostic at the signing step**. It signs over a digest
on a curve. The chain codec is responsible for assembly. This is what makes the
same `LocalKeySigner` usable for EVM today and any future secp256k1-based chain.

## 8. Migration of existing types & tests

| What | Change |
|---|---|
| `atlas-core::service::ChainService` (current single trait) | Removed; replaced with the 5-trait split |
| `MockEvmService` | Becomes `MockEvmCodec`, `MockEvmReader`, `MockEvmFeeEstimator`, `MockEvmBroadcaster`, `MockEvmChainService` (single struct that implements all five for ergonomics) |
| `crates/atlas-core/tests/smoke_flow.rs` | Updated to use the new 5-trait shape via `MockEvmChainService` |
| `crates/atlas-scenarios/src/world.rs` + `steps.rs` | Updated to the 5-trait API; existing 7 BDD scenarios pass unchanged |
| `crates/atlas-verify/tests/properties.rs` | `validate_shape_partition_is_total` and others stay; no signature change to pure types |
| `crates/atlas-verify/src/proofs.rs` | Kani proofs unaffected (operate on registry-only surfaces) |

The refactor is a single cohesive PR; CodeRabbit gets the full picture.

## 9. Testing strategy

- **Unit (atlas-core)**: `MockEvm*` impls keep their existing 99.75%+ coverage.
- **Unit (atlas-evm)**:
  - `EvmCodec::prepare_transfer` round-trip — assert RLP bytes match a
    hand-built alloy `TxEip1559` digest.
  - `EvmCodec::signing_request` keccak256 digest equals
    `alloy_consensus::SignableTransaction::signature_hash`.
  - `EvmCodec::assemble_signed` for each `SigningResponse` variant
    (success → recovered sender matches; submitted → typed error).
- **Unit (atlas-signer-localkey)**:
  - Round-trip raw / keystore / HD construction.
  - Sign + recover pubkey matches; signature is 65 bytes; v parity correct.
- **Integration (atlas-evm)**: mocked `alloy_provider::Provider` fakes
  `eth_getBalance`, `eth_getTransactionCount`, `eth_feeHistory`,
  `eth_sendRawTransaction`. End-to-end happy path through `EvmChainService`.
- **End-to-end smoke** (atlas-evm): real `EvmCodec` + real `LocalKeySigner` +
  mocked Provider. Build → digest → sign → assemble → decode → assert
  recovered sender = local key address.
- **Anvil-backed integration** (optional, `#[ignore]` or feature-gated): real
  Anvil node spawned via `alloy-node-bindings` for one happy-path transfer.
  Not in CI's main matrix to keep wall time low.
- **BDD** (atlas-scenarios): existing 7 scenarios pass against post-refactor
  `MockEvm*`. Optionally add one new scenario exercising real EVM build
  through atlas-evm with mocked Provider.
- **Coverage** target: ≥99.5% on atlas-core post-refactor; atlas-evm and
  atlas-signer-localkey get their own targeted property tests for round-trips
  and assembly invariants.
- **Bolero** unchanged for now; future round on RLP encoding once the codec
  lands.
- **Kani** unchanged.
- **WASM build** must still pass for `atlas-core`. `atlas-evm` likely builds on
  `wasm32-unknown-unknown` too (alloy supports it) but will be confirmed
  during implementation.

## 10. Errors

`ChainError` gains a few variants for the alloy paths:

```rust
pub enum ChainError {
    // existing:
    UnsupportedChain, UnsupportedNetwork, UnsupportedAssetInstance,
    InvalidAddress, FeeEstimationFailed, TransactionBuildFailed, BroadcastFailed,
    // new:
    /// RPC transport / provider error (wraps RpcError with context).
    Rpc(RpcError),
    /// `Network.chain_id` was missing or non-numeric for an EVM operation.
    MissingChainId(NetworkId),
    /// Asset standard is supported by the registry but not by this codec
    /// (e.g. SPL passed to the EVM codec).
    StandardNotSupported {
        instance: AssetInstanceId,
        standard: AssetStandard,
    },
}
```

`alloy::transports::TransportError` and `alloy::contract::Error` map into
`ChainError::Rpc(RpcError::*)` at the boundary.

## 11. Dependencies (new)

Workspace `Cargo.toml`:

```toml
alloy-consensus    = "0.x"
alloy-rlp          = "0.x"
alloy-primitives   = "0.x"
alloy-network      = "0.x"
alloy-provider     = "0.x"
alloy-signer       = "0.x"
alloy-signer-local = "0.x"
alloy-rpc-types-eth = "0.x"
eth-keystore       = "0.x"   # JSON keystore (scrypt KDF + AES-128-CTR)
bip32              = "0.x"   # BIP-32 HD derivation
bip39              = "0.x"   # mnemonic → seed
```

Exact versions resolved during implementation; pinned via `cargo update` then
fixed in `Cargo.lock`.

`deny.toml` license allow-list extends with whatever new transitives bring in
(historically Apache-2.0, MIT, BSD-3, ISC). `BlueOak-1.0.0` is already
allow-listed.

CI:
- WASM build job already runs against atlas-core; verify atlas-evm builds too
  during implementation.
- BDD job unaffected.
- Coverage job adds the new crates automatically (workspace).

## 12. PR shape

One cohesive PR, in the order:

1. atlas-core trait refactor (5 traits, MockEvm split, smoke + scenarios + verify
   updated).
2. New crate `atlas-evm` with codec / reader / fee-estimator / broadcaster /
   service + targeted unit tests.
3. New crate `atlas-signer-localkey` with raw / keystore / HD impls + signer
   tests.
4. End-to-end smoke test in atlas-evm tying the two together against a mocked
   alloy Provider.

Estimated size: ~1500–1800 LOC. Big, but the split into commits makes review
manageable.
