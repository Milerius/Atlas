//! Verification harness for atlas-core.
//!
//! Two layers:
//! - **Bolero property tests** — run on every PR via `cargo test -p atlas-verify`.
//!   Generated input across the registry / amount / asset / id surface,
//!   asserting hand-written invariants.
//! - **Kani proofs** (`#[cfg(kani)]`) — run nightly via `cargo kani -p atlas-verify`.
//!   Bounded model checking for the few totalities that survive the BigInt /
//!   BTreeMap heap-allocation limitations Kani has today. Real proof leverage
//!   arrives with the EVM crate (RLP length prefixes, gas u64 arithmetic,
//!   nonce comparison).
#![forbid(unsafe_code)]

#[cfg(kani)]
mod proofs;
