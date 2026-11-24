use indicatif::ProgressBar;
use std::time::{Duration, Instant};
use transport::dto::TaskProgressUnitDto;

pub(super) struct StageEstimate {
    bar: ProgressBar,
    first_sample: Option<Instant>,
    samples: u32,
    unit: Option<TaskProgressUnitDto>,
    previous_eta: Option<u64>,
    eta_updated: Option<Instant>,
}

impl StageEstimate {
    pub fn new() -> Self {
        Self {
            bar: ProgressBar::hidden(),
            first_sample: None,
            samples: 0,
            unit: None,
            previous_eta: None,
            eta_updated: None,
        }
    }

    pub fn record(&mut self, position: u64, total: Option<u64>, unit: TaskProgressUnitDto) {
        if self.unit != Some(unit) || position < self.bar.position() {
            *self = Self::new();
            self.unit = Some(unit);
        }
        if self.bar.length() != total {
            self.previous_eta = None;
            self.eta_updated = None;
        }
        if let Some(total) = total {
            self.bar.set_length(total);
        } else {
            self.bar.unset_length();
        }
        if self.first_sample.is_none() {
            self.first_sample = Some(Instant::now());
            self.bar.set_position(position);
            self.bar.reset_eta();
        } else if position > self.bar.position() {
            self.samples += 1;
            self.bar.set_position(position);
        }
    }

    pub fn values(&mut self) -> (Option<f64>, Option<u64>) {
        if self.samples < 3
            || !self
                .first_sample
                .is_some_and(|start| start.elapsed() >= Duration::from_secs(5))
        {
            return (None, None);
        }
        let rate = self.bar.per_sec();
        if !rate.is_finite() || rate <= 0.0 {
            return (None, None);
        }
        if self
            .eta_updated
            .is_some_and(|updated| updated.elapsed() < Duration::from_secs(1))
        {
            return (Some(rate), self.previous_eta);
        }
        let eta = self
            .bar
            .length()
            .filter(|total| *total > self.bar.position())
            .map(|_| {
                let estimate = self.bar.eta().as_secs().max(1);
                let bounded = self.previous_eta.map_or(estimate, |previous| {
                    estimate.clamp(
                        previous.saturating_sub((previous / 4).max(1)),
                        previous.saturating_add((previous / 4).max(1)),
                    )
                });
                self.previous_eta = Some(bounded);
                self.eta_updated = Some(Instant::now());
                bounded
            });
        (Some(rate), eta)
    }
}
