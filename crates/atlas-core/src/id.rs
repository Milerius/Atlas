//! Typed identifiers used throughout atlas-core.
//!
//! Every id is a non-empty-string newtype. Construction goes through
//! [`Id::new`] (or one of the typed wrappers' `new` / `FromStr` impls) so
//! empty / whitespace-only strings are rejected at the boundary, before
//! the value reaches the registry or chain service.
//!
//! The eight concrete typed ids are each a thin newtype over [`Id`]:
//!
//! - [`ChainId`] — chain-family identifier (e.g. `evm`, `solana`).
//! - [`NetworkId`] — concrete network in CAIP-2 form
//!   (e.g. `eip155:1`, `solana:5eykt4Us…vdp`).
//! - [`AssetGroupId`] — display-level grouping (e.g. `usdc`, `eth`).
//! - [`AssetInstrumentId`] — issuer-level instrument
//!   (e.g. `usdc.circle`, `eth.native`).
//! - [`AssetInstanceId`] — concrete on-chain instance in CAIP-19-ish
//!   form (e.g. `eip155:8453/native:eth`).
//! - [`SignerId`] — signer-provider identifier.
//! - [`AccountRef`] — opaque account identifier handed to chain services.
//! - [`AddressRef`] — recipient address (per-chain format validation
//!   deferred; today this is a typed string).

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Internal non-empty-string newtype that backs every typed id.
///
/// Direct use is rare; prefer [`ChainId`], [`NetworkId`], …
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// Construct a new [`Id`], rejecting empty / whitespace-only input
    /// with [`IdError::Empty`].
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self(value))
    }

    /// Borrow the underlying string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Id {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Errors raised by [`Id`] construction.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum IdError {
    /// Input was empty or contained only whitespace characters.
    #[error("id must not be empty")]
    Empty,
}

/// Generate a typed-id newtype around [`Id`].
///
/// The macro accepts and forwards arbitrary `#[…]` attributes (including
/// `///` doc comments) to the generated `pub struct`, so each typed id
/// can carry its own documentation.
macro_rules! typed_id {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Id);

        impl $name {
            /// Construct a new typed id, rejecting empty / whitespace-only
            /// input with [`IdError::Empty`].
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                Id::new(value).map(Self)
            }

            /// Borrow the underlying string slice.
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

typed_id! {
    /// Chain-family identifier — e.g. `"evm"`, `"solana"`, future
    /// `"sui"`, `"utxo"`. References [`crate::chain::Chain::id`].
    ChainId
}
typed_id! {
    /// Concrete deployed network in CAIP-2 form — e.g. `"eip155:1"`
    /// (Ethereum), `"eip155:8453"` (Base),
    /// `"solana:5eykt4Us…vdp"` (Solana mainnet).
    NetworkId
}
typed_id! {
    /// Display-level asset grouping — e.g. `"usdc"`, `"eth"`. Used for
    /// search, pricing, and aggregation; never for signing.
    AssetGroupId
}
typed_id! {
    /// Issuer-level token instrument — e.g. `"usdc.circle"`,
    /// `"eth.native"`. Owns metadata that's stable across networks.
    AssetInstrumentId
}
typed_id! {
    /// Concrete on-chain asset instance in CAIP-19-ish form — e.g.
    /// `"eip155:8453/native:eth"`,
    /// `"eip155:1/erc20:0xA0b8…eB48"`,
    /// `"solana:5eykt4Us…vdp/spl:EPjF…Dt1v"`. The only id shape that
    /// signs or broadcasts.
    AssetInstanceId
}
typed_id! {
    /// Identifier of a configured [`crate::signing::SignerProvider`].
    SignerId
}
typed_id! {
    /// Opaque account identifier handed to chain services. Maps to
    /// whatever account abstraction the consumer uses (key index,
    /// account contract address, MPC group id, …).
    AccountRef
}
typed_id! {
    /// Recipient address on a transfer. Currently a typed string with
    /// no per-chain format validation; EIP-55 / base58 / chain-aware
    /// validation is a deferred follow-up.
    AddressRef
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_id_rejects_empty() {
        assert_eq!(
            NetworkId::new("").unwrap_err().to_string(),
            "id must not be empty"
        );
    }

    #[test]
    fn asset_instance_id_preserves_value() {
        let id = AssetInstanceId::new("eip155:8453/native:eth").unwrap();
        assert_eq!(id.as_str(), "eip155:8453/native:eth");
    }

    #[test]
    fn id_rejects_whitespace_only() {
        assert_eq!(Id::new("   ").unwrap_err(), IdError::Empty);
    }

    #[test]
    fn id_from_str_round_trips() {
        let id = Id::from_str("evm").unwrap();
        assert_eq!(id.as_str(), "evm");
        assert_eq!(format!("{id}"), "evm");
    }

    #[test]
    fn typed_id_from_str_and_display() {
        let id = NetworkId::from_str("eip155:1").unwrap();
        assert_eq!(format!("{id}"), "eip155:1");
    }
}
