use domain::structure::{canonical_identity, CanonicalIdentityError};
use serde_json::json;
use sha2::{Digest, Sha256};

#[test]
fn exact_u64_uses_the_specified_integer_representation() {
    let domain = "test-identity-v1";
    let payload =
        br#"{"type":"object","value":{"size":{"type":"integer","value":"18446744073709551615"}}}"#;
    let mut expected = Sha256::new();
    expected.update((domain.len() as u64).to_le_bytes());
    expected.update(domain.as_bytes());
    expected.update((payload.len() as u64).to_le_bytes());
    expected.update(payload);
    assert_eq!(
        canonical_identity(domain, &json!({"size": u64::MAX})).unwrap(),
        format!("{domain}-{}", hex::encode(expected.finalize()))
    );
    assert_ne!(
        canonical_identity(domain, &json!({"size": u64::MAX})).unwrap(),
        canonical_identity(domain, &json!({"size": u64::MAX - 1})).unwrap()
    );
}

#[test]
fn different_value_types_and_domains_never_share_an_identity() {
    let values = [
        json!(1),
        json!("1"),
        json!([1]),
        json!({"0": 1}),
        json!(true),
    ];
    let identities = values
        .iter()
        .map(|value| canonical_identity("typed-v1", value).unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identities.len(), values.len());
    assert_ne!(
        canonical_identity("view-v1", &values[0]).unwrap(),
        canonical_identity("work-v1", &values[0]).unwrap()
    );
}

#[test]
fn object_order_is_canonical_but_sequences_keep_their_native_order() {
    let forward = [("first", 1_u64), ("second", u64::MAX)]
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    let backward = [("second", u64::MAX), ("first", 1_u64)]
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        canonical_identity("map-v1", &forward).unwrap(),
        canonical_identity("map-v1", &backward).unwrap()
    );
    assert_ne!(
        canonical_identity("sequence-v1", &[1_u64, 2]).unwrap(),
        canonical_identity("sequence-v1", &[2_u64, 1]).unwrap()
    );
}

#[test]
fn floating_point_values_are_rejected_including_integral_values() {
    for value in [json!(1.0), json!({"nested": [0.25]})] {
        assert!(matches!(
            canonical_identity("identity-v1", &value),
            Err(CanonicalIdentityError::FloatingPoint)
        ));
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            canonical_identity("identity-v1", &value),
            Err(CanonicalIdentityError::FloatingPoint)
        ));
    }
    assert!(matches!(
        canonical_identity("", &json!(1)),
        Err(CanonicalIdentityError::EmptyDomain)
    ));
}

#[test]
fn identity_inputs_have_explicit_depth_and_value_limits() {
    let mut deeply_nested = json!(null);
    for _ in 0..66 {
        deeply_nested = json!([deeply_nested]);
    }
    assert!(matches!(
        canonical_identity("depth-v1", &deeply_nested),
        Err(CanonicalIdentityError::StructuralLimit)
    ));
    assert!(matches!(
        canonical_identity("values-v1", &vec![0_u64; 65_536]),
        Err(CanonicalIdentityError::StructuralLimit)
    ));
}
