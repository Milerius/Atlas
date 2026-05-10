# Atlas Roadmap

This document captures the trajectory of the Atlas SDK: what's shipped, what's
planned, and what's deliberately deferred. It's a suggestion, not a contract —
priorities shift as the boundary stabilizes and real consumers appear.

For the current state in detail, see the project [`README.md`](README.md). For
the engineering rules every change must obey, see [`CODEX.md`](CODEX.md) /
[`CLAUDE.md`](CLAUDE.md).

---

## Where we are (2026-05-08)

| Area | State |
|---|---|
| `atlas-core` | Typed IDs, big-int amounts, error enums, chain/asset domain models, split registry with validation, provider-neutral signing trait, `ChainService` trait + `MockEvmService` |
| `atlas-verify` | Bolero properties (6), Kani proof scaffold (3 nightly proofs) |
| `atlas-scenarios` | Cucumber BDD scenarios (7 scenarios, 30 steps) |
| Official registry | Ethereum mainnet, Base mainnet, Solana mainnet · native ETH/SOL · Circle USDC across all 3 networks · embedded into atlas-core via `atlas_core::official` |
| Asset standards | `Native`, `Erc20`, `Spl` |
| Coverage | 99.88% line, 100% function (atlas-scenarios excluded) |
| Tests | 100 across the workspace |
| CI | fmt · clippy (Linux+macOS) · test (Linux+macOS) · BDD · doc (`-D warnings`) · WASM build · `cargo deny` · `cargo careful` · coverage |
| Nightly | mutation testing · Kani proofs · Bolero extended (100k iter) · full HTML coverage |

---

## Near-term (next 1–3 PRs)

### Real EVM `ChainService` + `LocalKeySigner` — designed, not yet implemented

Spec: [`docs/superpowers/specs/2026-05-08-atlas-chain-service-evm-design.md`](docs/superpowers/specs/2026-05-08-atlas-chain-service-evm-design.md)

- Refactor `atlas-core::service::ChainService` into 5 focused traits:
  `ChainCodec` / `ChainReader` / `FeeEstimator` / `ChainBroadcaster` /
  `ChainService` (orchestrator).
- New crate `atlas-evm` on top of [alloy](https://github.com/alloy-rs/alloy):
  RLP-encoded EVM transactions (legacy + EIP-1559), `eth_feeHistory`-driven
  fee suggestion, OP-Stack L1 fee oracle, full broadcast pipeline.
- New crate `atlas-signer-localkey`: secp256k1 in-process signer with three
  construction paths — raw key, JSON keystore, BIP-32 HD derivation from
  mnemonic.

**Why this first:** unlocks every downstream test, real assembly, and
hybrid (server-builds-tx, client-signs) deployments. `MockEvmService` becomes
a reference shape rather than the only working implementation.

### `AddressRef` validation

- EIP-55 checksum for EVM addresses.
- Base58 + length check for Solana pubkeys.
- Chain-aware validation at boundary call sites (e.g. `prepare_transfer`).

`AddressRef` is a typed string today; this PR makes it actually validate.
Probably fits in ~150 LOC + property tests.

---

## Mid-term (after the EVM service stabilizes)

### Real Solana `ChainService`

- Same 5-trait split as EVM, separate crate `atlas-solana`.
- Message encoding (`solana-sdk` or pure-Rust serializer).
- Recent-blockhash management with caching.
- SPL transfer instruction encoding.
- RPC client (Helius / public mainnet / `solana-client`).

The Solana entries are already in the official registry; this PR makes them
executable. Pressures the boundary on a non-EVM chain — will surface
hard-coded EVM assumptions (which we'd then fix in `atlas-core`).

### More `SignerProvider` adapters

Each as its own crate, all implementing the same trait so callers can swap:

- `atlas-signer-mpc-cramium` (or generic) — distributed signing with
  Guardian (cloud) or Silicon-style hardware (BLE/USB transport).
- `atlas-signer-privy` — Privy embedded-wallet authentication flow.
- `atlas-signer-aa-4337` — ERC-4337 account-abstraction relay (UserOperation
  signing + bundler submission).
- `atlas-signer-ledger` / `atlas-signer-trezor` — hardware wallet adapters
  via USB / WebUSB.
- `atlas-signer-walletconnect` — relay-mediated remote signing.

### CAIP-2 / CAIP-19 typed parsers

Promote `NetworkId` and `AssetInstanceId` from CAIP-shaped strings to
structured `(namespace, reference)` decomposition. Catches malformed ids at
the registry boundary, adds Bolero property tests for parser round-trips.

### Token approval and arbitrary contract-call intents

Beyond `TransferIntent`:

- `ApproveIntent` — ERC-20 / SPL approval flow.
- `ContractCallIntent` — generic contract / program invocation with
  ABI-encoded calldata.

Required for swap routing, on-chain governance interaction, and any
non-trivial DeFi flow.

### Dynamic token discovery

- Token-list adapter trait (pluggable source).
- Default implementations for Token Lists (Uniswap), CoinGecko, on-chain
  registries.
- Refresh policy + caching layer.

Currently the only assets in the registry are the curated ones in `registries/`.

---

## Longer-term

### More chain families

- **Sui** — Move-based, object-oriented ledger; new asset standard
  (`AssetStandard::Sui` for objects).
- **UTXO** — Bitcoin / Litecoin / etc.; new `ChainFamily::Utxo` execution
  semantics, UTXO selection in `ChainCodec::PrepareContext`.
- **Cosmos / IBC** — multi-zone routing; cross-chain transfer intents.
- **TON / Aptos / NEAR** — covered by WalletCore today; separate adapters
  if the demand appears.

### Product composition layer

Per [`docs/superpowers/specs/2026-05-08-atlas-product-accounts-composition-notes.md`](docs/superpowers/specs/2026-05-08-atlas-product-accounts-composition-notes.md):

- `Account` — composes signers + chain services + addresses across families.
- Unified portfolio queries — aggregate balances and positions across
  registered networks.
- Swap intent — abstract over DEX / aggregator routing.
- Lending, staking, payment cards — purposeful product modules layered on
  top of the boundary.

This layer is **deliberately deferred** until the boundary is stable and
multiple chain families are real. Building it on top of an unfinished
boundary risks freezing the wrong contracts.

### Verification depth

Once real chain services land, the surface for serious verification grows:

- **Kani proofs** for RLP length prefixes, gas u64 arithmetic, nonce
  comparison, fee invariants. Currently the 3 Kani proofs are scaffolding;
  real surface arrives with `atlas-evm`.
- **Differential tests** — Atlas's RLP encoder vs alloy's vs `tw_evm`'s on
  the same inputs.
- **Anvil-backed integration tests** in CI (gated, occasional).

### Cargo publish path

Currently atlas-core uses `include_str!("../../../registries/...")` which
reaches outside the crate root and would block `cargo publish`. When publishing
becomes a real goal, mirror the JSON via a `build.rs` into `OUT_DIR` or move
the registry files under `crates/atlas-core/registries/`. See the comment at
[`crates/atlas-core/src/official.rs`](crates/atlas-core/src/official.rs).

---

## Explicit non-goals (won't ship)

- A bundled HTTP client opinionated to one provider.
- Wallet-product UI / UX primitives. Atlas is the SDK foundation; product
  surfaces live downstream.
- Non-Rust language bindings. Possible later via `uniffi` if a real consumer
  needs it.
- Cross-language FFI codegen (the WalletCore route). Atlas stays pure-Rust.

---

## Suggestion box

Roadmap items that aren't yet specced — open for prioritization:

- Codec-only crate (split `atlas-evm-codec` out of `atlas-evm`) so server-side
  build-without-RPC consumers don't pull alloy-provider's HTTP deps.
- Property tests on RLP encoding via Bolero once `atlas-evm` lands.
- Token list snapshot under `registries/tokens/` for community-curated extras.
- BDD scenarios that exercise multi-network flows (e.g. "USDC on Base ↔
  USDC on Ethereum").

If you have a use case that's not represented here, file an issue or open a
discussion on the repo. Atlas's design works backwards from real consumer
needs.
