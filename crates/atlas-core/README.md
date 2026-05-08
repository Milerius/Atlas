# atlas-core

Boundary types and registry validation for the Atlas blockchain SDK.

Provides typed IDs, big-int raw amounts, split chain and asset registries, a provider-neutral signing trait, and a `ChainService` trait with a mock EVM implementation. No HTTP client, no real signing — those live in adapter crates that plug into the traits defined here.

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
                                                  │ + NetworkId    │
                                                  └───────┬────────┘
                                                          │
                                                          ▼
                                              ┌──────────────────────┐
                                              │     ChainService     │
                                              │                      │
                                              │  prepare_transfer    │
                                              │  signing_request     │── SigningRequest ─┐
                                              │  assemble_signed     │◄── SigningResponse┘
                                              │  broadcast           │
                                              └──────────┬───────────┘
                                                         │
                                                         ▼
                                                BroadcastResult
```

`AssetGroup` and `AssetInstrument` exist for display, search, pricing, routing, and aggregation. `AssetInstance` is the only thing that ever signs or broadcasts. The `ChainService` boundary refuses anything but a concrete instance whose ID belongs to the target network.

## Core Traits

Two extension points, both `Send + Sync`:

| Trait | Purpose | Implementations |
|---|---|---|
| `ChainService` | Per-chain-family transaction lifecycle: prepare, signing-request, assemble, broadcast | `MockEvmService` (in-tree); real EVM / Solana / UTXO services TBD |
| `SignerProvider` | Provider-neutral signing | `MockSigner` (in-tree); local-key / MPC / Privy / ERC-4337 adapters TBD |

Signers may return any of three shapes — atlas-core does not force one custody model:

```rust
pub enum SigningResponse {
    SignatureOnly      { signer, signature, public_key },  // raw sig — service assembles
    SignedTransaction  { signer, raw },                    // pre-encoded raw tx
    SubmittedTransaction { signer, tx_hash },              // signer broadcast itself
}
```

## Key Types

### Identifiers

| Type | Purpose |
|---|---|
| `Id` | Internal non-empty string newtype — `Display`, `FromStr`, `Serialize`, `Deserialize` |
| `ChainId` | Chain family ID (`evm`, `solana`, …) |
| `NetworkId` | Concrete network — CAIP-2 form (`eip155:1`, `eip155:8453`) |
| `AssetGroupId` | Display-level group (`usdc`, `eth`) |
| `AssetInstrumentId` | Issuer-level instrument (`usdc.circle`, `eth.native`) |
| `AssetInstanceId` | Concrete on-chain instance — CAIP-19-ish (`eip155:8453/native:eth`) |
| `SignerId` | Signer provider identifier |
| `AccountRef` | Account identifier handed to chain services |
| `AddressRef` | Recipient address (validation deferred — currently typed string) |

All typed IDs reject empty / whitespace-only input at construction (`IdError::Empty`).

### Amounts

| Type | Purpose |
|---|---|
| `RawAmount` | Big-integer base-unit amount (`BigInt` + decimal scale). Fallible constructor rejects negatives. |
| `AmountError` | `DecimalsMismatch { left, right }` and `NegativeValue` |

`RawAmount::checked_add` requires matching decimals and bypasses the sign check on the internal sum (non-negative + non-negative is non-negative).

### Domain Models

| Type | Purpose |
|---|---|
| `Chain` | Chain family record — `id`, `family`, `address_format`, `default_curve`, `supported_standards`, `capabilities` |
| `Network` | Concrete deployed network — `id`, `chain`, `environment`, `native_asset_instance_id`, `rpc`, `explorers`, `features` |
| `AssetGroup` | Display-level grouping — `id`, `symbol`, `name`, `metadata` |
| `AssetInstrument` | Issuer-level token shape — `id`, `group_id`, `asset_class`, `kind`, `decimals`, `traits`, `issuer` |
| `AssetInstance` | Concrete on-chain instance — `id`, `instrument_id`, `network`, `standard`, `decimals`, `contract`, `capabilities` |

`AssetInstance::validate_shape()` enforces:
- `Native` standard must not have a contract
- `Erc20` standard must have a non-empty contract

### Registries

| Type | Purpose |
|---|---|
| `ChainRegistryDocument` | Versioned input: `version`, `chains`, `networks` |
| `AssetRegistryDocument` | Versioned input: `version`, `asset_groups`, `asset_instruments`, `asset_instances` |
| `Registry` | Validated, in-memory registry with typed lookups and group resolution |
| `LATEST_REGISTRY_VERSION` | Currently `1` |

`Registry::from_documents` rejects:
- unknown versions (`UnsupportedVersion { version }`)
- duplicate IDs in any of the five collections (`InvalidReference { message: "duplicate <kind> id: …" }`)
- networks that reference a missing chain
- instruments that reference a missing group
- instances that reference a missing network or instrument
- network native instances that don't exist or belong to another network
- instances whose shape fails `validate_shape`

### Transactions

| Type | Purpose |
|---|---|
| `TransferIntent` | What the caller wants — concrete `AssetInstanceId`, `AddressRef`, `RawAmount` |
| `UnsignedTransaction` | What the chain service produces — `account`, `network`, `intent`, opaque `payload` bytes |
| `SignedTransaction` | Signed bytes ready to broadcast — `network`, `raw` |
| `BroadcastResult` | `tx_hash` returned from broadcast |

### Signing

| Type | Purpose |
|---|---|
| `SignerRef` | Reference to a configured signer |
| `SigningRequest` | What chain services hand to a signer — `account`, `network`, `curve`, `payload_kind`, `payload` |
| `SigningPayloadKind` | `transaction_digest` / `unsigned_transaction` / `message` / `typed_data` |
| `SigningResponse` | `signature_only` / `signed_transaction` / `submitted_transaction` |

### Errors

Six typed error enums, each `Clone + Debug + Eq + PartialEq + thiserror::Error`:

| Enum | Where it surfaces |
|---|---|
| `RegistryError` | Registry construction and lookup |
| `AssetError` | Asset shape validation |
| `ChainError` | Chain-service lifecycle (prepare / sign / broadcast) |
| `SigningError` | Signer providers |
| `RpcError` | RPC adapters (used by future real chain services) |
| `AmountError` | Raw amount construction and arithmetic |

## Usage

```rust
use atlas_core::{
    amount::RawAmount,
    id::{AccountRef, AddressRef, SignerId},
    registry::{AssetRegistryDocument, ChainRegistryDocument, Registry},
    service::{ChainService, MockEvmService},
    signing::{MockSigner, SignerProvider},
    transaction::TransferIntent,
};
use num_bigint::BigInt;
use std::str::FromStr;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let chain_doc: ChainRegistryDocument =
    serde_json::from_str(include_str!("../tests/fixtures/chain_registry.valid.json"))?;
let asset_doc: AssetRegistryDocument =
    serde_json::from_str(include_str!("../tests/fixtures/asset_registry.valid.json"))?;
let registry = Registry::from_documents(chain_doc, asset_doc)?;

// Always resolve to a concrete instance before signing.
let asset = registry.asset_instance("eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")?;
let network = registry.network(asset.network.as_str())?;

let service = MockEvmService;
let signer  = MockSigner::new(SignerId::from_str("mock-signer")?);

let intent = TransferIntent {
    asset_instance_id: asset.id.clone(),
    to:                AddressRef::from_str("0x0000000000000000000000000000000000000001")?,
    amount:            RawAmount::new(BigInt::from(100_000_000u64), asset.decimals)?,
};

let unsigned = service.prepare_transfer(
    AccountRef::from_str("account-1")?, network.id.clone(), intent,
).await?;
let request  = service.signing_request(&unsigned)?;
let response = signer.sign(request).await?;
let signed   = service.assemble_signed_transaction(unsigned, response)?;
let result   = service.broadcast(signed).await?;
# Ok(())
# }
```

## Design Invariants

- **`AssetInstance` is the only executable shape.** Calling a chain service with anything else is a type error, not a runtime error.
- **Network prefix is path-segment exact.** `prepare_transfer` rejects an asset instance whose CAIP path begins with the network ID as a plain string prefix; the `/` separator must immediately follow the network ID. (`eip155:1` does not match `eip155:10/...`.)
- **Registries reject unknown versions.** Forward-compat is opt-in via explicit version handling, not silent.
- **Duplicate IDs are an error, not a last-write-wins.** Every collection in both registry documents is validated for uniqueness during construction.
- **`f64` is forbidden for money.** Use `BigInt` for raw base units, `Decimal` for prices and rates.
- **Signers may return any of three shapes.** Atlas-core does not force one custody model into a single response variant.
- **No panics in SDK paths.** Typed error enums at every boundary.

## Testing

47 tests covering:

- Unit: every error path through `Registry::from_documents`, `AssetInstance::validate_shape`, `RawAmount`, typed IDs, `MockEvmService`, `MockSigner`, `SigningResponse` serde round-trip
- Fixtures: valid, invalid-missing-network, invalid-native-with-contract registries (`tests/registry_validation.rs`)
- Resolution: `asset_instances_for_group` and exact-instance lookup (`tests/asset_resolution.rs`)
- Smoke: end-to-end registry → instance → mock sign → broadcast (`tests/smoke_flow.rs`)

```bash
cargo test -p atlas-core --all-features
```

## License

Licensed under the [MIT License](../../LICENSE).
