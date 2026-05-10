# atlas-evm

Real EVM `ChainService` for the Atlas SDK, built on [alloy](https://github.com/alloy-rs/alloy) 2.x. Implements all five [`atlas_core::service`](../atlas-core/src/service.rs) traits plus the orchestrator.

`#![forbid(unsafe_code)]`.

## Topology

```text
                                 atlas-evm
                  ┌─────────────────────────────────────────────┐
                  │                                             │
                  │   EvmCodec ─── pure RLP                     │
                  │   (no I/O)     legacy + EIP-1559            │
                  │                keccak256 digest             │
                  │                EIP-2718 envelope            │
                  │                                             │
                  │   EvmReader ─────────────┐                  │
                  │       eth_getBalance     │                  │
                  │       eth_getTxCount     │                  │
                  │       eth_getTxReceipt   │                  │
                  │                          │                  │
                  │   EvmFeeEstimator ───────┼─► alloy::Provider│
                  │       eth_feeHistory     │   (mockable)     │
                  │                          │                  │
                  │   EvmBroadcaster ────────┘                  │
                  │       eth_sendRawTransaction                │
                  │                                             │
                  │   ┌────────────────────────────────────┐    │
                  │   │ EvmChainService<P: Provider>       │    │
                  │   │   prepare_unsigned_bundle          │    │
                  │   │   assemble_and_broadcast           │    │
                  │   │   transfer (composes the two)      │    │
                  │   └────────────────────────────────────┘    │
                  │                                             │
                  │   mock::MockEvmChainService                 │
                  │       in-tree mock for smoke tests + BDD    │
                  │                                             │
                  └─────────────────────────────────────────────┘
                                       │
                                       │ atlas-core trait surface
                                       ▼
                            ChainCodec / ChainReader /
                            FeeEstimator / ChainBroadcaster /
                                      ChainService
```

## What's in each module

| Module | Role |
|---|---|
| `codec` | `EvmCodec` + `EvmPrepareContext`. Pure: builds RLP unsigned bytes for legacy + EIP-1559, computes keccak256 signing digest, assembles signed envelope from any `SigningResponse` shape. No `Provider`. |
| `abi` | Hand-rolled `transfer(address,uint256)` ABI encoder. Avoids pulling `alloy-sol-types` into the build for a single 4-byte selector + two static encodes. |
| `reader` | `EvmReader<P: Provider>` — native + ERC-20 balance, nonce, tx receipt → `TransactionStatus`. |
| `fee_estimator` | `EvmFeeEstimator<P>` — delegates EIP-1559 estimation to `Provider::estimate_eip1559_fees()` (alloy's MetaMask-modelled algorithm); legacy `eth_gasPrice` fallback. Atlas contributes the per-asset gas-floor selection and the `EvmFee` envelope shape. OP-Stack L1 fee oracle deferred (`l1_fee_wei: None` for now). |
| `broadcaster` | `EvmBroadcaster<P>` — `eth_sendRawTransaction`. |
| `service` | `EvmChainService<P>` — orchestrator. Exposes `prepare_unsigned_bundle` (server-side build), `assemble_and_broadcast` (client-side finalize), and `transfer` (composition). |
| `mock` | `MockEvmChainService` — implements all 5 traits with deterministic `0xmock` output. Used by smoke tests and BDD scenarios; lets downstream consumers test integration without real RPC. |
| `error` | `map_transport_err` — bridges `alloy_transport::TransportError` into `atlas_core::error::RpcError`. |

## Two deployment shapes

### Direct (single process)

```rust
let service = EvmChainService::new(provider, network, chain_id, eip1559);
let result  = service.transfer(intent, account, &local_signer).await?;
```

### Split-host (server builds, client signs)

```rust
// Server: holds reader + fee estimator + codec, no signer
let bundle = server_service.prepare_unsigned_bundle(intent, account).await?;
let json   = serde_json::to_string(&bundle)?;          // ship over wire

// Client: holds signer + codec + broadcaster, no reader/estimator
let bundle: UnsignedBundle = serde_json::from_str(&json)?;
let response = client_signer.sign(bundle.signing_request).await?;
let result   = client_service
    .assemble_and_broadcast(bundle.unsigned, response)
    .await?;
```

The codec path is `Send + Sync` and pure; the only state crossing the wire is the `UnsignedBundle` JSON. See [`tests/split_host_flow.rs`](tests/split_host_flow.rs) for the working test that uses two independent mocked providers to prove the seam.

## Where we use alloy vs roll our own

| Layer | Source |
|---|---|
| RLP encoding (legacy + EIP-1559), `TxEnvelope`, EIP-2718 | `alloy_consensus`, `alloy_eips` |
| `keccak256`, `Address`, `B256`, `U256`, `Signature` | `alloy_primitives` |
| `Provider` + `eth_*` JSON-RPC methods | `alloy_provider` |
| ERC-20 `transfer(address,uint256)` calldata | hand-rolled ([`abi.rs`](src/abi.rs)) |
| EIP-1559 fee estimation algorithm | `alloy_provider::Provider::estimate_eip1559_fees` |
| Signing | atlas's `SignerProvider` (chain-agnostic) — alloy's `WalletFiller` is not used |

## Testing

```bash
cargo test -p atlas-evm
```

See [`tests/README.md`](tests/README.md) for the layout — codec tests, RPC-mocked integration tests, end-to-end recovery tests, split-host proof.

## License

Licensed under the [MIT License](../../LICENSE).
