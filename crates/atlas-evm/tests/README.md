# atlas-evm integration tests

Integration tests for [`atlas-evm`](..). Unit tests live alongside the source in `src/*.rs` under `#[cfg(test)]`; this directory is everything that exercises a real codec round-trip, a mocked `alloy::Provider`, or a cross-crate composition.

## Layout

```text
              tests/
              │
              │ ── codec (pure, no Provider) ─────────────────────
              │
              ├── codec_native.rs            native-asset RLP shape
              ├── codec_erc20.rs             ERC-20 calldata + RLP
              ├── codec_assemble.rs          signing-request →
              │                              signed-envelope round-trip
              │                              (EIP-1559 + legacy)
              ├── codec_errors.rs            error paths in
              │                              prepare_transfer / assemble
              │
              │ ── RPC seam (mocked alloy Provider) ──────────────
              │
              ├── reader_tests.rs            EvmReader against
              │                              alloy_provider::mock::Asserter
              ├── fee_estimator_tests.rs     EvmFeeEstimator
              │                              eth_feeHistory + clamping
              ├── broadcaster_tests.rs       EvmBroadcaster
              │                              eth_sendRawTransaction
              ├── service_tests.rs           EvmChainService::transfer
              │                              orchestration + error paths
              │
              │ ── end-to-end ────────────────────────────────────
              │
              ├── end_to_end.rs              real codec + real
              │                              LocalKeySigner →
              │                              recover_signer matches
              ├── smoke_flow.rs              MockEvmChainService
              │                              over the official registry
              └── split_host_flow.rs         server-builds /
                                             client-signs across two
                                             independent providers
                                             via JSON wire boundary
```

## How the layers fit

```text
                       per-test claim
                            │
   ┌────────────────────────┼─────────────────────────┐
   │                        │                         │
   │   "encoding is         "RPC integration         "the trait split
   │   correct"             works"                   actually works"
   │                                                  │
   │   codec_*.rs           reader_tests.rs            end_to_end.rs
   │      no Provider       fee_estimator_tests.rs    smoke_flow.rs
   │      no signer         broadcaster_tests.rs      split_host_flow.rs
   │                        service_tests.rs            (this is the
   │                                                     architectural
   │                                                     proof — see
   │                                                     below)
   │
   └────────────── all share atlas_core trait surface ──────────────┘
```

## Two flagship tests

### `end_to_end.rs`

Builds a transaction with the real `EvmCodec`, signs the keccak256 digest with a real `LocalKeySigner`, decodes the resulting envelope with `TxEnvelope::decode_2718`, and asserts that `envelope.recover_signer()` returns the signer's own address. Round-trips the codec ↔ signer seam without any RPC.

### `split_host_flow.rs`

Two `#[tokio::test]`s, two architectural claims:

| Test | Claim |
|---|---|
| `direct_mode_in_process_transfer` | One process, one provider, one service, full `ChainService::transfer`. Hash matches the mocked broadcast. |
| `split_host_server_builds_client_signs_and_broadcasts` | A "server" with no signer builds via `prepare_unsigned_bundle`. The bundle survives a JSON round-trip. A "client" with no reader/estimator (separate mocked provider) signs and calls `assemble_and_broadcast`. The signed envelope's `recover_signer` matches the signer's address — proving the codec on the client composed correctly with what the server prepared. |

This pair is the load-bearing demonstration that the 5-trait split decomposes across a wire boundary.

## Mocking pattern

Every RPC-touching test follows the same shape:

```rust
let asserter = Asserter::new();
asserter.push_success(&alloy_primitives::U64::from(0u64));    // eth_getTransactionCount
asserter.push_success(&FeeHistory { … });                     // eth_feeHistory
asserter.push_success(&B256::repeat_byte(0xab));              // eth_sendRawTransaction

let provider = ProviderBuilder::new()
    .disable_recommended_fillers()
    .connect_mocked_client(asserter);
```

`disable_recommended_fillers()` is important — alloy's default fillers would fire extra RPC calls (chain ID, gas estimation) that the mock isn't primed to answer.

## Running

```bash
# Whole crate
cargo test -p atlas-evm

# Just one suite
cargo test -p atlas-evm --test split_host_flow
cargo test -p atlas-evm --test end_to_end
```

## Coverage snapshot

The atlas-evm tests pull workspace coverage to ~96% lines. The residual lives in defensive guards (e.g. `parse_65_byte_signature` rejects on byte values that no Atlas signer emits) and a `llvm-cov` quirk on generic functions in `error.rs`.
