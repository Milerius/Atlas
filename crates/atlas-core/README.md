# atlas-core

Boundary types, registry validation, and the per-chain trait surface for the Atlas blockchain SDK.

This crate is the cross-chain foundation: typed IDs, big-int raw amounts, split chain and asset registries, a provider-neutral signing trait, and the five-trait `ChainService` surface every per-chain crate plugs into. It has no HTTP client, no key material, no RLP encoder — those belong in the adapter crates ([`atlas-evm`](../atlas-evm), [`atlas-signer-localkey`](../atlas-signer-localkey), …).

`#![forbid(unsafe_code)]`.

## Architecture

```text
                ┌────────────────────────────────────────────┐
                │  Registry::from_documents(chain, asset)    │
                │                                            │
                │  • reject unknown versions                 │
                │  • reject duplicate IDs (per collection)   │
                │  • validate cross-references               │
                │  • validate AssetInstance shape            │
                │  • validate Network.native_asset_instance  │
                └─────────────────────┬──────────────────────┘
                                      │ Registry
                                      │
                ┌─────────────────────┼──────────────────────┐
                │                     │                      │
   resolve     ▼                     ▼                      ▼
  ┌──────────────┐         ┌────────────────┐       ┌──────────────┐
  │ AssetGroup   │  group  │ AssetInstrument│  inst │ AssetInstance│
  │ (display)    │────────►│   (display)    │──────►│  (execute)   │
  └──────────────┘         └────────────────┘       └──────┬───────┘
                                                           │
                                                           ▼
                                                  ┌────────────────┐
                                                  │ TransferIntent │
                                                  │ + AccountRef   │
                                                  └───────┬────────┘
                                                          │
                                                          ▼
        ┌─────────────────────── ChainService (5 traits) ─────────────────────┐
        │                                                                     │
        │   ChainCodec      ChainReader    FeeEstimator   ChainBroadcaster    │
        │   (pure)          (RPC)          (RPC)          (RPC)               │
        │       │              │              │              │                │
        │       └──────────────┴───── ChainService ──────────┘                │
        │                              │                                      │
        │                              ▼                                      │
        │                    SigningRequest ───► SignerProvider ─► Response   │
        └─────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
                              BroadcastResult
```

`AssetGroup` and `AssetInstrument` exist for display, search, pricing, routing, and aggregation. `AssetInstance` is the only thing that ever signs or broadcasts. The codec refuses anything but a concrete instance whose ID belongs to the target network — `eip155:1` does not match `eip155:10/...`.

## The five `ChainService` traits

The transfer lifecycle is split into focused seams so a server can build offline, a client can sign with hardware/MPC, and any of the four sub-traits can be swapped in isolation.

| Trait | Surface | I/O |
|---|---|---|
| `ChainCodec` | `prepare_transfer`, `signing_request`, `assemble_signed` | none — pure |
| `ChainReader` | `get_balance`, `get_nonce`, `get_transaction_status` | RPC reads |
| `FeeEstimator` | `estimate_fee` (returns `Self::Fee`) | RPC reads |
| `ChainBroadcaster` | `broadcast` | RPC write |
| `ChainService` | `transfer` (orchestrator) | composes all four |

Concrete implementations (real `EvmChainService`, in-tree `MockEvmChainService`) live in per-chain crates. `atlas-core` defines the surface and stops there.

## Provider-neutral signing

`SignerProvider::sign(SigningRequest) -> SigningResponse` covers MPC, local keys, Privy, hardware wallets, ERC-4337 — Atlas does not force one custody model:

```rust
pub enum SigningResponse {
    SignatureOnly      { signer, signature, public_key },  // raw sig — codec assembles
    SignedTransaction  { signer, raw },                    // pre-encoded raw tx
    SubmittedTransaction { signer, tx_hash },              // signer broadcast itself
}
```

`UnsignedBundle { unsigned, signing_request }` is the wire format for the server-builds / client-signs deployment topology. See [`atlas-evm`](../atlas-evm) for an end-to-end demonstration.

## Key types

### Identifiers (all reject empty/whitespace at construction)

| Type | Purpose |
|---|---|
| `Id` | Internal non-empty string newtype |
| `ChainId` | Chain family ID (`evm`, `solana`, …) |
| `NetworkId` | Concrete network — CAIP-2 form (`eip155:1`) |
| `AssetGroupId` | Display-level group (`usdc`, `eth`) |
| `AssetInstrumentId` | Issuer-level instrument (`usdc.circle`) |
| `AssetInstanceId` | On-chain instance (`eip155:8453/native:eth`) |
| `SignerId` / `AccountRef` / `AddressRef` | Signer / account / address handles |

### Money

| Type | Purpose |
|---|---|
| `RawAmount` | `BigInt` value + decimal scale. Constructor rejects negatives. |
| `AmountError` | `DecimalsMismatch { left, right }` / `NegativeValue` |

`f64` is forbidden for money — use `BigInt` for raw base units, `Decimal` for prices and rates.

### Domain models

| Type | Purpose |
|---|---|
| `Chain` | Chain family — `id`, `family`, `address_format`, `default_curve`, `supported_standards`, `capabilities`, `default_derivation_path` |
| `Network` | Concrete deployed env — `id`, `chain`, `environment`, `native_asset_instance_id`, `rpc`, `explorers`, `features` |
| `AssetGroup` | Display-level grouping |
| `AssetInstrument` | Issuer-level token shape |
| `AssetInstance` | Concrete on-chain instance — only thing that signs / broadcasts |

### Registries

`Registry::from_documents(chain_doc, asset_doc) -> Result<Registry, RegistryError>` rejects:
- unknown versions (`UnsupportedVersion`)
- duplicate IDs in any of the five collections
- networks referencing a missing chain
- instruments referencing a missing group
- instances referencing a missing network or instrument
- network native instances that don't exist or belong elsewhere
- instances whose `(standard, contract)` shape fails `validate_shape`

Lookup accessors return typed errors: `network`, `chain`, `asset_group`, `asset_instrument`, `asset_instance`, `asset_instances_for_group`.

### Transactions

| Type | Purpose |
|---|---|
| `TransferIntent` | What the caller wants — concrete `AssetInstanceId`, `AddressRef`, `RawAmount` |
| `UnsignedTransaction` | Codec output — `account`, `network`, `intent`, opaque `payload` bytes |
| `UnsignedBundle` | Wire format: `UnsignedTransaction` + pre-computed `SigningRequest` |
| `SignedTransaction` | Broadcast-ready bytes |
| `BroadcastResult` | `tx_hash` returned by the network |

### Errors

Six typed enums, each `Clone + Debug + Eq + PartialEq + thiserror::Error`:

| Enum | Where it surfaces |
|---|---|
| `RegistryError` | Registry construction and lookup |
| `AssetError` | `AssetInstance::validate_shape` |
| `ChainError` | Chain-service lifecycle (prepare / sign / broadcast) — `#[from]` for `RpcError` and `SigningError` |
| `SigningError` | Signer providers |
| `RpcError` | Adapter RPC layer |
| `AmountError` | `RawAmount` construction and arithmetic |

## Embedded official registry

`atlas_core::official::{CHAIN_REGISTRY_JSON, ASSET_REGISTRY_JSON}` ships Atlas's curated chain + asset set as `include_str!`-bundled JSON. Three networks (Ethereum, Base, Solana mainnet), three asset groups (`eth`, `usdc`, `sol`), six instances. Consumers can ignore it and bring their own JSON.

## Design invariants

- **`AssetInstance` is the only executable shape.** Calling a chain service with anything else is a type error, not a runtime error.
- **Network prefix is path-segment exact.** The `/` separator must immediately follow the network ID.
- **Registries reject unknown versions.** Forward-compat is opt-in.
- **Duplicate IDs are an error**, not last-write-wins.
- **`f64` is forbidden for money.**
- **Signers may return any of three shapes.** Atlas-core does not force one custody model.
- **No panics in SDK paths.** Typed error enums at every boundary.

## Testing

```bash
cargo test -p atlas-core
```

See [`tests/README.md`](tests/README.md) for the test layout.

## License

Licensed under the [MIT License](../../LICENSE).
