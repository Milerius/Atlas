//! Property tests for atlas-core boundary invariants.
//!
//! Every property is a hand-written rule that the boundary must obey. Bolero
//! generates input across the type's `Arbitrary` space and asserts the rule
//! holds. Failures replay deterministically.
//!
//! Run with `cargo test -p atlas-verify`.

use atlas_core::amount::{AmountError, RawAmount};
use atlas_core::asset::{AssetCapability, AssetInstance, AssetMetadata, AssetStandard};
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
///   Native + None         → Ok
///   Native + Some(_)      → Err
///   Erc20  + Some(s)      → Ok iff `s.trim()` is non-empty
///   Erc20  + None         → Err
#[test]
fn validate_shape_partition_is_total() {
    check!().with_type::<(bool, Option<String>, u8)>().for_each(
        |(is_native, contract, decimals): &(bool, Option<String>, u8)| {
            let standard = if *is_native {
                AssetStandard::Native
            } else {
                AssetStandard::Erc20
            };
            let instance = AssetInstance {
                id: AssetInstanceId::new("eip155:1/test").unwrap(),
                instrument_id: AssetInstrumentId::new("test").unwrap(),
                network: NetworkId::new("eip155:1").unwrap(),
                standard,
                decimals: *decimals,
                contract: contract.clone(),
                capabilities: vec![AssetCapability::Balance],
                metadata: AssetMetadata::default(),
            };
            let result = instance.validate_shape();
            match (is_native, contract.as_deref()) {
                (true, None) => assert!(result.is_ok()),
                (true, Some(_)) => assert!(result.is_err()),
                (false, Some(s)) if !s.trim().is_empty() => assert!(result.is_ok()),
                (false, _) => assert!(result.is_err()),
            }
        },
    );
}
