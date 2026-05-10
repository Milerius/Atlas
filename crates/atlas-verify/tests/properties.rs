//! Property tests for atlas-core boundary invariants.
//!
//! Every property is a hand-written rule that the boundary must obey. Bolero
//! generates input across the type's `Arbitrary` space and asserts the rule
//! holds. Failures replay deterministically.
//!
//! Run with `cargo test -p atlas-verify`.

use atlas_core::amount::{AmountError, RawAmount};
use atlas_core::asset::{AssetCapability, AssetInstance, AssetMetadata, AssetStandard};
use atlas_core::caip::{Caip19, Caip2};
use atlas_core::id::{AssetInstanceId, AssetInstrumentId, Id, NetworkId};
use bolero::check;
use num_bigint::BigInt;

// ── Id ─────────────────────────────────────────────────────────────────────

/// `Id::new` accepts iff the trimmed input is non-empty.
#[test]
fn id_new_validity_matches_trimmed_emptiness() {
    check!().with_type::<String>().for_each(|s: &String| {
        let trimmed_empty = s.trim().is_empty();
        match Id::new(s.clone()) {
            Ok(_) => assert!(!trimmed_empty),
            Err(_) => assert!(trimmed_empty),
        }
    });
}

/// Id preserves the input verbatim (no trimming, no normalization) and the
/// `Display` impl round-trips through `as_str`.
#[test]
fn id_round_trips_through_display() {
    check!().with_type::<String>().for_each(|s: &String| {
        if let Ok(id) = Id::new(s.clone()) {
            assert_eq!(id.as_str(), s);
            assert_eq!(format!("{id}"), *s);
        }
    });
}

// ── RawAmount ──────────────────────────────────────────────────────────────

/// `RawAmount::new` accepts iff the value is non-negative; the constructor
/// preserves both value and decimals exactly.
#[test]
fn raw_amount_sign_check_matches_bigint_sign() {
    check!()
        .with_type::<(i128, u8)>()
        .for_each(|(value, decimals): &(i128, u8)| {
            let big = BigInt::from(*value);
            match RawAmount::new(big, *decimals) {
                Ok(amount) => {
                    assert!(*value >= 0);
                    assert_eq!(amount.value().to_string(), value.to_string());
                    assert_eq!(amount.decimals(), *decimals);
                }
                Err(AmountError::NegativeValue) => assert!(*value < 0),
                Err(other) => panic!("unexpected error: {other}"),
            }
        });
}

/// Serde round-trip preserves both fields.
#[test]
fn raw_amount_serde_round_trips() {
    check!()
        .with_type::<(u64, u8)>()
        .for_each(|(value, decimals): &(u64, u8)| {
            let amount = RawAmount::new(BigInt::from(*value), *decimals).unwrap();
            let json = serde_json::to_string(&amount).unwrap();
            let decoded: RawAmount = serde_json::from_str(&json).unwrap();
            assert_eq!(amount, decoded);
        });
}

/// `checked_add` succeeds iff decimals match; the result preserves the
/// shared decimals and equals the BigInt sum of the operands.
#[test]
fn raw_amount_checked_add_obeys_decimals_invariant() {
    check!()
        .with_type::<(u64, u64, u8, u8)>()
        .for_each(|(a, b, da, db): &(u64, u64, u8, u8)| {
            let amount_a = RawAmount::new(BigInt::from(*a), *da).unwrap();
            let amount_b = RawAmount::new(BigInt::from(*b), *db).unwrap();
            match amount_a.checked_add(&amount_b) {
                Ok(sum) => {
                    assert_eq!(*da, *db);
                    assert_eq!(sum.decimals(), *da);
                    assert_eq!(sum.value(), &(BigInt::from(*a) + BigInt::from(*b)));
                }
                Err(AmountError::DecimalsMismatch { left, right }) => {
                    assert_ne!(*da, *db);
                    assert_eq!(left, *da);
                    assert_eq!(right, *db);
                }
                Err(other) => panic!("unexpected error: {other}"),
            }
        });
}

// ── AssetInstance::validate_shape ──────────────────────────────────────────

/// `validate_shape` partitions `(Standard, Option<contract>)` exactly:
///   Native + None              → Ok
///   Native + Some(_)           → Err
///   Erc20  + Some(non-empty)   → Ok
///   Erc20  + None|Some(empty)  → Err
///   Spl    + Some(non-empty)   → Ok
///   Spl    + None|Some(empty)  → Err
#[test]
fn validate_shape_partition_is_total() {
    check!().with_type::<(u8, Option<String>, u8)>().for_each(
        |(standard_idx, contract, decimals): &(u8, Option<String>, u8)| {
            let standard_kind = standard_idx % 3;
            let standard = match standard_kind {
                0 => AssetStandard::Native,
                1 => AssetStandard::Erc20,
                _ => AssetStandard::Spl,
            };
            // The asset id must be CAIP-19-shaped; the property under
            // test exercises `validate_shape`, not id parsing, so we
            // reuse a static valid id and let `standard` / `contract`
            // vary across iterations.
            let instance = AssetInstance {
                id: AssetInstanceId::new("eip155:1/native:eth").unwrap(),
                instrument_id: AssetInstrumentId::new("test").unwrap(),
                network: NetworkId::new("eip155:1").unwrap(),
                standard,
                decimals: *decimals,
                contract: contract.clone(),
                capabilities: vec![AssetCapability::Balance],
                metadata: AssetMetadata::default(),
            };
            let result = instance.validate_shape();
            match (standard_kind, contract.as_deref()) {
                (0, None) => assert!(result.is_ok()),
                (0, Some(_)) => assert!(result.is_err()),
                (1, Some(s)) if !s.trim().is_empty() => assert!(result.is_ok()),
                (1, _) => assert!(result.is_err()),
                (2, Some(s)) if !s.trim().is_empty() => assert!(result.is_ok()),
                (2, _) => assert!(result.is_err()),
                _ => unreachable!("standard_kind clamped to 0..3"),
            }
        },
    );
}

// ── CAIP parsers ───────────────────────────────────────────────────────────

/// `Caip2` parses iff the input matches `<namespace>:<reference>` with both
/// segments respecting the CAIP-2 character classes and length bounds. When
/// it parses, `Display` round-trips back to the original input verbatim.
#[test]
fn caip2_round_trips_through_display() {
    check!().with_type::<String>().for_each(|s: &String| {
        if let Ok(c) = Caip2::parse(s) {
            assert_eq!(format!("{c}"), *s);
        }
    });
}

/// `Caip19` parses iff the chain segment is a valid Caip2 *and* the asset
/// segment matches `<asset_namespace>:<asset_reference>`. When it parses,
/// the typed accessors agree with the Display round-trip.
#[test]
fn caip19_round_trips_through_display() {
    check!().with_type::<String>().for_each(|s: &String| {
        if let Ok(c) = Caip19::parse(s) {
            // Display must reconstruct the canonical form
            let display = format!("{c}");
            assert_eq!(
                display,
                format!(
                    "{}/{}:{}",
                    c.network(),
                    c.asset_namespace(),
                    c.asset_reference()
                )
            );
            // Any string that parses must contain the separators in order
            assert!(display.contains(':'));
            assert!(display.contains('/'));
        }
    });
}

/// Strict construction agrees: a string parses as `Caip2` iff
/// `NetworkId::new` accepts it.
#[test]
fn caip2_parser_agrees_with_network_id_constructor() {
    check!().with_type::<String>().for_each(|s: &String| {
        let caip_ok = Caip2::parse(s).is_ok();
        let id_ok = NetworkId::new(s.clone()).is_ok();
        assert_eq!(caip_ok, id_ok);
    });
}

/// Strict construction agrees: a string parses as `Caip19` iff
/// `AssetInstanceId::new` accepts it.
#[test]
fn caip19_parser_agrees_with_asset_instance_id_constructor() {
    check!().with_type::<String>().for_each(|s: &String| {
        let caip_ok = Caip19::parse(s).is_ok();
        let id_ok = AssetInstanceId::new(s.clone()).is_ok();
        assert_eq!(caip_ok, id_ok);
    });
}
