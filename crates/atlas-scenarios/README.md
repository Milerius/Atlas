# atlas-scenarios

[Cucumber](https://github.com/cucumber-rs/cucumber) BDD harness for atlas-core. Drives the documented end-to-end flows (registry load → asset resolution → transfer pipeline) with the in-tree `MockEvmChainService` from [`atlas-evm`](../atlas-evm).

`publish = false` — this is a test-only crate.

## Layout

```text
                        crates/atlas-scenarios
                  ┌─────────────────────────────────────┐
                  │                                     │
                  │   src/                              │
                  │     world.rs   ─── AtlasWorld       │
                  │                    (cucumber state) │
                  │     steps.rs   ─── Given / When /   │
                  │                    Then definitions │
                  │                                     │
                  │   features/atlas-core/              │
                  │     asset_resolution.feature        │
                  │     transfer.feature                │
                  │                                     │
                  │   tests/                            │
                  │     atlas_bdd.rs   ─── harness      │
                  │       (run_and_exit on failure)     │
                  │                                     │
                  └─────────────────────────────────────┘
                                  │
                                  ▼
                  ┌─────────────────────────────────────┐
                  │   atlas-core   atlas-evm            │
                  │   • Registry  • MockEvmChainService │
                  │   • MockSigner                      │
                  └─────────────────────────────────────┘
```

## Why BDD here

Two reasons the rest of the workspace doesn't need:

1. **Behaviour, not units.** The features describe what a wallet integration should expect end-to-end — "a transfer of 100 USDC on Base from a known account using a mock signer". Refactors that change implementation but preserve behaviour stay green.
2. **Sanity-check the docs.** The `.feature` files are themselves prose specs. If a step changes meaning, the feature file changes too — keeps the human-readable description in lockstep with the code.

## Running

```bash
# Full BDD run (exits non-zero on any step failure)
cargo test -p atlas-scenarios --test atlas_bdd

# Filter by tag (cucumber CLI flags pass through)
cargo test -p atlas-scenarios --test atlas_bdd -- --tags @smoke
```

The harness uses `World::run_and_exit` rather than `run` so a failing scenario surfaces as a non-zero exit code — bare `run` swallows step failures and would let regressions slip past CI as exit 0.

## What's covered

| Feature | Scenarios | Asserts |
|---|---|---|
| `asset_resolution.feature` | exact-instance lookup, group → instances expansion | `Registry` resolution surface |
| `transfer.feature` | mock transfer happy path, network-mismatch rejection | `ChainService::transfer` path |
| `signer.feature` | EVM chain exposes `default_derivation_path`, signer derived from registry path lands on a known address | registry → signer wiring |
| `typed_ids.feature` | `AssetInstanceId` CAIP-19 decomposition, `NetworkId` CAIP-2 rejection, EVM / Solana address-format validation | typed-id boundary contracts |

14 scenarios / 45 steps total.

## License

Licensed under the [MIT License](../../LICENSE).
