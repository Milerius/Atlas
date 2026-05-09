//! Real EVM `ChainService` for the Atlas SDK, built on alloy.
//!
//! Implements all 5 traits from `atlas_core::service`:
//! - [`codec::EvmCodec`] — pure RLP encoding (legacy + EIP-1559)
//! - [`reader::EvmReader`] — RPC reads via `alloy-provider`
//! - [`fee_estimator::EvmFeeEstimator`] — `eth_feeHistory`-based EIP-1559
//!   suggestion with a legacy `eth_gasPrice` fallback. OP-Stack L1 fee oracle
//!   integration is deferred (the `EvmFee::Eip1559` variant always emits
//!   `l1_fee_wei: None` for now).
//! - [`broadcaster::EvmBroadcaster`] — `eth_sendRawTransaction`
//! - [`service::EvmChainService`] — orchestrator
//!
//! The codec is pure — no `Provider`, no I/O. Apps that build transactions
//! server-side and sign client-side depend only on the codec; readers /
//! fee estimator / broadcaster are separate concrete impls that hold a
//! `RootProvider<Http>` (or any `Provider` implementation).

#![forbid(unsafe_code)]

pub mod abi;
pub mod broadcaster;
pub mod codec;
pub mod error;
pub mod fee_estimator;
pub mod reader;
pub mod service;
