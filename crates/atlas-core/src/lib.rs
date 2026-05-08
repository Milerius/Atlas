//! Boundary types and registry validation for the Atlas blockchain SDK.
//!
//! `atlas-core` provides the typed primitives that every other Atlas crate
//! builds on:
//!
//! - **Typed identifiers** ([`id`]) — non-empty string newtypes for chains,
//!   networks, asset groups, instruments, instances, signers, accounts, and
//!   addresses. Constructed once and validated at the boundary.
//! - **Big-int raw amounts** ([`amount::RawAmount`]) — arbitrary-precision
//!   token amounts with a decimal scale, never `f64`.
//! - **Domain models** ([`chain`], [`asset`]) — `Chain`, `Network`,
//!   `AssetGroup`, `AssetInstrument`, `AssetInstance`, plus the enums that
//!   classify them.
//! - **Split registries** ([`registry`]) — chain data and asset data live in
//!   separate documents; `Registry::from_documents` validates cross-references
//!   and rejects unknown versions / duplicate ids before exposing anything.
//! - **Provider-neutral signing** ([`signing`]) — one `SignerProvider` trait
//!   covers MPC, local keys, Privy, account abstraction, and future signers.
//!   The response can be a raw signature, a signed transaction, or a
//!   submitted transaction result.
//! - **Per-chain fee shapes** ([`fee`]) — `Fee` envelope, `EvmFee` variants,
//!   and `TransactionStatus` for `ChainReader` returns.
//! - **Per-chain-family execution** ([`service`]) — `ChainService` trait with
//!   a `MockEvmService` smoke implementation. Chain services accept only
//!   concrete `AssetInstance`s; `AssetGroup` and `AssetInstrument` are for
//!   display, search, pricing, and routing.
//! - **Transaction lifecycle types** ([`transaction`]) — `TransferIntent`,
//!   `UnsignedTransaction`, `SignedTransaction`, `BroadcastResult`.
//! - **Embedded official registry** ([`official`]) — Atlas's curated chain
//!   and asset set, bundled in via `include_str!` and ready to deserialize.
//!
//! Every public path-or-trait error is a typed enum from [`error`]; no
//! panics in SDK paths. The crate is `#![forbid(unsafe_code)]`.
#![forbid(unsafe_code)]

pub mod amount;
pub mod asset;
pub mod chain;
pub mod error;
pub mod fee;
pub mod id;
pub mod official;
pub mod registry;
pub mod service;
pub mod signing;
pub mod transaction;

pub use amount::{AmountError, RawAmount};
pub use error::{AssetError, ChainError, RegistryError, RpcError, SigningError};
pub use fee::{EvmFee, Fee, TransactionStatus};
