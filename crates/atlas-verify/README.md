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

Examples of rules covered:

- `RawAmount::new(value, decimals)` succeeds iff `value >= 0`; the constructed amount round-trips its decimals.
- `RawAmount::checked_add` rejects mismatched decimals and is commutative + associative on matching inputs.
- `AssetInstance::validate_shape` accepts native+`None` contract and rejects native+`Some(_)`; accepts erc20+`Some(_)` and rejects erc20+`None`.
- ID newtypes round-trip through `FromStr`/`Display`.

```bash
cargo test -p atlas-verify
```

## Kani proofs

Kept narrow on purpose. `BigInt` and `BTreeMap` involve heap allocation and unbounded loops that Kani can't symbolically execute today. The current proof set is a placeholder — real proof leverage arrives with the EVM crate (RLP length prefixes, gas `u64` arithmetic, nonce comparison).

```bash
cargo kani -p atlas-verify
```

(Optional toolchain — not required for `cargo test`.)

## License

Licensed under the [MIT License](../../LICENSE).
