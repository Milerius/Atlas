# Atlas Core Blockchain Layer Design

## Summary

Atlas will start as a strict blockchain core SDK with a small, extensible data
model for chains, networks, assets, chain services, and signer providers. The
first implementation should stay intentionally narrow: EVM, native coins,
fungible tokens, public RPC, and abstract signing. Product accounts, cards,
stocks, lending, staking, swaps, and portfolio APIs are future layers.

The design still preserves the shape required for those future layers. It
separates user-facing asset identity from executable asset instances, splits
slow-moving chain data from fast-moving asset data, and keeps signing
provider-neutral so MPC, local keys, Privy, account abstraction, and future
signers can plug in without changing chain adapters.

## Goals

- Define precise meanings for `Chain`, `Network`, `AssetGroup`,
  `AssetInstrument`, and `AssetInstance`.
- Support unified asset UX, such as showing one USDC balance, while preserving
  exact per-network execution.
- Keep chain services deterministic by requiring concrete `AssetInstance`
  inputs for execution.
- Keep canonical registries clean and adapter-neutral.
- Establish signing as an abstraction rather than an MPC-specific workflow.
- Capture defensive programming, precision, and testing expectations for the
  first implementation.

## Non-Goals

- No card account, broker account, product account, or neobank account model in
  the first implementation.
- No unified portfolio API in the first implementation.
- No swap, bridge, staking, lending, or tokenized stock execution module in the
  first implementation.
- No concrete MPC, Privy, local-key, or account-abstraction signer
  implementation unless needed as a test double.
- No private RPC, indexing backend, pricing backend, or balance API in the first
  implementation.

## Architecture

The first core should be split into focused modules:

- `atlas-registry`: chain registry, asset registry, validation, layered loading,
  and id resolution.
- `atlas-assets`: asset groups, instruments, instances, taxonomy, and
  executable-asset validation.
- `atlas-chains`: chain service trait and chain-family adapters. EVM is first.
- `atlas-rpc`: public RPC endpoint config and RPC client abstraction.
- `atlas-signing`: signer provider trait, signer refs, signing requests, and
  signing responses.
- `atlas-transactions`: transaction intents, fee quotes, unsigned
  transactions, signable payloads, signed transactions, and broadcast results.

The core data relationship is:

```text
Chain
  -> Network
      -> AssetInstance
          -> AssetInstrument
              -> AssetGroup
```

The execution rule is:

```text
ChainService executes AssetInstance only.
Resolvers may work with AssetGroup or AssetInstrument.
```

This keeps chain adapters precise. For example, EVM should know how to transfer
USDC ERC-20 on Base; it should not know what global "USDC" means.

## Core Entities

### Chain

`Chain` is the execution family or service adapter. Examples: `evm`, `solana`,
`sui`, `utxo`.

```text
Chain
  id: "evm"
  name: "EVM"
  family: account_based | utxo | object_based
  address_format
  default_curve
  supported_standards
  capabilities
```

Example:

```json
{
  "id": "evm",
  "name": "EVM",
  "family": "account_based",
  "addressFormat": "evm_address",
  "defaultCurve": "secp256k1",
  "supportedStandards": ["native", "erc20"],
  "capabilities": ["balance", "transfer", "approve", "contract_call", "broadcast"]
}
```

### Network

`Network` is a concrete deployed environment for a chain. Examples:
`eip155:1` for Ethereum mainnet and `eip155:8453` for Base.

Atlas should prefer CAIP-style ids as canonical network ids and may expose
developer-friendly aliases such as `ethereum` and `base`.

```text
Network
  id: "eip155:1"
  alias: "ethereum"
  chain: "evm"
  name: "Ethereum"
  environment: mainnet | testnet | devnet
  native_asset_instance_id
  rpc
  explorers
  features
```

Example:

```json
{
  "id": "eip155:8453",
  "alias": "base",
  "chainId": "8453",
  "chain": "evm",
  "name": "Base",
  "environment": "mainnet",
  "nativeAssetInstanceId": "eip155:8453/native:eth",
  "rpc": {
    "defaultUrl": "https://mainnet.base.org"
  },
  "explorers": [
    {
      "name": "BaseScan",
      "tx": "https://basescan.org/tx/{txid}",
      "address": "https://basescan.org/address/{address}"
    }
  ],
  "features": {
    "eip1559": true,
    "erc20": true,
    "opStackL1Fee": true
  }
}
```

Wallet Core ids, provider-specific RPC options, and generated binding details
must stay outside `Network` and live in adapters.

### AssetGroup

`AssetGroup` is a user-facing and portfolio-facing grouping. It is useful for
search, display, pricing, routing, and aggregation.

```text
AssetGroup
  id: "usdc"
  symbol: "USDC"
  name: "USDC"
  metadata
```

`AssetGroup` is not executable. Atlas must never sign or broadcast directly
from an asset group.

### AssetInstrument

`AssetInstrument` is the economic or legal instrument. In the first scope this
will cover native coins and fungible tokens. Later it can cover stablecoins,
stocks, tokenized stocks, vault shares, liquid staked assets, and more.

```text
AssetInstrument
  id: "usdc.circle"
  group_id: "usdc"
  asset_class
  kind
  symbol
  name
  decimals
  optional issuer
  traits
  metadata
```

Example:

```json
{
  "id": "usdc.circle",
  "groupId": "usdc",
  "assetClass": "crypto",
  "kind": "fungible_token",
  "symbol": "USDC",
  "name": "USD Coin",
  "issuer": "Circle",
  "decimals": 6,
  "traits": ["fungible", "transferable"]
}
```

### AssetInstance

`AssetInstance` is the concrete executable representation on a network.
Balances, transfers, approvals, fee estimation, signing, and broadcasting work
against asset instances.

```text
AssetInstance
  id: "eip155:8453/erc20:0x8335..."
  instrument_id: "usdc.circle"
  network: "eip155:8453"
  standard
  decimals
  identifier
  capabilities
  metadata
```

Example:

```json
{
  "id": "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
  "instrumentId": "usdc.circle",
  "network": "eip155:8453",
  "standard": "erc20",
  "contract": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
  "decimals": 6,
  "capabilities": ["balance", "transfer", "approve", "swap"]
}
```

The first taxonomy subset is:

```text
AssetClass: crypto
InstrumentKind: native_coin, fungible_token
AssetStandard: native, erc20
AssetTrait: fungible, transferable, gas_asset
```

Future taxonomy can add:

```text
AssetClass: equity, fiat, fund, debt, commodity, derivative
InstrumentKind: stablecoin, stock, tokenized_stock, vault_share, liquid_staked_asset
AssetStandard: spl, token2022, sui_coin, broker_ledger, custodial_ledger
AssetTrait: rebasing, yield_bearing, permissioned, redeemable, fractional
```

## ETH And USDC Examples

ETH should be modeled as one instrument with multiple native instances:

```json
{
  "group": {
    "id": "eth",
    "symbol": "ETH",
    "name": "Ethereum"
  },
  "instrument": {
    "id": "eth.native",
    "groupId": "eth",
    "assetClass": "crypto",
    "kind": "native_coin",
    "symbol": "ETH",
    "name": "Ether",
    "decimals": 18,
    "traits": ["fungible", "transferable", "gas_asset"]
  },
  "instances": [
    {
      "id": "eip155:1/native:eth",
      "instrumentId": "eth.native",
      "network": "eip155:1",
      "standard": "native",
      "decimals": 18,
      "capabilities": ["balance", "transfer", "pay_gas"]
    },
    {
      "id": "eip155:8453/native:eth",
      "instrumentId": "eth.native",
      "network": "eip155:8453",
      "standard": "native",
      "decimals": 18,
      "capabilities": ["balance", "transfer", "pay_gas"]
    }
  ]
}
```

USDC should be modeled as one instrument with multiple token instances:

```json
{
  "group": {
    "id": "usdc",
    "symbol": "USDC",
    "name": "USDC"
  },
  "instrument": {
    "id": "usdc.circle",
    "groupId": "usdc",
    "assetClass": "crypto",
    "kind": "fungible_token",
    "symbol": "USDC",
    "name": "USD Coin",
    "issuer": "Circle",
    "decimals": 6,
    "traits": ["fungible", "transferable"]
  },
  "instances": [
    {
      "id": "eip155:1/erc20:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
      "instrumentId": "usdc.circle",
      "network": "eip155:1",
      "standard": "erc20",
      "contract": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
      "decimals": 6,
      "capabilities": ["balance", "transfer", "approve", "swap"]
    },
    {
      "id": "eip155:8453/erc20:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
      "instrumentId": "usdc.circle",
      "network": "eip155:8453",
      "standard": "erc20",
      "contract": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
      "decimals": 6,
      "capabilities": ["balance", "transfer", "approve", "swap"]
    }
  ]
}
```

Portfolio aggregation can group by `AssetInstrument` or `AssetGroup`, while
execution remains instance-specific.

```text
Portfolio aggregates by AssetInstrument or AssetGroup.
Transactions execute against AssetInstance.
```

## Registries

Atlas should split slow-moving chain data from faster-moving asset data.

```text
chain_registry.json
asset_registry.json
```

`chain_registry` contains:

- chains
- networks
- RPC defaults
- explorers
- network feature flags
- native asset instance references

`asset_registry` contains:

- asset groups
- asset instruments
- asset instances
- token metadata
- contract or mint identifiers
- decimals
- icons
- issuer metadata

Registries connect through stable ids:

```text
Network.chain -> Chain.id
Network.native_asset_instance_id -> AssetInstance.id
AssetInstance.network -> Network.id
AssetInstance.instrument_id -> AssetInstrument.id
AssetInstrument.group_id -> AssetGroup.id
```

Registry loading should support layers:

```text
Bundled Chain Registry
Bundled Asset Registry
Remote Asset Registry
Local Asset Registry
```

Resolution order:

```text
local override
remote curated
bundled curated
unknown/discovered fallback
```

Discovered assets can create executable asset instances, but must not claim a
global asset group identity unless a trusted registry verifies that mapping.

## Service And Signing Flow

Lowest-level chain services operate on concrete asset instances:

```text
ChainService
  get_balance(account/address, asset_instance)
  estimate_fee(intent, from, asset_instance)
  prepare_transaction(intent, from, fee)
  signable_payload(unsigned_transaction)
  assemble_signed_transaction(unsigned_transaction, signing_response)
  broadcast(signed_transaction)
```

EVM ERC-20 transfer flow:

```text
TransferIntent
  asset_instance: Base USDC
  to: 0xRecipient
  amount: 100000000

EvmService
  validates Base USDC is erc20
  builds contract call transfer(to, amount)
  estimates gas
  builds unsigned EVM transaction
  creates SignablePayload digest
```

Signer abstraction:

```text
SignerProvider
  id
  capabilities
  public_key(account_ref, curve)
  sign(SigningRequest) -> SigningResponse
```

Signing requests are typed. They should be able to represent digest signing,
whole transaction signing, message signing, typed data signing, and future
provider-specific execution.

```text
SigningRequest
  account_ref
  chain
  network
  curve
  payload_kind
  payload
  metadata
```

Signing responses must allow different provider shapes:

```text
signature_only
signed_transaction
submitted_transaction
```

Examples:

- MPC signer signs a digest and returns a raw signature.
- Local signer signs a digest and returns a raw signature.
- Privy signer may sign a digest or sign the whole transaction depending on
  capabilities.
- Account abstraction signer may return a user operation hash, transaction hash,
  or sponsored execution result.

## Precision Rules

Atlas must never use floating-point numbers for money, token amounts, balances,
fees, prices, rates, percentages, or quantities.

Use:

- Arbitrary-precision integers or chain-native integer types for raw base units:
  wei, satoshi, token units, gas units, nonces, and block numbers.
- Decimal or fixed-scale decimal domain types for prices, FX rates, APY,
  percentages, and fiat values.

Authoritative amount shape:

```text
RawAmount
  value: BigInt
  decimals: u8
```

Display strings are derived values, not authoritative values.

Arithmetic should be checked by default. Saturating, wrapping, or lossy
conversions must be explicit in the API name and tested.

## Validation Rules

- `Network.chain` must reference an existing `Chain`.
- `Network.native_asset_instance_id` must reference an `AssetInstance` on that
  network.
- `AssetInstance.network` must reference an existing `Network`.
- `AssetInstance.instrument_id` must reference an existing `AssetInstrument`.
- `AssetInstrument.group_id` must reference an existing `AssetGroup`.
- Native asset instances must not have a contract.
- ERC-20 asset instances must have a contract.
- Chain services must reject unsupported asset standards.
- Executable operations on `AssetGroup` must fail before signing.
- Unknown registry versions must be rejected.

## Error Model

Errors should be typed by layer.

```text
RegistryError
  missing_chain
  missing_network
  missing_asset_group
  missing_asset_instrument
  missing_asset_instance
  invalid_registry_reference
  unsupported_registry_version
```

```text
AssetError
  unsupported_standard
  invalid_contract
  invalid_decimals
  missing_required_identifier
  asset_not_executable
```

```text
ChainError
  unsupported_chain
  unsupported_network
  unsupported_asset_instance
  invalid_address
  fee_estimation_failed
  transaction_build_failed
  broadcast_failed
```

```text
SigningError
  signer_not_found
  unsupported_curve
  unsupported_payload
  user_rejected
  signature_failed
  invalid_signature
```

```text
RpcError
  transport
  timeout
  rate_limited
  malformed_response
  node_error
```

Follow the repository-wide rule in `CODEX.md`: no panics in SDK/runtime paths;
typed errors at trait and public API boundaries.

## Testing Notes

Atlas should follow the defensive style used in the user's Rust projects:
small unit tests, fixture tests, smoke tests, integration tests, BDD scenarios
for product flows, and property/fuzz tests as parsers and invariants mature.

Initial test layers:

- Unit tests for registry validation, asset resolution, chain/network lookup,
  EVM native/ERC-20 intent validation, and typed error variants.
- Fixture tests for valid and invalid registries, missing references, native
  asset rules, ERC-20 token rules, and multi-instance assets like ETH and USDC.
- Smoke tests for registry loading, asset lookup, chain service preparation,
  mock signing, and broadcast stubbing.
- Minimal integration tests for public RPC balance queries where stable enough.

Later test layers:

- Property tests for registry invariants and id parsing round-trips.
- Fuzz tests for registry JSON, CAIP/network ids, asset instance ids, addresses,
  and decimal conversions.
- Integration tests for Wallet Core transaction compiler adapters and signer
  provider adapters.
- BDD scenarios for unified portfolio balance, exact network send, signer
  mismatch, and discovered token behavior.

Example future BDD notes:

```gherkin
Given a wallet has USDC on Base and Ethereum
When portfolio asks for USDC
Then it returns a unified total and a per-network breakdown
```

```gherkin
Given a user sends USDC on Base
When Atlas prepares the transaction
Then it resolves the Base asset instance
And it never signs an AssetGroup
```

## Future Design Documents

The following topics should be separate specs:

- Product accounts and neobank-style account containers:
  `docs/superpowers/specs/2026-05-08-atlas-product-accounts-composition-notes.md`.
- Unified portfolio and backend balance APIs.
- Token discovery and remote asset registry reconciliation.
- Concrete signer providers: local keys, MPC, Privy, account abstraction.
- Solana, Sui, and UTXO chain adapters.
- Swaps, bridges, staking, lending, and yield positions.
- Stocks, tokenized equities, broker rails, and custodial ledgers.
