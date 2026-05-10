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
    /// Input failed CAIP-2 / CAIP-19 validation. Surfaced by typed ids
    /// that promise CAIP shape ([`NetworkId`], [`AssetInstanceId`]).
    #[error("CAIP validation failed: {0}")]
    Caip(#[from] crate::caip::CaipError),
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
    /// Recipient address on a transfer.
    ///
    /// Stored as a typed string. Per-chain format validation
    /// (EIP-55 for EVM, base58 + length for Solana, …) is available
    /// via [`AddressRef::validate_for`] and the strict constructor
    /// [`AddressRef::for_format`]. Construction via
    /// [`AddressRef::new`] / `FromStr` is intentionally chain-agnostic
    /// (only rejects empty / whitespace) so the type can be carried
    /// through layers that don't yet know which chain owns it.
    AddressRef
}

// ── NetworkId — CAIP-2-validated ─────────────────────────────────────────

/// Concrete deployed network in CAIP-2 form — e.g. `"eip155:1"`
/// (Ethereum), `"eip155:8453"` (Base),
/// `"solana:5eykt4Us…vdp"` (Solana mainnet).
///
/// Validates CAIP-2 shape at construction and on JSON deserialisation,
/// rejecting typos like `eip_155:1`, `:1`, or `eip155:` before they
/// reach the registry.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct NetworkId(Id);

impl NetworkId {
    /// Construct a new [`NetworkId`], rejecting empty input
    /// ([`IdError::Empty`]) or non-CAIP-2 form ([`IdError::Caip`]).
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let id = Id::new(value)?;
        crate::caip::Caip2::parse(id.as_str())?;
        Ok(Self(id))
    }

    /// Borrow the underlying string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Return the parsed CAIP-2 decomposition. Total — the id was
    /// validated at construction, so re-parsing always succeeds.
    pub fn caip2(&self) -> crate::caip::Caip2 {
        crate::caip::Caip2::parse(self.as_str())
            .expect("NetworkId is CAIP-2-validated at construction")
    }

    /// CAIP-2 namespace — `"eip155"`, `"solana"`, etc.
    pub fn namespace(&self) -> &str {
        self.as_str()
            .split_once(':')
            .map(|(ns, _)| ns)
            .expect("NetworkId is CAIP-2-validated at construction")
    }

    /// CAIP-2 reference — chain-specific (`"1"` for Ethereum mainnet,
    /// `"5eykt4Us…vdp"` for Solana mainnet).
    pub fn reference(&self) -> &str {
        self.as_str()
            .split_once(':')
            .map(|(_, r)| r)
            .expect("NetworkId is CAIP-2-validated at construction")
    }
}

impl fmt::Display for NetworkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for NetworkId {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for NetworkId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        NetworkId::new(raw).map_err(serde::de::Error::custom)
    }
}

// ── AssetInstanceId — CAIP-19-validated ──────────────────────────────────

/// Concrete on-chain asset instance in CAIP-19 form — e.g.
/// `"eip155:8453/native:eth"`,
/// `"eip155:1/erc20:0xA0b8…eB48"`,
/// `"solana:5eykt4Us…vdp/spl:EPjF…Dt1v"`. The only id shape that signs
/// or broadcasts.
///
/// Validates CAIP-19 shape at construction and on JSON
/// deserialisation. Malformed input (missing `/`, missing `:`, illegal
/// characters) is rejected at the boundary.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct AssetInstanceId(Id);

impl AssetInstanceId {
    /// Construct a new [`AssetInstanceId`], rejecting empty input
    /// ([`IdError::Empty`]) or non-CAIP-19 form ([`IdError::Caip`]).
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let id = Id::new(value)?;
        crate::caip::Caip19::parse(id.as_str())?;
        Ok(Self(id))
    }

    /// Borrow the underlying string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Return the parsed CAIP-19 decomposition. Total — the id was
    /// validated at construction, so re-parsing always succeeds.
    pub fn caip19(&self) -> crate::caip::Caip19 {
        crate::caip::Caip19::parse(self.as_str())
            .expect("AssetInstanceId is CAIP-19-validated at construction")
    }

    /// The CAIP-2 chain segment. For an instance like
    /// `eip155:8453/native:eth` this returns the `eip155:8453` part.
    pub fn network_id(&self) -> NetworkId {
        let (chain_part, _) = self
            .as_str()
            .split_once('/')
            .expect("AssetInstanceId is CAIP-19-validated at construction");
        NetworkId::new(chain_part)
            .expect("AssetInstanceId chain segment is CAIP-2-validated at construction")
    }

    /// CAIP-19 asset namespace — `"native"`, `"erc20"`, `"spl"`.
    pub fn asset_namespace(&self) -> &str {
        let (_, asset_part) = self
            .as_str()
            .split_once('/')
            .expect("AssetInstanceId is CAIP-19-validated at construction");
        asset_part
            .split_once(':')
            .map(|(ns, _)| ns)
            .expect("AssetInstanceId is CAIP-19-validated at construction")
    }

    /// CAIP-19 asset reference — token symbol, contract address, mint, etc.
    pub fn asset_reference(&self) -> &str {
        let (_, asset_part) = self
            .as_str()
            .split_once('/')
            .expect("AssetInstanceId is CAIP-19-validated at construction");
        asset_part
            .split_once(':')
            .map(|(_, r)| r)
            .expect("AssetInstanceId is CAIP-19-validated at construction")
    }
}

impl fmt::Display for AssetInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for AssetInstanceId {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for AssetInstanceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        AssetInstanceId::new(raw).map_err(serde::de::Error::custom)
    }
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

    // ── CAIP-validated typed ids ─────────────────────────────────────────

    #[test]
    fn network_id_rejects_non_caip2_form() {
        // No `:` separator → CAIP-2 missing-separator error.
        let err = NetworkId::new("eip1551").unwrap_err();
        assert!(matches!(err, IdError::Caip(_)));
    }

    #[test]
    fn network_id_rejects_uppercase_namespace() {
        let err = NetworkId::new("EIP155:1").unwrap_err();
        assert!(matches!(err, IdError::Caip(_)));
    }

    #[test]
    fn network_id_exposes_caip2_namespace_and_reference() {
        let id = NetworkId::new("eip155:8453").unwrap();
        assert_eq!(id.namespace(), "eip155");
        assert_eq!(id.reference(), "8453");
    }

    #[test]
    fn network_id_caip2_round_trip_is_lossless() {
        let id = NetworkId::new("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").unwrap();
        let parsed = id.caip2();
        assert_eq!(format!("{parsed}"), id.as_str());
    }

    #[test]
    fn asset_instance_id_rejects_input_without_slash() {
        let err = AssetInstanceId::new("eip155:1").unwrap_err();
        assert!(matches!(err, IdError::Caip(_)));
    }

    #[test]
    fn asset_instance_id_rejects_input_without_asset_separator() {
        let err = AssetInstanceId::new("eip155:1/native").unwrap_err();
        assert!(matches!(err, IdError::Caip(_)));
    }

    #[test]
    fn asset_instance_id_exposes_typed_segments() {
        let id = AssetInstanceId::new("eip155:1/erc20:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")
            .unwrap();
        assert_eq!(id.network_id().as_str(), "eip155:1");
        assert_eq!(id.asset_namespace(), "erc20");
        assert_eq!(
            id.asset_reference(),
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        );
    }

    #[test]
    fn asset_instance_id_caip19_round_trip_is_lossless() {
        let s = "eip155:8453/native:eth";
        let id = AssetInstanceId::new(s).unwrap();
        let parsed = id.caip19();
        assert_eq!(format!("{parsed}"), s);
    }

    #[test]
    fn network_id_deserialization_validates_caip2() {
        // serde_json now goes through the strict Deserialize impl.
        let valid: NetworkId = serde_json::from_str(r#""eip155:1""#).unwrap();
        assert_eq!(valid.as_str(), "eip155:1");
        let err = serde_json::from_str::<NetworkId>(r#""eip_155:1""#).unwrap_err();
        assert!(err.to_string().contains("CAIP"));
    }

    #[test]
    fn asset_instance_id_deserialization_validates_caip19() {
        let valid: AssetInstanceId = serde_json::from_str(r#""eip155:1/native:eth""#).unwrap();
        assert_eq!(valid.as_str(), "eip155:1/native:eth");
        let err = serde_json::from_str::<AssetInstanceId>(r#""noslash""#).unwrap_err();
        assert!(err.to_string().contains("CAIP"));
    }
}
