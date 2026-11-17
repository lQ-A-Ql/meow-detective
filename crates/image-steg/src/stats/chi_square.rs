use super::gamma::regularized_gamma_q;

/// Returns the chi-square p-value and the unbiased degrees of freedom.
///
/// Returns `None` when no PoV pair has samples or the resulting degrees of
/// freedom are below one.
pub(crate) fn chi_square_p(histogram: &[u32; 256]) -> Option<(f64, u32)> {
    let mut chi_square = 0.0;
    let mut pairs = 0u32;
    for index in 0..128 {
        let even = f64::from(histogram[index * 2]);
        let odd = f64::from(histogram[index * 2 + 1]);
        let total = even + odd;
        if total == 0.0 {
            continue;
        }
        let expected = total / 2.0;
        chi_square += (even - expected) * (even - expected) / expected;
        chi_square += (odd - expected) * (odd - expected) / expected;
        pairs += 1;
    }
    let degrees = pairs.saturating_sub(1);
    if degrees == 0 {
        return None;
    }
    Some((
        regularized_gamma_q(f64::from(degrees) / 2.0, chi_square / 2.0),
        degrees,
    ))
}

/// Maps a p-value to a 0..100 suspicion score.
pub(crate) fn chi_square_score(p_value: f64) -> f64 {
    100.0 * (2.0 * p_value - 1.0).clamp(0.0, 1.0)
}
