//! Per-chain address-format validation.
//!
//! [`crate::id::AddressRef`] is intentionally chain-agnostic at
//! construction (it only rejects empty / whitespace) so the same type
//! can flow through layers — registry loaders, transfer intents,
//! signers — without forcing every layer to know which chain owns the
//! value. This module adds the chain-aware *validation* surface that
//! callers invoke at the boundaries that *do* know the chain:
//!
//! - [`AddressRef::validate_for`] — post-construction check against an
//!   [`AddressFormat`].
//! - [`AddressRef::for_format`] — strict constructor that runs
//!   construction + validation in one shot.
//!
//! Per chain family:
//!
//! - **EVM** ([`AddressFormat::EvmAddress`]) — 20-byte hex string with
//!   `0x` prefix. EIP-55 mixed-case checksum is *accepted but not
//!   required*: a lowercase address validates, an EIP-55-checksummed
//!   address validates, and a mismatched-case address (some chars in
//!   the wrong case relative to EIP-55) is rejected. Validation goes
//!   through `alloy_primitives::Address::from_str`, which encodes
//!   exactly that contract.
//! - **Solana** ([`AddressFormat::SolanaPubkey`]) — 32-byte ed25519
//!   public key, base58-encoded. We reject anything that doesn't
//!   base58-decode to exactly 32 bytes.

use crate::chain::AddressFormat;
use crate::id::{AddressRef, IdError};
use std::str::FromStr;

/// Errors raised by [`AddressRef`] format validation.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AddressError {
    /// Underlying string failed [`AddressRef`] construction (empty /
    /// whitespace).
    #[error("address id construction failed: {0}")]
    Id(#[from] IdError),
    /// Input was not valid for the target [`AddressFormat`].
    #[error("address {input:?} is not valid for {format:?}: {reason}")]
    InvalidFormat {
        format: AddressFormat,
        input: String,
        reason: String,
    },
}

impl AddressRef {
    /// Strict constructor: build an [`AddressRef`] *and* validate it
    /// against `format` in one shot. Equivalent to
    /// `AddressRef::new(s).and_then(|a| a.validate_for(format).map(|_| a))`
    /// but yields a single typed error.
    pub fn for_format(
        value: impl Into<String>,
        format: AddressFormat,
    ) -> Result<Self, AddressError> {
        let address = AddressRef::new(value)?;
        address.validate_for(&format)?;
        Ok(address)
    }

    /// Validate the underlying string against `format`. Idempotent and
    /// pure — does not mutate the value.
    pub fn validate_for(&self, format: &AddressFormat) -> Result<(), AddressError> {
        match format {
            AddressFormat::EvmAddress => validate_evm(self.as_str(), format),
            AddressFormat::SolanaPubkey => validate_solana(self.as_str(), format),
        }
    }
}

fn validate_evm(s: &str, format: &AddressFormat) -> Result<(), AddressError> {
    // Atlas convention: EVM addresses must carry the `0x` prefix and
    // be exactly 20 hex bytes (42 characters total). alloy's
    // `Address::from_str` is permissive about both — it accepts
    // prefix-less input and silently parses without validating the
    // EIP-55 checksum on mixed-case input — so we layer the strict
    // checks on top.
    let invalid = |reason: String| AddressError::InvalidFormat {
        format: format.clone(),
        input: s.to_string(),
        reason,
    };

    let hex = s
        .strip_prefix("0x")
        .ok_or_else(|| invalid("missing '0x' prefix".to_string()))?;
    if hex.len() != 40 {
        return Err(invalid(format!(
            "expected 40 hex chars after '0x', got {}",
            hex.len()
        )));
    }
    // Catches non-hex input (`0xZZ…`) and any other parse failure.
    alloy_primitives::Address::from_str(s).map_err(|e| invalid(e.to_string()))?;

    // EIP-55: if the input is mixed-case, the case pattern itself
    // encodes a checksum and must match. All-lowercase / all-uppercase
    // inputs carry no checksum and are accepted as-is.
    let has_upper = hex.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = hex.chars().any(|c| c.is_ascii_lowercase());
    if has_upper && has_lower {
        alloy_primitives::Address::parse_checksummed(s, None)
            .map_err(|e| invalid(format!("EIP-55 checksum mismatch: {e}")))?;
    }
    Ok(())
}

fn validate_solana(s: &str, format: &AddressFormat) -> Result<(), AddressError> {
    let decoded = bs58::decode(s)
        .into_vec()
        .map_err(|e| AddressError::InvalidFormat {
            format: format.clone(),
            input: s.to_string(),
            reason: format!("base58 decode failed: {e}"),
        })?;
    if decoded.len() != 32 {
        return Err(AddressError::InvalidFormat {
            format: format.clone(),
            input: s.to_string(),
            reason: format!(
                "expected 32-byte ed25519 public key, decoded {} bytes",
                decoded.len()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EVM ───────────────────────────────────────────────────────────────

    /// Lowercase variant of the well-known USDC on Base contract.
    const EVM_LOWERCASE: &str = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913";
    /// Same address in EIP-55 mixed-case checksum form.
    const EVM_EIP55: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";

    #[test]
    fn evm_accepts_lowercase_address() {
        let a = AddressRef::for_format(EVM_LOWERCASE, AddressFormat::EvmAddress).unwrap();
        assert_eq!(a.as_str(), EVM_LOWERCASE);
    }

    #[test]
    fn evm_accepts_eip55_checksum_address() {
        let a = AddressRef::for_format(EVM_EIP55, AddressFormat::EvmAddress).unwrap();
        assert_eq!(a.as_str(), EVM_EIP55);
    }

    #[test]
    fn evm_accepts_all_uppercase_address() {
        // All-uppercase carries no EIP-55 checksum information, so
        // it's accepted as-is. Exercises the `has_upper && has_lower`
        // false-branch (no `parse_checksummed` call).
        let upper = "0x833589FCD6EDB6E08F4C7C32D4F71B54BDA02913";
        let a = AddressRef::for_format(upper, AddressFormat::EvmAddress).unwrap();
        assert_eq!(a.as_str(), upper);
    }

    #[test]
    fn evm_rejects_mismatched_checksum() {
        // Real EIP-55 form has uppercase `C` and lowercase `c`; flip
        // one to make the checksum invalid.
        let bad = "0x833589FCD6eDb6E08f4c7C32D4f71b54bdA02913";
        let err = AddressRef::for_format(bad, AddressFormat::EvmAddress).unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
        assert!(format!("{err}").contains("EIP-55"));
    }

    #[test]
    fn evm_rejects_missing_0x_prefix() {
        let err = AddressRef::for_format(
            "833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            AddressFormat::EvmAddress,
        )
        .unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
    }

    #[test]
    fn evm_rejects_wrong_length() {
        let err = AddressRef::for_format("0x1234", AddressFormat::EvmAddress).unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
    }

    #[test]
    fn evm_rejects_non_hex() {
        let err = AddressRef::for_format(
            "0xZZ3589fcd6edb6e08f4c7c32d4f71b54bda02913",
            AddressFormat::EvmAddress,
        )
        .unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
    }

    // ── Solana ────────────────────────────────────────────────────────────

    /// Solana mainnet "USDC" mint — well-known 32-byte base58 pubkey.
    const SOLANA_USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    #[test]
    fn solana_accepts_base58_pubkey_of_correct_length() {
        let a = AddressRef::for_format(SOLANA_USDC_MINT, AddressFormat::SolanaPubkey).unwrap();
        assert_eq!(a.as_str(), SOLANA_USDC_MINT);
    }

    #[test]
    fn solana_rejects_non_base58() {
        // `0` (zero) is not in the base58 alphabet
        let err = AddressRef::for_format(
            "0PjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            AddressFormat::SolanaPubkey,
        )
        .unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
    }

    #[test]
    fn solana_rejects_wrong_length() {
        // Decodes successfully but to the wrong byte count
        let err = AddressRef::for_format("abcdef", AddressFormat::SolanaPubkey).unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
    }

    // ── Strict constructor / boundary surface ────────────────────────────

    #[test]
    fn for_format_propagates_id_error_on_empty() {
        let err = AddressRef::for_format("", AddressFormat::EvmAddress).unwrap_err();
        assert!(matches!(err, AddressError::Id(_)));
    }

    #[test]
    fn validate_for_does_not_mutate_inner_string() {
        // Passing a lowercase address should leave the stored string
        // exactly as constructed; no implicit checksumming.
        let a = AddressRef::new(EVM_LOWERCASE).unwrap();
        a.validate_for(&AddressFormat::EvmAddress).unwrap();
        assert_eq!(a.as_str(), EVM_LOWERCASE);
    }

    #[test]
    fn evm_validation_rejects_solana_address() {
        // Cross-chain confusion: a Solana base58 address is not valid
        // EVM hex.
        let err = AddressRef::for_format(SOLANA_USDC_MINT, AddressFormat::EvmAddress).unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
    }

    #[test]
    fn solana_validation_rejects_evm_address() {
        // Cross-chain confusion the other way. EVM 0x-prefixed hex
        // happens to base58-decode, so the rejection comes from the
        // length check.
        let err = AddressRef::for_format(EVM_LOWERCASE, AddressFormat::SolanaPubkey).unwrap_err();
        assert!(matches!(err, AddressError::InvalidFormat { .. }));
    }
}
