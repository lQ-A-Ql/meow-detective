//! SPA (Sample Pair Analysis) on horizontal adjacent pairs.
//!
//! Inverse-model equations and the least-squares objective follow Powell 2020
//! (arXiv:2003.03658) equations (2)-(10), using equation (8) and (10) only.

const RADIUS: i32 = 5;
const SETS: usize = (RADIUS * 2 + 1) as usize;
const GRID_STEP: f64 = 0.01;
const GRID_MAX: f64 = 0.45;
const MIN_TERMS: usize = 8;
const RESIDUAL_FLOOR: f64 = 1e-9;
const BOUNDARY_EPSILON: f64 = 1e-6;

#[derive(Debug, Clone, Copy)]
pub(crate) struct TraceCounts {
    pub e_even: [u32; SETS],
    pub o_odd: [u32; SETS],
    pub e_odd: [u32; SETS],
    pub o_even: [u32; SETS],
    pub pairs: u64,
}

impl Default for TraceCounts {
    fn default() -> Self {
        Self {
            e_even: [0; SETS],
            o_odd: [0; SETS],
            e_odd: [0; SETS],
            o_even: [0; SETS],
            pairs: 0,
        }
    }
}

impl TraceCounts {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn add_pair(&mut self, first: u8, second: u8) {
        let m = i32::from(second >> 1) - i32::from(first >> 1);
        if !(-RADIUS..=RADIUS).contains(&m) {
            return;
        }
        let index = (m + RADIUS) as usize;
        let difference = i32::from(second) - i32::from(first);
        if difference == 2 * m {
            if first.is_multiple_of(2) {
                self.e_even[index] += 1;
            } else {
                self.o_even[index] += 1;
            }
        } else if difference == 2 * m - 1 {
            self.o_odd[index] += 1;
        } else if difference == 2 * m + 1 {
            self.e_odd[index] += 1;
        }
        self.pairs += 1;
    }

    pub(crate) fn estimate(&self, min_pairs: u64) -> Option<f64> {
        if self.pairs < min_pairs {
            return None;
        }
        let mut best_p = 0.0;
        let mut best_residual = f64::INFINITY;
        let mut found = false;
        let steps = (GRID_MAX / GRID_STEP).round() as u32;
        for step in 0..=steps {
            let p = f64::from(step) * GRID_STEP;
            if let Some((residual, terms)) = self.residual_at(p) {
                if terms >= MIN_TERMS && residual < best_residual {
                    best_residual = residual;
                    best_p = p;
                    found = true;
                }
            }
        }
        if !found {
            return None;
        }
        let low = (best_p - GRID_STEP).max(0.0);
        let high = (best_p + GRID_STEP).min(GRID_MAX);
        for step in 0..=100 {
            let p = low + (high - low) * f64::from(step) / 100.0;
            if let Some((residual, terms)) = self.residual_at(p) {
                if terms >= MIN_TERMS && residual < best_residual {
                    best_residual = residual;
                    best_p = p;
                }
            }
        }
        if best_p <= BOUNDARY_EPSILON || best_p >= GRID_MAX - BOUNDARY_EPSILON {
            return None;
        }
        if best_residual <= RESIDUAL_FLOOR {
            return None;
        }
        Some((2.0 * best_p).clamp(0.0, 1.0))
    }

    fn residual_at(&self, p: f64) -> Option<(f64, usize)> {
        let a = p;
        let b = 1.0 - p;
        let gap = b - a;
        if gap.abs() <= 0.0 {
            return None;
        }
        let inverse = 1.0 / (gap * gap);
        let mut expected_odd = [0.0f64; SETS];
        let mut observed_odd = [0.0f64; SETS];
        for index in 0..SETS {
            let e_even = f64::from(self.e_even[index]);
            let o_odd = f64::from(self.o_odd[index]);
            let e_odd = f64::from(self.e_odd[index]);
            let o_even = f64::from(self.o_even[index]);
            expected_odd[index] =
                inverse * (-a * b * e_even + a * a * o_odd + b * b * e_odd - a * b * o_even);
            observed_odd[index] =
                inverse * (-a * b * e_even + b * b * o_odd + a * a * e_odd - a * b * o_even);
        }
        let mut residual = 0.0;
        let mut terms = 0usize;
        for index in 0..SETS - 1 {
            if self.set_pairs(index) == 0 || self.set_pairs(index + 1) == 0 {
                continue;
            }
            let difference = expected_odd[index] - observed_odd[index + 1];
            residual += difference * difference;
            terms += 1;
        }
        Some((residual, terms))
    }

    fn set_pairs(&self, index: usize) -> u64 {
        u64::from(self.e_even[index])
            + u64::from(self.o_odd[index])
            + u64::from(self.e_odd[index])
            + u64::from(self.o_even[index])
    }
}
