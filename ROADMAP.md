# Atlas Roadmap

This document captures the trajectory of the Atlas SDK: what's shipped, what's
planned, and what's deliberately deferred. It's a suggestion, not a contract —
priorities shift as the boundary stabilizes and real consumers appear.

For the current state in detail, see the project [`README.md`](README.md). For
the engineering rules every change must obey, see [`CODEX.md`](CODEX.md) /
[`CLAUDE.md`](CLAUDE.md).

---

## Where we are (2026-05-10)

| Area | State |
|---|---|
| `atlas-core` | Typed IDs, big-int amounts, error enums, chain/asset domain models, split registry with validation, provider-neutral signing trait, 5-trait `ChainService` surface (`ChainCodec` / `ChainReader` / `FeeEstimator` / `ChainBroadcaster` / `ChainService`), `UnsignedBundle` wire type for split-host deployments, `Chain.default_derivation_path` + `Registry::chain()` for registry-driven HD paths |
| `atlas-evm` | Real EVM `ChainService` on alloy 2.x — RLP codec (legacy + EIP-1559), `eth_getBalance`/`eth_getTransactionCount`/`eth_getTransactionReceipt` reader, `eth_feeHistory`-based fee estimator, `eth_sendRawTransaction` broadcaster, orchestrator with `prepare_unsigned_bundle` / `assemble_and_broadcast` for server-builds / client-signs flows. In-tree `MockEvmChainService` for smoke tests + BDD |
| `atlas-signer-localkey` | secp256k1 reference signer with three construction paths: raw bytes, Web3 secret-storage JSON keystore, BIP-39 mnemonic + BIP-32 derivation. Path read from `Chain.default_derivation_path` rather than hardcoded |
| `atlas-verify` | Bolero properties (6), Kani proof scaffold (3 nightly proofs) |
| `atlas-scenarios` | Cucumber BDD scenarios (3 features, 9 scenarios, 35 steps) including registry-driven signer wiring |
| Official registry | Ethereum mainnet, Base mainnet, Solana mainnet · native ETH/SOL · Circle USDC across all 3 networks · embedded into atlas-core via `atlas_core::official` · EVM `defaultDerivationPath` = `m/44'/60'/0'/0/0`, Solana = `m/44'/501'/0'/0'` |
| Asset standards | `Native`, `Erc20`, `Spl` |
| Coverage | **99.80% line** workspace (every file ≥ 99% lines), 99.51% function — atlas-scenarios excluded; codecov target pinned at 95% |
| Tests | 162 across the workspace, plus 9 BDD scenarios |
| CI | fmt · clippy (Linux+macOS) · test (Linux+macOS) · BDD · doc (`-D warnings`) · WASM build · `cargo deny` · `cargo careful` · coverage · codecov patch + project |
| Nightly | mutation testing · Kani proofs · Bolero extended (100k iter) · full HTML coverage |

---

## Recently shipped

### Real EVM `ChainService` + `LocalKeySigner` — PR [#9](https://github.com/Milerius/Atlas/pull/9), 2026-05-10

Spec: [`docs/superpowers/specs/2026-05-08-atlas-chain-service-evm-design.md`](docs/superpowers/specs/2026-05-08-atlas-chain-service-evm-design.md)

- 5-trait split (`ChainCodec` / `ChainReader` / `FeeEstimator` /
  `ChainBroadcaster` / `ChainService`) on the atlas-core surface.
- `atlas-evm` crate on alloy 2.x: RLP encoding, fee estimation, broadcast,
  orchestrator. OP-Stack L1 fee oracle stubbed (`l1_fee_wei: None`); the hook
  is in place for a follow-up.
- `atlas-signer-localkey` crate with three construction paths: raw bytes,
  Web3 keystore, BIP-39 + BIP-32 mnemonic. Path is registry-driven via
  `Chain.default_derivation_path`.
- Split-host deployment: `EvmChainService::prepare_unsigned_bundle` (no
  signer) + `assemble_and_broadcast` (no reader/estimator) with a JSON
  `UnsignedBundle` wire format. Proven by [`crates/atlas-evm/tests/split_host_flow.rs`](crates/atlas-evm/tests/split_host_flow.rs).

`MockEvmChainService` moved out of atlas-core into `atlas_evm::mock` —
atlas-core now ships only the trait surface.

---

## Near-term (next 1–3 PRs)

### `AddressRef` validation

- EIP-55 checksum for EVM addresses.
- Base58 + length check for Solana pubkeys.
- Chain-aware validation at boundary call sites (e.g. `prepare_transfer`).

`AddressRef` is a typed string today; this PR makes it actually validate.
Probably fits in ~150 LOC + property tests. Closes a typed-string gap that
predates the EVM service work.

### CAIP-2 / CAIP-19 typed parsers

Promote `NetworkId` and `AssetInstanceId` from CAIP-shaped strings to
structured `(namespace, reference)` decomposition. Catches malformed ids at
the registry boundary, adds Bolero property tests for parser round-trips.
Pairs naturally with the AddressRef PR — both tighten the typed-id boundary.

### OP-Stack L1 fee oracle

`EvmFee::Eip1559` carries an `l1_fee_wei` field that today is always `None`.
A small follow-up adds the [GasPriceOracle](https://docs.optimism.io/builders/dapp-developers/transactions/fees) read at fee-estimation
time for OP-Stack networks (Base, Optimism). The flag in `Network.features`
(`opStackL1Fee: true` for Base in the registry) is already wired.

---

## Mid-term

### Real Solana `ChainService`

- Same 5-trait split as EVM, separate crate `atlas-solana`.
- Message encoding (`solana-sdk` or pure-Rust serializer).
- Recent-blockhash management with caching.
- SPL transfer instruction encoding.
- RPC client (Helius / public mainnet / `solana-client`).

The Solana entries are already in the official registry; this PR makes them
executable. Pressures the boundary on a non-EVM chain — will surface
hard-coded EVM assumptions (which we'd then fix in `atlas-core`). The
registry-driven HD path (`m/44'/501'/0'/0'`) is already there too.

### More `SignerProvider` adapters

`atlas-signer-localkey` is the reference shape; each new adapter is its own
crate implementing the same `SignerProvider` trait so callers can swap:

- `atlas-signer-mpc-cramium` (or generic) — distributed signing with
  Guardian (cloud) or Silicon-style hardware (BLE/USB transport).
- `atlas-signer-privy` — Privy embedded-wallet authentication flow.
- `atlas-signer-aa-4337` — ERC-4337 account-abstraction relay (UserOperation
  signing + bundler submission).
- `atlas-signer-ledger` / `atlas-signer-trezor` — hardware wallet adapters
  via USB / WebUSB.
- `atlas-signer-walletconnect` — relay-mediated remote signing.

The split-host wire format (`UnsignedBundle` + `SigningRequest`) means each
new adapter only has to implement `sign(SigningRequest) -> SigningResponse`;
codec, reader, fee, and broadcaster are reusable across signers.

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

`atlas-evm` is now the natural home for serious verification:

- **Kani proofs** for `bigint_to_u256` / `bigint_to_u128` totality,
  signature-byte parsing exhaustiveness, and the legacy / EIP-1559
  encode→decode round-trip. The 3 atlas-core scaffolding proofs stay
  narrow; the EVM crate is where bounded model checking earns its keep.
- **Differential tests** — Atlas's RLP encoder vs alloy's reference impl
  on the same inputs (one such cross-check already lives in
  [`crates/atlas-evm/tests/codec_native.rs`](crates/atlas-evm/tests/codec_native.rs); generalize it under Bolero).
- **Anvil-backed integration tests** in CI (gated, occasional) to exercise
  the full broadcast path against a local devnet.

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

- Codec-only feature gate on `atlas-evm` (or a sister crate `atlas-evm-codec`)
  so server-side build-without-RPC consumers don't pull `alloy-provider`'s
  HTTP deps. The codec module is already pure; this is just packaging.
- **Replace the hand-rolled fee estimator with `Provider::estimate_eip1559_fees()`** —
  alloy 2.x exposes the same `eth_feeHistory`-based algorithm we wrote by
  hand in [`crates/atlas-evm/src/fee_estimator.rs`](crates/atlas-evm/src/fee_estimator.rs). Drops ~50 LOC, keeps the
  `EvmFee::Eip1559` shape and per-asset gas floor logic.
- Bolero property tests over `EvmCodec::prepare_transfer` round-trips —
  generate `(intent, fee, standard, contract)` quads, encode, decode through
  alloy, recover sender, assert equality.
- Token list snapshot under `registries/tokens/` for community-curated extras.
- BDD scenarios that exercise multi-network flows (e.g. "USDC on Base ↔
  USDC on Ethereum") and split-host build/sign through the real
  `EvmChainService` rather than only the mock.

If you have a use case that's not represented here, file an issue or open a
discussion on the repo. Atlas's design works backwards from real consumer
needs.
