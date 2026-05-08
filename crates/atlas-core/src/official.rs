//! Atlas's official, in-tree registry as embedded JSON.
//!
//! Use these constants when you want the curated chain + asset set Atlas
//! ships with — Ethereum, Base, Solana, plus native coins and the Circle
//! USDC instrument across all three networks.
//!
//! ```no_run
//! use atlas_core::official::{ASSET_REGISTRY_JSON, CHAIN_REGISTRY_JSON};
//! use atlas_core::registry::{AssetRegistryDocument, ChainRegistryDocument, Registry};
//!
//! let chain_doc: ChainRegistryDocument = serde_json::from_str(CHAIN_REGISTRY_JSON).unwrap();
//! let asset_doc: AssetRegistryDocument = serde_json::from_str(ASSET_REGISTRY_JSON).unwrap();
//! let registry = Registry::from_documents(chain_doc, asset_doc).unwrap();
//! ```
//!
//! Consumers can also bring their own registry and skip these entirely;
//! Atlas does not assume any particular network or asset set at the
//! `Registry::from_documents` boundary.

// NOTE: the include_str! paths reach outside the crate root, which works
// fine for workspace-internal builds but blocks `cargo package` /
// `cargo publish`. atlas-core is not on a publish track today; when it
// is, mirror the JSON into the crate via a build.rs that copies into
// `OUT_DIR`, or move the files under `crates/atlas-core/registries/`.

/// Atlas's official chain registry document, as embedded JSON.
///
/// Lists the EVM and Solana chain families and three networks: Ethereum
/// mainnet (`eip155:1`), Base mainnet (`eip155:8453`), and Solana mainnet
/// (`solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp`).
pub const CHAIN_REGISTRY_JSON: &str = include_str!("../../../registries/chain_registry.json");

/// Atlas's official asset registry document, as embedded JSON.
///
/// Defines the `eth`, `usdc`, and `sol` groups, three instruments
/// (`eth.native`, `usdc.circle`, `sol.native`), and six concrete instances
/// — native ETH on Ethereum + Base, Circle USDC on Ethereum + Base + Solana,
/// and native SOL on Solana mainnet.
pub const ASSET_REGISTRY_JSON: &str = include_str!("../../../registries/asset_registry.json");
