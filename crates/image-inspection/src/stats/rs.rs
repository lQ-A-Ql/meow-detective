//! RS analysis (Fridrich et al.) on horizontal, non-overlapping 4-pixel groups.
//!
//! The message length is derived from the root of the quadratic in
//! `2(d1 + d0)z^2 + (d-0 - d-1 - d1 - 3d0)z + d0 - d-0` with the smallest
//! absolute value via `p = z / (z - 1/2)` (Fridrich et al., Eq. (4)).

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RsCounts {
    pub r_m: i64,
    pub s_m: i64,
    pub r_minus_m: i64,
    pub s_minus_m: i64,
    pub r_m_flipped: i64,
    pub s_m_flipped: i64,
    pub r_minus_m_flipped: i64,
    pub s_minus_m_flipped: i64,
    pub groups: u64,
}

impl RsCounts {
    pub(crate) fn add_group(&mut self, group: &[u8; 4]) {
        self.groups += 1;
        let original = variation(group);
        let positive = masked_variation(group, false);
        let negative = masked_variation(group, true);
        update_pair(&mut self.r_m, &mut self.s_m, positive, original);
        update_pair(&mut self.r_minus_m, &mut self.s_minus_m, negative, original);

        let flipped = group.map(|value| value ^ 1);
        let flipped_original = variation(&flipped);
        let flipped_positive = masked_variation(&flipped, false);
        let flipped_negative = masked_variation(&flipped, true);
        update_pair(
            &mut self.r_m_flipped,
            &mut self.s_m_flipped,
            flipped_positive,
            flipped_original,
        );
        update_pair(
            &mut self.r_minus_m_flipped,
            &mut self.s_minus_m_flipped,
            flipped_negative,
            flipped_original,
        );
    }

    pub(crate) fn estimate(&self, min_groups: u64) -> Option<f64> {
        if self.groups < min_groups {
            return None;
        }
        let d0 = (self.r_m - self.s_m) as f64;
        let d1 = (self.r_m_flipped - self.s_m_flipped) as f64;
        let d_minus_0 = (self.r_minus_m - self.s_minus_m) as f64;
        let d_minus_1 = (self.r_minus_m_flipped - self.s_minus_m_flipped) as f64;
        let a = 2.0 * (d0 + d1);
        if a.abs() < 1e-9 {
            return None;
        }
        let b = d_minus_0 - d_minus_1 - d1 - 3.0 * d0;
        let c = d0 - d_minus_0;
        let discriminant = b * b - 4.0 * a * c;
        if discriminant < 0.0 || !discriminant.is_finite() {
            return None;
        }
        let root = discriminant.sqrt();
        let mut best: Option<f64> = None;
        for candidate in [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)] {
            if candidate.is_finite()
                && best.is_none_or(|current: f64| candidate.abs() < current.abs())
            {
                best = Some(candidate);
            }
        }
        let z = best?;
        let gap = z - 0.5;
        if gap.abs() < 1e-9 {
            return None;
        }
        let rate = z / gap;
        if !rate.is_finite() || !(0.0..=1.0).contains(&rate) {
            return None;
        }
        Some(rate)
    }
}

fn update_pair(r: &mut i64, s: &mut i64, changed: i32, original: i32) {
    if changed > original {
        *r += 1;
    } else if changed < original {
        *s += 1;
    }
}

fn variation(group: &[u8; 4]) -> i32 {
    (i32::from(group[1]) - i32::from(group[0])).abs()
        + (i32::from(group[2]) - i32::from(group[1])).abs()
        + (i32::from(group[3]) - i32::from(group[2])).abs()
}

fn masked_variation(group: &[u8; 4], minus: bool) -> i32 {
    let first = flip(group[0], minus);
    let third = flip(group[2], minus);
    (i32::from(group[1]) - i32::from(first)).abs()
        + (i32::from(third) - i32::from(group[1])).abs()
        + (i32::from(group[3]) - i32::from(third)).abs()
}

fn flip(value: u8, minus: bool) -> u8 {
    if !minus {
        return value ^ 1;
    }
    match value {
        0 | 255 => value,
        even if even % 2 == 0 => even - 1,
        odd => odd + 1,
    }
}
