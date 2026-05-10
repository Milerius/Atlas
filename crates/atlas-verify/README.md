# atlas-verify

Verification harness for atlas-core. Two layers, both opt-in to the rest of the workspace's CI.

`#![forbid(unsafe_code)]`. `publish = false`.

## Layers

```text
                        atlas-verify
                  ┌─────────────────────────────────────┐
                  │                                     │
                  │   tests/properties.rs               │
                  │     ┌─────────────────────────┐     │   per-PR
                  │     │ Bolero property tests   │     │   cargo test
                  │     │   generates input via   │     │   -p atlas-verify
                  │     │   Arbitrary, asserts    │     │
                  │     │   hand-written rules    │     │
                  │     └─────────────────────────┘     │
                  │                                     │
                  │   src/proofs.rs    #[cfg(kani)]     │
                  │     ┌─────────────────────────┐     │   nightly
                  │     │ Kani bounded model      │     │   cargo kani
                  │     │   checking — narrow on  │     │   -p atlas-verify
                  │     │   purpose (BigInt and   │     │
                  │     │   BTreeMap don't        │     │
                  │     │   symbolic-execute      │     │
                  │     │   well today)           │     │
                  │     └─────────────────────────┘     │
                  │                                     │
                  └─────────────────────────────────────┘
                                  │
                                  ▼
                            atlas-core
                       boundary invariants
```

## Property tests (Bolero)

Every property is a hand-written rule the boundary must obey. Bolero generates input across the type's `Arbitrary` space and asserts the rule holds; failures replay deterministically from the seed it logs.

The suite splits across two test files:

- [`tests/properties.rs`](tests/properties.rs) — **atlas-core invariants**. `RawAmount` sign + decimals algebra, `AssetInstance::validate_shape` partition, `Id` round-trips through `FromStr`/`Display`, CAIP-2/CAIP-19 parser/constructor agreement.
- [`tests/evm_codec_properties.rs`](tests/evm_codec_properties.rs) — **atlas-evm codec round-trips**. Generates `(chain_id, nonce, gas, fee, recipient, amount)` tuples, runs them through `EvmCodec::prepare_transfer`, and asserts every field decodes back through `alloy_consensus::TxEip1559::decode` / `TxLegacy::decode` lossless. Pins bit-equivalence with alloy's reference encoding.

```bash
cargo test -p atlas-verify
```

## Kani proofs

Kept narrow on purpose. `BigInt` and `BTreeMap` involve heap allocation and unbounded loops that Kani can't symbolically execute today. Now that the EVM codec is in tree, real proof leverage is reachable on RLP length prefixes, signature-byte parsing exhaustiveness, and the legacy / EIP-1559 encode→decode round-trip — see the verification-depth note in [ROADMAP.md](../../ROADMAP.md).

```bash
cargo kani -p atlas-verify
```

(Optional toolchain — not required for `cargo test`.)

## License

Licensed under the [MIT License](../../LICENSE).
