<h1 align="center">Atlas</h1>

<p align="center">
  A modular Rust blockchain SDK foundation.<br>
  <b>Typed amounts · Split registries · Provider-neutral signing · Concrete instances first</b>
</p>

<p align="center">
  <a href="https://github.com/Milerius/Atlas/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Milerius/Atlas/ci.yml?style=flat-square&logo=github&label=CI&branch=main" alt="CI"></a>
  <a href="https://github.com/Milerius/Atlas/actions/workflows/nightly.yml"><img src="https://img.shields.io/github/actions/workflow/status/Milerius/Atlas/nightly.yml?style=flat-square&logo=github&label=nightly" alt="Nightly"></a>
  <a href="https://codecov.io/gh/Milerius/Atlas"><img src="https://img.shields.io/codecov/c/github/Milerius/Atlas?style=flat-square&logo=codecov&label=coverage" alt="Coverage"></a>
  <img src="https://img.shields.io/badge/rust-stable-93450a?style=flat-square&logo=rust" alt="Rust stable">
  <img src="https://img.shields.io/badge/edition-2021-blue?style=flat-square&logo=rust" alt="Rust edition 2021">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License: MIT"></a>
</p>

<p align="center">
  <a href="https://github.com/Milerius/Atlas/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Milerius/Atlas/ci.yml?style=for-the-badge&label=tests&branch=main" alt="Tests"></a>
  <a href="https://codecov.io/gh/Milerius/Atlas"><img src="https://img.shields.io/codecov/c/github/Milerius/Atlas?style=for-the-badge&label=coverage" alt="Coverage"></a>
  <a href="https://github.com/Milerius/Atlas/actions/workflows/nightly.yml"><img src="https://img.shields.io/github/actions/workflow/status/Milerius/Atlas/nightly.yml?style=for-the-badge&label=mutants%20%2B%20coverage" alt="Nightly Verification"></a>
</p>

<p align="center">
  <b>Big-int raw amounts · No <code>f64</code> for money · Typed errors · Split chain/asset registries · 99.7% line coverage</b>
</p>

---

## Why Atlas?

Most "wallet SDKs" couple a registry, a UI style, a chain client, a custody model, and a token list into one library and ship it as the product. Switching any one of those means forking the SDK.

Atlas is the foundation underneath that. The boundary types — chains, networks, assets, instances, signing requests — are explicit and typed. Adapters are pluggable. Execution always resolves to a concrete `AssetInstance` before signing.

|                              | Atlas                                            | Typical wallet SDK                            |
|------------------------------|--------------------------------------------------|-----------------------------------------------|
| **Token amounts**            | Arbitrary-precision `BigInt` + decimal scale     | `f64` / `Number` (precision loss on > 2^53)   |
| **Asset model**              | `AssetGroup` (display) ≠ `AssetInstance` (exec)  | One `Token` type covering both                |
| **Chain data**               | `chain_registry` (slow-moving) split from        | One mega-registry mixing chains, RPCs, tokens |
|                              | `asset_registry` (fast-moving)                   |                                               |
| **Signing**                  | Provider-neutral trait (raw sig / signed tx /    | One custody model baked into the SDK          |
|                              | submitted tx)                                    |                                               |
| **Errors**                   | Typed enums per domain, no panics in SDK paths   | `Box<dyn Error>` / `anyhow` everywhere        |
| **Validation**               | Registries rejected before exposure              | Trust fixtures, fail at runtime               |
| **Modularity**               | Core ships primitives, adapters live separately  | "All-in-one" SDK forces vendor lock-in        |
| **Verification rigor**       | 99.73% line coverage · `cargo deny` · `careful`  | Happy-path unit tests                         |

---

## Architecture

The first scope is a strict blockchain core. Higher product layers (accounts, cards, lending, staking, swaps, unified portfolio) are deliberately deferred — but the boundary types are designed to support them.

```text
                       ┌─────────────────────────────────────┐
                       │         Application / Product        │
                       │   (accounts, cards, swaps, …) — TBD  │
                       └─────────────────┬───────────────────┘
                                         │ uses concrete AssetInstance
                                         │
       ┌─────────────────────────────────┼─────────────────────────────────┐
       │                                 │                                 │
┌──────▼──────┐                ┌─────────▼─────────┐               ┌──────▼──────┐
│  Registry   │                │   ChainService    │               │   Signer    │
│             │                │    (per chain     │               │  Provider   │
│  ┌────────┐ │                │     family)       │               │             │
│  │ chains │ │                │                   │               │  raw sig /  │
│  └────────┘ │   resolve to   │  prepare_transfer │   sign        │  signed tx /│
│  ┌────────┐ │ ──────────────►│  signing_request  │ ◄──────────►  │  submitted  │
│  │assets  │ │                │  assemble_signed  │               │             │
│  └────────┘ │                │  broadcast        │               │  MockSigner │
│             │                │                   │               │  MPC / Privy│
│  validates  │                │  MockEvmService   │               │  4337 / …   │
│  references │                │  (real EVM TBD)   │               │  (TBD)      │
└─────────────┘                └───────────────────┘               └─────────────┘
        ▲                                                                   ▲
        │                                                                   │
        │                  TransferIntent { instance, to, amount }          │
        └───────────────────────────────────────────────────────────────────┘
```

**A few important rules:**

- **Asset groups never sign.** `AssetGroup` and `AssetInstrument` are for display, search, pricing, routing. `AssetInstance` is the only thing that signs or broadcasts.
- **Chain services operate on concrete instances.** Higher layers may accept a group or instrument, but resolve before execution.
- **Signing is provider-neutral.** A signer may return a raw signature, a signed transaction, or a submitted transaction result depending on capabilities.
- **Registries connect through stable IDs.** `Network.chain → Chain.id`, `AssetInstance.network → Network.id`, `AssetInstance.instrument_id → AssetInstrument.id`, `AssetInstrument.group_id → AssetGroup.id`.
- **Validation happens before exposure.** Unknown versions and dangling references are rejected at `Registry::from_documents`.

---

## Highlights

🎯 **Concrete instances first** — execution always resolves to an `AssetInstance`. No accidental signing on a display-only group.

🔢 **Big-int raw amounts** — `RawAmount` wraps `num_bigint::BigInt` + decimal scale. No `f64`, no precision loss, no silent rounding. Negative values rejected at construction.

🧩 **Split registries** — `chain_registry` (chains, networks, RPC defaults, native asset references) split from `asset_registry` (groups, instruments, instances, contracts, decimals). Atlas ships an [official registry](registries/) covering Ethereum, Base, and Solana mainnet with native ETH/SOL plus Circle USDC across all three networks.

🔐 **Provider-neutral signing** — one `SignerProvider` trait covers MPC, local keys, Privy, account abstraction. Signers may return a raw signature, a signed transaction, or a submitted transaction result.

🚦 **Typed errors everywhere** — `RegistryError`, `AssetError`, `ChainError`, `SigningError`, `RpcError`, `AmountError` with named variants. No `Box<dyn Error>` in SDK paths.

🧪 **Verification rigor** — unit + fixture + smoke + BDD scenarios, 99.73% line coverage, Bolero property tests, Kani proof scaffold, `cargo deny`, `cargo careful`, mutation testing nightly, `cargo doc -D warnings`, `wasm32-unknown-unknown` build check.

🚫 **No unsafe** — `#![forbid(unsafe_code)]` at the crate root.

---

## Crates

| Crate | Purpose | Tests |
|---|---|---:|
| [`atlas-core`](crates/atlas-core/) | Typed IDs, big-int amounts, chain/asset domain models, split registries with validation, provider-neutral signing trait, `ChainService` trait + `MockEvmService`, embedded official registry constants | 49 |
| [`atlas-verify`](crates/atlas-verify/) | Bolero property tests + Kani proofs targeting atlas-core boundary invariants | 6 |
| [`atlas-scenarios`](crates/atlas-scenarios/) | Cucumber BDD scenarios — product-level flows for asset resolution and end-to-end mock transfer | 7 |

---

## Quick Start

```bash
# Build
cargo build --workspace

# Run the test suite
cargo test --workspace --all-features

# Lint
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Coverage report (HTML)
cargo +nightly llvm-cov --workspace --all-features --html

# Verify dependency hygiene
cargo deny check
```

A minimal end-to-end flow — load registries, resolve a concrete instance, sign with a mock signer, broadcast through the mock chain service:

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

let chain_doc: ChainRegistryDocument = serde_json::from_str(include_str!("chain_registry.json"))?;
let asset_doc: AssetRegistryDocument = serde_json::from_str(include_str!("asset_registry.json"))?;
let registry = Registry::from_documents(chain_doc, asset_doc)?;

// Resolve a concrete asset instance — never sign a group.
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
```

---

<details>
<summary><h2>🧪 Verification Tiers</h2></summary>

| Tier | Tool                                | Cadence       | What it catches                                                                |
|------|-------------------------------------|---------------|--------------------------------------------------------------------------------|
| 1    | Unit tests (49 in atlas-core)       | Every PR      | Per-module behavior, registry error paths, signing/service boundaries          |
| 2    | Fixture tests                       | Every PR      | Valid + invalid registry JSON, missing references, native asset rules          |
| 3    | Smoke test (`smoke_flow`)           | Every PR      | End-to-end: registry load → instance lookup → mock sign → broadcast            |
| 4    | Bolero property tests (6 in verify) | Every PR      | Generated input across `Id`, `RawAmount`, `validate_shape`; replayable on fail |
| 5    | BDD scenarios (Cucumber, 7)         | Every PR      | Product-level flows: group resolution, end-to-end transfer, network mismatch   |
| 6    | `cargo fmt --check`                 | Every PR      | Formatting drift                                                               |
| 7    | `cargo clippy -D warnings`          | Every PR      | Lint regressions on Linux + macOS                                              |
| 8    | `cargo doc -D warnings`             | Every PR      | Broken doc links and rustdoc warnings                                          |
| 9    | `wasm32-unknown-unknown` build      | Every PR      | atlas-core stays WASM-compilable                                               |
| 10   | `cargo deny`                        | Every PR      | License + advisory + dependency hygiene                                        |
| 11   | `cargo careful`                     | Every PR      | Extra UB detection beyond standard tests                                       |
| 12   | Coverage (`cargo llvm-cov`)         | Every PR      | Line coverage tracked via Codecov (currently 99.73%)                           |
| 13   | Mutation testing (`cargo-mutants`)  | Nightly       | Test-suite quality regression                                                  |
| 14   | Full HTML coverage                  | Nightly       | Detailed line-level coverage artifact                                          |
| 15   | Kani proofs (3 scaffold proofs)     | Nightly       | Bounded model checking for registry version invariants                         |
| 16   | Bolero extended (100k iterations)   | Nightly       | Property tests with deeper input space than the PR-time run                    |

Every PR runs: fmt → clippy (Linux + macOS) → test (Linux + macOS) → bolero properties → bdd → doc → wasm → deny → careful → coverage.

</details>

<details>
<summary><h2>🏛️ Design Principles</h2></summary>

1. **Modular and composable** — the SDK provides primitives, registries, adapters, and clean extension points. It must not impose a wallet-product style on consumers.
2. **Explicit domain models** — typed IDs, typed amounts, typed errors. No loosely-typed strings or ad-hoc maps for things that have a shape.
3. **Concrete execution** — UX and portfolio layers may aggregate, but low-level chain execution must resolve to concrete instances first.
4. **No `f64` for money** — arbitrary-precision integers for raw base units (wei, satoshi, token units, gas, nonces); decimal types for prices, FX rates, percentages.
5. **No panics in SDK paths** — typed errors at trait and public API boundaries so consumers can `match` on failure.
6. **Validate external input** — registries, IDs, addresses, contracts, RPC responses, signer responses, generated bindings. Reject unknown registry versions; reject invalid references before exposing a registry to services.
7. **Adapter-specific details out of canonical registries** — Wallet Core IDs, RPC vendor quirks, provider-specific payloads, codegen details belong in adapters, not in the registry schema.
8. **First-scope bias** — start with EVM, native coins, fungible tokens. Keep enums small at first, but design them to grow.

See [CODEX.md](CODEX.md) and [CLAUDE.md](CLAUDE.md) for the full engineering rules. The same rules apply across both files — they exist as parallel sources for whichever agent (or human) is reading.

</details>

<details>
<summary><h2>🎯 Who this is for</h2></summary>

Atlas is useful for teams building:

- crypto wallet products that need to support multiple chains and signers
- treasury / custody platforms with mixed signing models (MPC + local + AA)
- portfolio aggregators that want one canonical asset model
- DeFi backends that need typed amounts and concrete chain execution
- any system where loosely-typed token amounts or stringly-typed asset IDs would cause real money loss

If your blockchain product would otherwise rebuild the same registry + amount + signing boundary in every service, Atlas is designed for that problem.

</details>

<details>
<summary><h2>🚫 What Atlas is <i>not</i> (yet)</h2></summary>

Atlas is the **boundary layer**, not a full wallet product. The first scope is intentionally narrow.

Atlas does **not** currently provide:

- **Real EVM execution** — `MockEvmService` is a boundary stub. RLP encoding, gas estimation, EIP-1559 dynamic fees, and a real RPC client are deferred.
- **Real signers** — only `MockSigner` exists. Local-key, MPC, Privy, and ERC-4337 adapters are deferred.
- **Address validation** — `AddressRef` is currently a typed string. EIP-55 / chain-aware format validation is deferred.
- **Non-EVM execution** — Solana data is in the [official registry](registries/) (chain entry, mainnet network, native SOL, Circle USDC SPL), but there's no real Solana `ChainService` yet. Sui and UTXO families are designed for but not in the registry yet.
- **Token approvals / contract calls** — only transfer is modeled.
- **Discovered tokens** — token list is static fixtures; dynamic discovery is deferred.
- **Product layer** — accounts, cards, stocks, lending, staking, swaps, unified portfolio APIs are deliberately out of scope until the boundary stabilizes.

The model is designed so each of these can land as an additive adapter or sibling crate without reshaping the core.

</details>

<details>
<summary><h2>📋 Project Status</h2></summary>

Early development. The full design specification lives in [`docs/superpowers/specs/2026-05-08-atlas-core-blockchain-layer-design.md`](docs/superpowers/specs/2026-05-08-atlas-core-blockchain-layer-design.md). Implementation is tracked in [`docs/superpowers/plans/2026-05-08-atlas-core-blockchain-layer.md`](docs/superpowers/plans/2026-05-08-atlas-core-blockchain-layer.md).

**Done:**
- `atlas-core` crate: typed IDs, `RawAmount` (BigInt), error enums, chain/network models, asset group/instrument/instance models with shape validation
- Split chain + asset registry documents with cross-reference validation, version checking, duplicate-ID detection
- Provider-neutral signing boundary (`SignerProvider` trait, `SigningRequest`, `SigningResponse` with raw-sig / signed-tx / submitted variants)
- `ChainService` trait + `MockEvmService` smoke implementation
- `AssetStandard::Spl` for SPL tokens; `Spl` shape validation requires a non-empty mint
- Official registry shipped in-tree under [`registries/`](registries/) — Ethereum, Base, Solana mainnet with native ETH/SOL and Circle USDC across all three; embedded into `atlas-core` via `include_str!` in `atlas_core::official`
- BDD scenarios (Cucumber, 7 in `atlas-scenarios`); Bolero properties (6) + Kani proof scaffold (3) in `atlas-verify`
- Full CI (fmt/clippy/test/bdd/doc/wasm/deny/careful/coverage); nightly mutants + Kani + Bolero extended

**Next:**
- Real EVM `ChainService`: RLP encoding, EIP-1559 fee model, gas estimation, RPC client trait
- Real Solana `ChainService` to match the new Solana registry entries
- Real `SignerProvider` impls: secp256k1 local key, MPC adapter, Privy adapter, ERC-4337 account abstraction; ed25519 local key for Solana
- `AddressRef` validation — EIP-55 checksum, chain-aware format (EVM hex vs base58 Solana pubkey)
- CAIP-2 / CAIP-19 typed parsers for `NetworkId` and `AssetInstanceId`
- Token approval and generic contract-call intent
- Dynamic token discovery (token-list adapter trait)
- Product composition layer: accounts, swaps, unified portfolio (per [`docs/superpowers/specs/2026-05-08-atlas-product-accounts-composition-notes.md`](docs/superpowers/specs/2026-05-08-atlas-product-accounts-composition-notes.md))

</details>

---

## License

Licensed under the [MIT License](LICENSE).
