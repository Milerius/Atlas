# Atlas Official Registry

Atlas's curated chain and asset registry, shipped in-tree as JSON.

| File | Schema | What's inside |
|---|---|---|
| [`chain_registry.json`](chain_registry.json) | `ChainRegistryDocument` | 2 chain families (EVM, Solana) and 3 networks (Ethereum mainnet, Base mainnet, Solana mainnet) |
| [`asset_registry.json`](asset_registry.json) | `AssetRegistryDocument` | 3 groups (`eth`, `usdc`, `sol`), 3 instruments, 6 instances |

## Loading

```rust
use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};

let chain_doc: ChainRegistryDocument = serde_json::from_str(CHAIN_REGISTRY_JSON).unwrap();
let asset_doc: AssetRegistryDocument = serde_json::from_str(ASSET_REGISTRY_JSON).unwrap();
let registry = Registry::from_documents(chain_doc, asset_doc).unwrap();
```

The JSON files are embedded into `atlas-core` at compile time via `include_str!`, so consumers get them for free without a filesystem round-trip.

## Schema notes

- **`chainId`** is an EVM-specific decimal string (e.g. `"1"`, `"8453"`). It's optional on the [`Network`](../crates/atlas-core/src/chain.rs) struct and omitted entirely for non-EVM networks (Solana, future Sui / UTXO). Don't rely on it as a universal identifier — use `Network.id` (CAIP-2 form) when you need a stable cross-chain key.

## Coverage

| Group | Instrument | Network | ID |
|---|---|---|---|
| `eth` | `eth.native` | `eip155:1` | `eip155:1/native:eth` |
| `eth` | `eth.native` | `eip155:8453` | `eip155:8453/native:eth` |
| `usdc` | `usdc.circle` | `eip155:1` | `eip155:1/erc20:0xA0b8…eB48` |
| `usdc` | `usdc.circle` | `eip155:8453` | `eip155:8453/erc20:0x8335…2913` |
| `usdc` | `usdc.circle` | `solana:5eykt4Us…vdp` | `solana:5eykt4Us…vdp/spl:EPjF…Dt1v` |
| `sol` | `sol.native` | `solana:5eykt4Us…vdp` | `solana:5eykt4Us…vdp/native:sol` |

## Bring your own

Atlas does not assume the official registry is the only valid one. Consumers can:
- Load the official registry and treat it as a base.
- Bring their own JSON conforming to the same schemas.
- Mix and match — pull chain data from the official registry and asset data from elsewhere, or vice versa.

The `Registry::from_documents` boundary validates whatever you hand it.

## Note on the `defaultUrl` fields

The RPC URLs in `chain_registry.json` are placeholders (`https://rpc.example.com/<chain>`). atlas-core does not invoke them — it has no HTTP client today, only `MockEvmService`. Consumers wiring up a real chain service should override `RpcConfig.default_url` with their own provider URL (Alchemy, Infura, Helius, QuickNode, self-hosted, etc.).
