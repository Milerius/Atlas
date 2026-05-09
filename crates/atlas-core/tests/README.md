# atlas-core integration tests

Integration tests for [`atlas-core`](..). The unit tests for individual types (typed-ID error paths, `RawAmount` arithmetic, `MockSigner` round-trips, `EvmFee` serde) live alongside the source in `src/*.rs` under `#[cfg(test)]`. This directory is everything that needs more than one type to make sense — registry validation, asset-graph traversal, official-registry sanity.

## Layout

```text
              tests/
              │
              ├── asset_resolution.rs       ── group → instance expansion
              │                                exact-instance lookup
              │
              ├── registry_validation.rs    ── Registry::from_documents
              │                                fixture-driven failure modes
              │
              ├── official_registry.rs      ── atlas_core::official::*
              │                                end-to-end loads
              │
              ├── common/mod.rs             ── shared helper:
              │                                fn registry() -> Registry
              │
              └── fixtures/
                  ├── chain_registry.valid.json
                  ├── asset_registry.valid.json
                  ├── asset_registry.invalid_missing_network.json
                  └── asset_registry.invalid_native_contract.json
```

## What each file covers

| File | Asserts |
|---|---|
| `registry_validation.rs` | A valid pair of documents constructs a `Registry`. Invalid pairs surface the typed `RegistryError` variant the spec promises (`MissingNetwork`, `MissingAssetGroup`, `InvalidReference` for shape failures, etc). |
| `asset_resolution.rs` | `Registry::asset_instances_for_group` returns every instance whose instrument belongs to the group; exact `asset_instance(...)` lookup matches by full CAIP-19 ID; mismatched IDs surface `MissingAssetInstance`. |
| `official_registry.rs` | The bundled `CHAIN_REGISTRY_JSON` + `ASSET_REGISTRY_JSON` deserialize and validate successfully. Sanity-checks the curated set: 2 chains, 3 networks, 3 groups, 6 instances. |

## Test layering

```text
   ┌─────────────────────────────────────────────────────┐
   │  Unit tests in src/*.rs                             │
   │    typed-ID errors, RawAmount, MockSigner, EvmFee   │
   │    serde, ChainCodec/ChainReader/... mock impls     │
   └─────────────────────────────────────────────────────┘
                       ▲
                       │ depends on
                       │
   ┌─────────────────────────────────────────────────────┐
   │  Integration tests in tests/                        │
   │    multi-type flows: registry, asset graph, smoke   │
   │    fixtures kept under tests/fixtures/              │
   └─────────────────────────────────────────────────────┘
                       ▲
                       │ also exercised by
                       │
   ┌─────────────────────────────────────────────────────┐
   │  Property tests in atlas-verify (Bolero)            │
   │  BDD scenarios in atlas-scenarios (Cucumber)        │
   └─────────────────────────────────────────────────────┘
```

The smoke-flow test that previously lived here (`smoke_flow.rs`) moved to [`atlas-evm/tests/smoke_flow.rs`](../../atlas-evm/tests/smoke_flow.rs) when `MockEvmChainService` migrated to atlas-evm.

## Running

```bash
# Whole crate
cargo test -p atlas-core

# Just the integration suite
cargo test -p atlas-core --tests

# One file
cargo test -p atlas-core --test registry_validation
```
