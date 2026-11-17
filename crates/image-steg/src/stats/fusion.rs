use crate::types::{StegAssessment, StegBand, StegMethod, StegMethodReport};

const CHI_WEIGHT: f64 = 0.35;
const RS_WEIGHT: f64 = 0.35;
const SPA_WEIGHT: f64 = 0.30;
const MEDIUM_THRESHOLD: f64 = 10.0;
const HIGH_THRESHOLD: f64 = 30.0;

pub(crate) fn band_for(score: Option<f64>) -> StegBand {
    match score {
        None => StegBand::Inconclusive,
        Some(value) if value < MEDIUM_THRESHOLD => StegBand::Low,
        Some(value) if value < HIGH_THRESHOLD => StegBand::Medium,
        Some(_) => StegBand::High,
    }
}

pub(crate) fn assess(methods: &[StegMethodReport]) -> StegAssessment {
    let used = methods
        .iter()
        .filter(|method| method.score.is_some())
        .count() as u8;
    let score = fused_score(methods);
    StegAssessment {
        score,
        band: band_for(score),
        methods_used: used,
        methods: methods.to_vec(),
    }
}

pub(crate) fn fused_score(methods: &[StegMethodReport]) -> Option<f64> {
    let mut weighted = 0.0;
    let mut total_weight = 0.0;
    let mut used = 0u32;
    for method in methods {
        let Some(score) = method.score else {
            continue;
        };
        let weight = match method.method {
            StegMethod::ChiSquare => CHI_WEIGHT,
            StegMethod::Rs => RS_WEIGHT,
            StegMethod::Spa => SPA_WEIGHT,
        };
        weighted += weight * score;
        total_weight += weight;
        used += 1;
    }
    if used < 2 || total_weight <= 0.0 {
        return None;
    }
    Some(weighted / total_weight)
}
