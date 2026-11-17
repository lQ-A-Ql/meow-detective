use crate::stats::chi_square::{chi_square_p, chi_square_score};
use crate::stats::fusion::{assess, band_for, fused_score};
use crate::stats::gamma::regularized_gamma_q;
use crate::stats::rs::RsCounts;
use crate::stats::spa::TraceCounts;
use crate::types::{StegBand, StegMethod, StegMethodReport};

fn method(method: StegMethod, score: Option<f64>) -> StegMethodReport {
    StegMethodReport {
        method,
        estimate: score.map(|value| value / 100.0),
        score,
        samples: 10_000,
    }
}

#[test]
fn gamma_matches_known_values() {
    let cases = [
        (1.0, 0.5, 0.606_530_659_712_633_4),
        (0.5, 1.0, 0.157_299_207_050_285_13),
        (1.0, 2.995_732_273_553_991, 0.05),
        (5.0, 9.153_519_026_637_573, 0.05),
    ];
    for (a, x, expected) in cases {
        let actual = regularized_gamma_q(a, x);
        assert!(
            (actual - expected).abs() < 1e-6,
            "Q({a}, {x}) = {actual}, expected {expected}"
        );
    }
}

#[test]
fn chi_square_reports_certainty_for_balanced_pairs() {
    let histogram = [10u32; 256];
    let (p_value, degrees) = chi_square_p(&histogram).expect("balanced histogram");
    assert_eq!(degrees, 127);
    assert!((p_value - 1.0).abs() < 1e-9);
    assert!((chi_square_score(p_value) - 100.0).abs() < 1e-6);
}

#[test]
fn chi_square_requires_two_non_empty_pairs() {
    let mut histogram = [0u32; 256];
    histogram[0] = 5;
    histogram[1] = 5;
    assert!(chi_square_p(&histogram).is_none());
}

#[test]
fn chi_square_score_uses_two_p_minus_one() {
    assert!((chi_square_score(0.5) - 0.0).abs() < 1e-9);
    assert!((chi_square_score(0.75) - 50.0).abs() < 1e-9);
    assert!((chi_square_score(1.0) - 100.0).abs() < 1e-9);
    assert!((chi_square_score(0.0) - 0.0).abs() < 1e-9);
}

#[test]
fn rs_classifies_masked_groups() {
    let mut counts = RsCounts::default();
    counts.add_group(&[0, 255, 0, 255]);
    assert_eq!(counts.groups, 1);
    assert_eq!((counts.r_m, counts.s_m), (0, 1));
    assert_eq!((counts.r_minus_m, counts.s_minus_m), (0, 0));
    assert_eq!((counts.r_m_flipped, counts.s_m_flipped), (1, 0));
    assert_eq!((counts.r_minus_m_flipped, counts.s_minus_m_flipped), (0, 1));

    let mut negative = RsCounts::default();
    negative.add_group(&[1, 255, 1, 255]);
    assert_eq!((negative.r_m, negative.s_m), (1, 0));
    assert_eq!((negative.r_minus_m, negative.s_minus_m), (0, 1));
}

#[test]
fn rs_requires_minimum_groups() {
    let mut counts = RsCounts::default();
    for _ in 0..255 {
        counts.add_group(&[0, 255, 0, 255]);
    }
    assert!(counts.estimate(256).is_none());
    counts.add_group(&[0, 255, 0, 255]);
    assert_eq!(counts.groups, 256);
    assert!(counts.estimate(257).is_none());
}

#[test]
fn rs_message_length_uses_smallest_root() {
    let counts = RsCounts {
        r_m: 404,
        s_m: 0,
        r_m_flipped: 476,
        s_m_flipped: 0,
        r_minus_m: 476,
        s_minus_m: 0,
        r_minus_m_flipped: 404,
        s_minus_m_flipped: 0,
        groups: 1000,
    };
    let rate = counts.estimate(256).expect("smallest root is a valid rate");
    assert!((rate - 0.0785).abs() < 1e-3, "rate {rate}");
}

#[test]
fn rs_rejects_degenerate_quadratics() {
    let zero = RsCounts {
        groups: 1000,
        ..RsCounts::default()
    };
    assert!(zero.estimate(256).is_none());

    let positive = RsCounts {
        r_m: 500,
        s_m: 0,
        r_m_flipped: 100,
        s_m_flipped: 0,
        r_minus_m: 100,
        s_minus_m: 0,
        r_minus_m_flipped: 50,
        s_minus_m_flipped: 0,
        groups: 1000,
    };
    assert!(positive.estimate(256).is_none());
}

#[test]
fn spa_classifies_trace_sets() {
    let mut counts = TraceCounts::default();
    counts.add_pair(0, 0);
    counts.add_pair(1, 0);
    counts.add_pair(0, 1);
    counts.add_pair(1, 1);
    counts.add_pair(255, 255);
    counts.add_pair(0, 20);
    assert_eq!(counts.pairs, 5);
    assert_eq!(counts.e_even[5], 1);
    assert_eq!(counts.o_odd[5], 1);
    assert_eq!(counts.e_odd[5], 1);
    assert_eq!(counts.o_even[5], 2);
}

#[test]
fn spa_requires_minimum_pairs() {
    let mut counts = TraceCounts::default();
    for _ in 0..511 {
        counts.add_pair(0, 0);
    }
    assert!(counts.estimate(512).is_none());
}

#[test]
fn fusion_requires_two_methods() {
    let single = [method(StegMethod::ChiSquare, Some(90.0))];
    assert!(fused_score(&single).is_none());
    let assessment = assess(&single);
    assert_eq!(assessment.band, StegBand::Inconclusive);
    assert_eq!(assessment.methods_used, 1);
    assert!(assessment.score.is_none());
}

#[test]
fn fusion_renormalizes_available_weights() {
    let methods = [
        method(StegMethod::ChiSquare, Some(40.0)),
        method(StegMethod::Spa, Some(0.0)),
    ];
    let score = fused_score(&methods).expect("two methods");
    let expected = (0.35 * 40.0) / (0.35 + 0.30);
    assert!((score - expected).abs() < 1e-9);

    let all = [
        method(StegMethod::ChiSquare, Some(100.0)),
        method(StegMethod::Rs, Some(100.0)),
        method(StegMethod::Spa, Some(100.0)),
    ];
    assert!((fused_score(&all).expect("three methods") - 100.0).abs() < 1e-9);

    let missing = [
        method(StegMethod::ChiSquare, None),
        method(StegMethod::Rs, Some(60.0)),
    ];
    assert!(fused_score(&missing).is_none());
}

#[test]
fn bands_follow_thresholds() {
    assert_eq!(band_for(None), StegBand::Inconclusive);
    assert_eq!(band_for(Some(0.0)), StegBand::Low);
    assert_eq!(band_for(Some(9.999)), StegBand::Low);
    assert_eq!(band_for(Some(10.0)), StegBand::Medium);
    assert_eq!(band_for(Some(29.999)), StegBand::Medium);
    assert_eq!(band_for(Some(30.0)), StegBand::High);
}
