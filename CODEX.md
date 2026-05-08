# Codex Engineering Rules

These rules apply across the Atlas repository. They are intentionally broader
than a single feature spec and should guide implementation, review, and tests.

## Core Principles

- Keep Atlas modular and composable. The SDK provides primitives, registries,
  adapters, and clean extension points; it must not impose a wallet product
  style on consumers.
- Prefer explicit domain models over loosely typed strings or ad hoc maps.
- Keep execution concrete. UX and portfolio layers may aggregate, but low-level
  chain execution must resolve to concrete instances first.
- Keep adapter-specific details out of canonical registries. Wallet Core ids,
  RPC vendor quirks, provider-specific payloads, and codegen details belong in
  adapters.

## Precision And Money

- Never use `float`, `double`, `f32`, or `f64` for money, token amounts,
  balances, fees, prices, rates, percentages, or quantities.
- Use arbitrary-precision integers or chain-native integer types for raw base
  units: wei, satoshi, token units, gas units, nonces, and block numbers.
- Use decimal or fixed-scale decimal domain types for prices, FX rates, APY,
  percentages, and fiat values.
- Prefer checked arithmetic by default. Saturating, wrapping, or lossy
  conversions must be explicit in the API name and covered by tests.
- Preserve raw amounts and decimals separately. Display strings are derived
  values, not authoritative values.

## Errors And Defensive Programming

- No panics in SDK/runtime paths. Return typed errors instead.
- Use typed error enums at trait and public API boundaries so consumers can
  match on failures.
- Use broad error wrappers only at application/test leaf layers where errors are
  being reported to humans.
- Validate external input before use: registries, ids, addresses, contracts,
  RPC responses, signer responses, and generated bindings.
- Reject unknown registry versions.
- Reject invalid registry references before exposing a registry to services.
- Error messages should name the failing id and expected relationship when safe:
  for example, an asset instance referencing a missing network.

## Registry Rules

- Split slow-moving chain data from fast-moving asset data.
- `chain_registry` owns chains, networks, RPC defaults, explorers, network
  features, and native asset references.
- `asset_registry` owns asset groups, instruments, instances, token metadata,
  contracts, mints, decimals, icons, issuer metadata, and discovered assets.
- Registries connect through stable ids:
  - `Network.chain -> Chain.id`
  - `Network.native_asset_instance_id -> AssetInstance.id`
  - `AssetInstance.network -> Network.id`
  - `AssetInstance.instrument_id -> AssetInstrument.id`
  - `AssetInstrument.group_id -> AssetGroup.id`
- Asset groups and instruments are for display, search, pricing, routing, and
  aggregation. Asset instances are for balances, fees, transfers, approvals,
  signing, and broadcasting.
- Never sign or broadcast from an `AssetGroup`. Resolve to an `AssetInstance`
  first.

## Chain And Signing Boundaries

- `Chain` means execution family or adapter: EVM, Solana, Sui, UTXO.
- `Network` means concrete deployed environment: Ethereum mainnet, Base,
  Solana mainnet, Bitcoin mainnet.
- `AssetInstance` means concrete executable representation on a network:
  native ETH on Base, ERC-20 USDC on Ethereum, SPL USDC on Solana.
- Chain services should operate on concrete `AssetInstance`s only.
- Higher layers may accept `AssetGroup` or `AssetInstrument`, but must resolve
  before execution.
- Signing must be provider-neutral. MPC, local keys, Privy, account abstraction,
  and future signers plug into a common request/response boundary.
- A signer may return a raw signature, a signed transaction, or a submitted
  transaction result depending on its capabilities. Do not force all providers
  into one custody model.

## Testing Strategy

Start small, but keep the layers ready.

- Unit tests cover domain validation, registry parsing, id resolution, typed
  errors, and chain-service boundary behavior.
- Fixture tests cover valid and invalid registries, missing references, native
  asset rules, token rules, and multi-instance assets like ETH and USDC.
- Smoke tests cover a minimal end-to-end path through registry loading, asset
  lookup, chain service preparation, mock signing, and broadcast stubbing.
- Integration tests are for real external behavior such as public RPC calls,
  transaction compiler adapters, and signer provider adapters.
- BDD tests are useful for product-level flows later, such as unified USDC
  portfolio balance, exact network send, signer mismatch, and discovered token
  behavior.
- Property/fuzz tests should be added for parsers and invariants as the model
  grows: registry JSON, CAIP/network ids, asset instance ids, addresses, and
  decimal conversions.

## First-Scope Bias

- Start with the strict blockchain core.
- Start with EVM, native coins, and fungible tokens.
- Keep enums small at first, but design them to grow:
  - `AssetClass`: initially `crypto`
  - `InstrumentKind`: initially `native_coin`, `fungible_token`
  - `AssetStandard`: initially `native`, `erc20`
  - `AssetTrait`: initially `fungible`, `transferable`, `gas_asset`
- Defer product accounts, cards, stocks, lending, staking, swaps, and unified
  portfolio APIs to later modules, while preserving the model that will support
  them.
