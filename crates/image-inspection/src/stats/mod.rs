use crate::limits::StegLimits;
use crate::stats::chi_square::{chi_square_p, chi_square_score};
use crate::types::{StegMethod, StegMethodReport};

pub(crate) mod chi_square;
pub(crate) mod fusion;
pub(crate) mod gamma;
pub(crate) mod rs;
pub(crate) mod spa;

pub(crate) use fusion::{assess, band_for};
pub(crate) use rs::RsCounts;
pub(crate) use spa::TraceCounts;

/// Per-plane or per-block counters shared by all detectors.
pub(crate) struct Accumulator {
    pub samples: u64,
    pub histogram: Box<[u32; 256]>,
    pub unique: [u64; 4],
    pub rs: RsCounts,
    pub trace: TraceCounts,
}

impl Accumulator {
    pub(crate) fn new() -> Self {
        Self {
            samples: 0,
            histogram: Box::new([0; 256]),
            unique: [0; 4],
            rs: RsCounts::default(),
            trace: TraceCounts::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.samples = 0;
        self.histogram.fill(0);
        self.unique = [0; 4];
        self.rs = RsCounts::default();
        self.trace.clear();
    }

    pub(crate) fn add_segment(&mut self, row: &[u8]) {
        self.samples += row.len() as u64;
        for &value in row {
            self.histogram[value as usize] += 1;
            self.unique[(value >> 6) as usize] |= 1u64 << (value & 63);
        }
        for pair in row.windows(2) {
            self.trace.add_pair(pair[0], pair[1]);
        }
        for group in row.as_chunks::<4>().0 {
            self.rs.add_group(group);
        }
    }

    pub(crate) fn unique_count(&self) -> u32 {
        self.unique.iter().map(|word| word.count_ones()).sum()
    }

    pub(crate) fn method_reports(
        &self,
        limits: &StegLimits,
        include_texture: bool,
    ) -> Vec<StegMethodReport> {
        let mut methods = Vec::with_capacity(3);
        if self.unique_count() < 2 {
            return methods;
        }
        if let Some((p_value, degrees)) = chi_square_p(&self.histogram) {
            if degrees >= limits.min_chi_df {
                methods.push(StegMethodReport {
                    method: StegMethod::ChiSquare,
                    estimate: Some(p_value),
                    score: Some(chi_square_score(p_value)),
                    samples: self.samples,
                });
            }
        }
        if !include_texture {
            return methods;
        }
        if let Some(rate) = self.rs.estimate(limits.min_rs_groups) {
            methods.push(StegMethodReport {
                method: StegMethod::Rs,
                estimate: Some(rate),
                score: Some(100.0 * rate),
                samples: self.samples,
            });
        }
        if let Some(rate) = self.trace.estimate(limits.min_spa_pairs) {
            methods.push(StegMethodReport {
                method: StegMethod::Spa,
                estimate: Some(rate),
                score: Some(100.0 * rate),
                samples: self.samples,
            });
        }
        methods
    }
}
