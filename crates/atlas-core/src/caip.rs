//! CAIP-2 / CAIP-19 typed parsers.
//!
//! Atlas's chain and asset ids follow the [CAIP] family of specs:
//!
//! - **CAIP-2** — chain id: `<namespace>:<reference>` (e.g.
//!   `eip155:1`, `solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp`).
//! - **CAIP-19** — asset id: `<chain_id>/<asset_namespace>:<asset_reference>`
//!   (e.g. `eip155:8453/native:eth`,
//!   `eip155:1/erc20:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`).
//!
//! Atlas constructs these strings everywhere — in [`crate::registry`]
//! documents, in [`crate::transaction::TransferIntent`] payloads, in the
//! official registry JSON, in test fixtures. Until now, the only check
//! applied was "non-empty"; a typo like `eip-155:1` or `:1` would slip
//! through to the chain service. Strict parsers at the typed-id boundary
//! catch malformed input the moment the data is constructed or
//! deserialised.
//!
//! Both parsers also expose structured accessors so callers don't have to
//! re-split a CAIP string at every read site.
//!
//! [CAIP]: https://chainagnostic.org/caips/

/// Validation rules per [CAIP-2].
///
/// [CAIP-2]: https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md
mod caip2_rules {
    pub const NAMESPACE_MIN: usize = 3;
    pub const NAMESPACE_MAX: usize = 8;
    pub const REFERENCE_MIN: usize = 1;
    pub const REFERENCE_MAX: usize = 128;

    pub fn is_namespace_char(c: char) -> bool {
        c.is_ascii_lowercase() || c.is_ascii_digit()
    }

    pub fn is_reference_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '-' || c == '_'
    }
}

/// Validation rules per [CAIP-19].
///
/// [CAIP-19]: https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-19.md
mod caip19_rules {
    pub const NAMESPACE_MIN: usize = 3;
    pub const NAMESPACE_MAX: usize = 8;
    pub const REFERENCE_MIN: usize = 1;
    pub const REFERENCE_MAX: usize = 128;

    pub fn is_namespace_char(c: char) -> bool {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
    }

    pub fn is_reference_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '%'
    }
}

/// Parsed CAIP-2 chain id: a `(namespace, reference)` decomposition with
/// each segment validated against the [CAIP-2] rules.
///
/// Build via [`Caip2::parse`] or borrow from a [`crate::id::NetworkId`]
/// via [`crate::id::NetworkId::caip2`]. Display gives back the canonical
/// `namespace:reference` form so the round-trip is lossless.
///
/// [CAIP-2]: https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Caip2 {
    namespace: String,
    reference: String,
}

impl Caip2 {
    /// Parse a CAIP-2 string like `"eip155:1"` or
    /// `"solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"`.
    pub fn parse(s: &str) -> Result<Self, CaipError> {
        let (namespace, reference) = s
            .split_once(':')
            .ok_or_else(|| CaipError::Caip2MissingSeparator(s.to_string()))?;

        validate_segment(
            namespace,
            caip2_rules::NAMESPACE_MIN,
            caip2_rules::NAMESPACE_MAX,
            caip2_rules::is_namespace_char,
            CaipSegment::Caip2Namespace,
        )?;
        validate_segment(
            reference,
            caip2_rules::REFERENCE_MIN,
            caip2_rules::REFERENCE_MAX,
            caip2_rules::is_reference_char,
            CaipSegment::Caip2Reference,
        )?;

        Ok(Self {
            namespace: namespace.to_string(),
            reference: reference.to_string(),
        })
    }

    /// CAIP-2 namespace — `"eip155"`, `"solana"`, etc.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// CAIP-2 reference — chain-specific (`"1"` for Ethereum mainnet,
    /// `"5eykt4Us…vdp"` for Solana mainnet).
    pub fn reference(&self) -> &str {
        &self.reference
    }
}

impl std::fmt::Display for Caip2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace, self.reference)
    }
}

/// Parsed CAIP-19 asset id: the underlying [`Caip2`] chain plus
/// `(asset_namespace, asset_reference)`.
///
/// Build via [`Caip19::parse`] or borrow from an [`crate::id::AssetInstanceId`]
/// via [`crate::id::AssetInstanceId::caip19`]. Display gives back the
/// canonical `chain_id/asset_namespace:asset_reference` form.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Caip19 {
    network: Caip2,
    asset_namespace: String,
    asset_reference: String,
}

impl Caip19 {
    /// Parse a CAIP-19 string like `"eip155:8453/native:eth"` or
    /// `"eip155:1/erc20:0xA0b8…eB48"`.
    pub fn parse(s: &str) -> Result<Self, CaipError> {
        let (chain_part, asset_part) = s
            .split_once('/')
            .ok_or_else(|| CaipError::Caip19MissingSeparator(s.to_string()))?;
        let network = Caip2::parse(chain_part)?;
        let (asset_namespace, asset_reference) = asset_part
            .split_once(':')
            .ok_or_else(|| CaipError::Caip19MissingAssetSeparator(s.to_string()))?;

        validate_segment(
            asset_namespace,
            caip19_rules::NAMESPACE_MIN,
            caip19_rules::NAMESPACE_MAX,
            caip19_rules::is_namespace_char,
            CaipSegment::Caip19AssetNamespace,
        )?;
        validate_segment(
            asset_reference,
            caip19_rules::REFERENCE_MIN,
            caip19_rules::REFERENCE_MAX,
            caip19_rules::is_reference_char,
            CaipSegment::Caip19AssetReference,
        )?;

        Ok(Self {
            network,
            asset_namespace: asset_namespace.to_string(),
            asset_reference: asset_reference.to_string(),
        })
    }

    /// Underlying CAIP-2 chain id.
    pub fn network(&self) -> &Caip2 {
        &self.network
    }

    /// CAIP-19 asset namespace — `"native"`, `"erc20"`, `"spl"`.
    pub fn asset_namespace(&self) -> &str {
        &self.asset_namespace
    }

    /// CAIP-19 asset reference — token symbol, contract address, mint, etc.
    pub fn asset_reference(&self) -> &str {
        &self.asset_reference
    }
}

impl std::fmt::Display for Caip19 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}:{}",
            self.network, self.asset_namespace, self.asset_reference
        )
    }
}

/// Source-of-error tag carried by [`CaipError::SegmentInvalid`]. Names the
/// segment that failed validation so messages and tests can disambiguate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaipSegment {
    Caip2Namespace,
    Caip2Reference,
    Caip19AssetNamespace,
    Caip19AssetReference,
}

impl std::fmt::Display for CaipSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Caip2Namespace => "CAIP-2 namespace",
            Self::Caip2Reference => "CAIP-2 reference",
            Self::Caip19AssetNamespace => "CAIP-19 asset namespace",
            Self::Caip19AssetReference => "CAIP-19 asset reference",
        };
        f.write_str(label)
    }
}

/// Errors raised by [`Caip2::parse`] / [`Caip19::parse`].
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CaipError {
    /// CAIP-2 input has no `:` separator.
    #[error("CAIP-2 input missing ':' separator: {0:?}")]
    Caip2MissingSeparator(String),
    /// CAIP-19 input has no `/` separator between chain id and asset
    /// segment.
    #[error("CAIP-19 input missing '/' separator: {0:?}")]
    Caip19MissingSeparator(String),
    /// CAIP-19 asset segment has no `:` separator between asset
    /// namespace and asset reference.
    #[error("CAIP-19 asset segment missing ':' separator: {0:?}")]
    Caip19MissingAssetSeparator(String),
    /// A segment violated length or character-class rules.
    #[error(
        "{segment}: input {input:?} fails validation \
         (allowed length {min}..={max}, allowed character class)"
    )]
    SegmentInvalid {
        segment: CaipSegment,
        input: String,
        min: usize,
        max: usize,
    },
}

fn validate_segment(
    s: &str,
    min: usize,
    max: usize,
    is_valid_char: impl Fn(char) -> bool,
    segment: CaipSegment,
) -> Result<(), CaipError> {
    if s.len() < min || s.len() > max || !s.chars().all(is_valid_char) {
        return Err(CaipError::SegmentInvalid {
            segment,
            input: s.to_string(),
            min,
            max,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Caip2 ─────────────────────────────────────────────────────────────

    #[test]
    fn caip2_parses_eip155_mainnet() {
        let c = Caip2::parse("eip155:1").unwrap();
        assert_eq!(c.namespace(), "eip155");
        assert_eq!(c.reference(), "1");
        assert_eq!(format!("{c}"), "eip155:1");
    }

    #[test]
    fn caip2_parses_solana_mainnet() {
        let c = Caip2::parse("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp").unwrap();
        assert_eq!(c.namespace(), "solana");
        assert_eq!(c.reference(), "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp");
    }

    #[test]
    fn caip2_accepts_reference_with_dash_and_underscore() {
        // Reference charset is [-_a-zA-Z0-9]; both `-` and `_` need
        // to flow through `is_reference_char` so the short-circuit
        // `||` branches are exercised.
        let c = Caip2::parse("cosmos:cosmoshub-4").unwrap();
        assert_eq!(c.reference(), "cosmoshub-4");
        let c2 = Caip2::parse("foo:bar_baz").unwrap();
        assert_eq!(c2.reference(), "bar_baz");
    }

    #[test]
    fn caip2_rejects_input_without_separator() {
        assert!(matches!(
            Caip2::parse("eip1551"),
            Err(CaipError::Caip2MissingSeparator(_))
        ));
    }

    #[test]
    fn caip2_rejects_namespace_too_short() {
        // Min is 3 chars
        assert!(matches!(
            Caip2::parse("ab:1"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Namespace,
                ..
            })
        ));
    }

    #[test]
    fn caip2_rejects_namespace_too_long() {
        // Max is 8 chars
        assert!(matches!(
            Caip2::parse("abcdefghi:1"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Namespace,
                ..
            })
        ));
    }

    #[test]
    fn caip2_rejects_namespace_with_uppercase() {
        assert!(matches!(
            Caip2::parse("EIP155:1"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Namespace,
                ..
            })
        ));
    }

    #[test]
    fn caip2_rejects_namespace_with_dash() {
        assert!(matches!(
            Caip2::parse("eip-155:1"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Namespace,
                ..
            })
        ));
    }

    #[test]
    fn caip2_rejects_empty_reference() {
        assert!(matches!(
            Caip2::parse("eip155:"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Reference,
                ..
            })
        ));
    }

    #[test]
    fn caip2_rejects_reference_too_long() {
        let long_ref = "a".repeat(129);
        let input = format!("eip155:{long_ref}");
        assert!(matches!(
            Caip2::parse(&input),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Reference,
                ..
            })
        ));
    }

    #[test]
    fn caip2_accepts_reference_at_max_length() {
        let max_ref = "a".repeat(128);
        let input = format!("eip155:{max_ref}");
        assert!(Caip2::parse(&input).is_ok());
    }

    #[test]
    fn caip2_rejects_reference_with_disallowed_char() {
        // CAIP-2 reference disallows `/`. (`:` would be the separator,
        // so test `/` which survives split_once.)
        assert!(matches!(
            Caip2::parse("eip155:1/2"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Reference,
                ..
            })
        ));
    }

    // ── Caip19 ────────────────────────────────────────────────────────────

    #[test]
    fn caip19_parses_native_asset() {
        let c = Caip19::parse("eip155:8453/native:eth").unwrap();
        assert_eq!(c.network().namespace(), "eip155");
        assert_eq!(c.network().reference(), "8453");
        assert_eq!(c.asset_namespace(), "native");
        assert_eq!(c.asset_reference(), "eth");
        assert_eq!(format!("{c}"), "eip155:8453/native:eth");
    }

    #[test]
    fn caip19_parses_erc20_with_hex_contract() {
        let c = Caip19::parse("eip155:1/erc20:0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap();
        assert_eq!(c.asset_namespace(), "erc20");
        assert_eq!(
            c.asset_reference(),
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        );
    }

    #[test]
    fn caip19_parses_solana_spl_with_base58_mint() {
        let c =
            Caip19::parse("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp/spl:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")
                .unwrap();
        assert_eq!(c.asset_namespace(), "spl");
    }

    #[test]
    fn caip19_accepts_asset_namespace_with_dash() {
        // Asset namespace charset is [-a-z0-9]; `-` needs an explicit
        // input so the short-circuit branch in `is_namespace_char`
        // gets exercised. Length max is 8, so we use `n-1` (3 chars).
        let c = Caip19::parse("eip155:1/n-1:eth").unwrap();
        assert_eq!(c.asset_namespace(), "n-1");
    }

    #[test]
    fn caip19_accepts_asset_reference_with_dash_dot_and_percent() {
        // Asset reference charset is [-.%a-zA-Z0-9]; exercise each of
        // the three non-alphanumeric branches in `is_reference_char`.
        let c1 = Caip19::parse("eip155:1/erc721:0xabc-1").unwrap();
        assert_eq!(c1.asset_reference(), "0xabc-1");
        let c2 = Caip19::parse("eip155:1/erc721:0xabc.1").unwrap();
        assert_eq!(c2.asset_reference(), "0xabc.1");
        let c3 = Caip19::parse("eip155:1/erc721:0xabc%201").unwrap();
        assert_eq!(c3.asset_reference(), "0xabc%201");
    }

    #[test]
    fn caip19_rejects_input_without_slash() {
        assert!(matches!(
            Caip19::parse("eip155:1"),
            Err(CaipError::Caip19MissingSeparator(_))
        ));
    }

    #[test]
    fn caip19_rejects_asset_segment_without_colon() {
        assert!(matches!(
            Caip19::parse("eip155:1/eth"),
            Err(CaipError::Caip19MissingAssetSeparator(_))
        ));
    }

    #[test]
    fn caip19_propagates_chain_segment_errors() {
        // Bad chain segment surfaces as a CAIP-2 error, not a CAIP-19
        // structural error — so callers can distinguish "bad chain"
        // from "bad asset".
        assert!(matches!(
            Caip19::parse("eip-155:1/native:eth"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip2Namespace,
                ..
            })
        ));
    }

    #[test]
    fn caip19_rejects_asset_namespace_with_uppercase() {
        // CAIP-19 asset namespace charset is [-a-z0-9] only
        assert!(matches!(
            Caip19::parse("eip155:1/ERC20:0x123"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip19AssetNamespace,
                ..
            })
        ));
    }

    #[test]
    fn caip19_rejects_empty_asset_reference() {
        assert!(matches!(
            Caip19::parse("eip155:1/native:"),
            Err(CaipError::SegmentInvalid {
                segment: CaipSegment::Caip19AssetReference,
                ..
            })
        ));
    }

    #[test]
    fn caip_segment_displays_human_label() {
        assert_eq!(
            format!("{}", CaipSegment::Caip2Namespace),
            "CAIP-2 namespace"
        );
        assert_eq!(
            format!("{}", CaipSegment::Caip2Reference),
            "CAIP-2 reference"
        );
        assert_eq!(
            format!("{}", CaipSegment::Caip19AssetNamespace),
            "CAIP-19 asset namespace"
        );
        assert_eq!(
            format!("{}", CaipSegment::Caip19AssetReference),
            "CAIP-19 asset reference"
        );
    }

    #[test]
    fn caip_error_display_includes_input() {
        let err = Caip2::parse("nope").unwrap_err();
        assert!(format!("{err}").contains("nope"));
    }
}
