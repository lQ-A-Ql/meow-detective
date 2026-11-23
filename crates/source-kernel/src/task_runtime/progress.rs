use std::time::{Duration, Instant};

use transport::dto::{
    TaskProgressDto, TaskProgressModeDto as Mode, TaskProgressStateDto as State,
    TaskProgressUnitDto as Unit, TaskStageKeyDto as Stage, TaskStageProgressDto,
};

mod estimate;
use estimate::StageEstimate;

/// Job-local progress, advanced only by completed work and explicit phase transitions.
pub struct TaskProgressReporter {
    snapshot: TaskProgressDto,
    started: Instant,
    stage_started: Instant,
    estimate: StageEstimate,
    last_emit: Option<Instant>,
}

impl TaskProgressReporter {
    pub fn progress(&self) -> u32 {
        self.snapshot.progress
    }
    pub fn new(job: &str, case: &str, nested: bool) -> Self {
        let plan = if nested {
            vec![
                (Stage::Validate, 5),
                (Stage::Register, 5),
                (Stage::Probe, 10),
                (Stage::Enumerate, 55),
                (Stage::Analyze, 20),
                (Stage::Publish, 5),
            ]
        } else {
            vec![
                (Stage::Prepare, 8),
                (Stage::Enumerate, 37),
                (Stage::Analyze, 35),
                (Stage::Index, 15),
                (Stage::Publish, 5),
            ]
        };
        Self {
            snapshot: TaskProgressDto {
                job_id: job.into(),
                case_id: case.into(),
                seq: 0,
                state: State::Running,
                progress: 0,
                mode: Mode::Indeterminate,
                stage: plan[0].0,
                stages: plan
                    .into_iter()
                    .map(|(key, weight)| TaskStageProgressDto {
                        key,
                        weight,
                        state: State::Pending,
                    })
                    .collect(),
                completed_units: 0,
                total_units: None,
                unit: Unit::Items,
                elapsed_ms: 0,
                stage_elapsed_ms: 0,
                heartbeat_at: String::new(),
                heartbeat: false,
                rate: None,
                eta_seconds: None,
                cancellable: true,
            },
            started: Instant::now(),
            stage_started: Instant::now(),
            estimate: StageEstimate::new(),
            last_emit: None,
        }
    }

    pub fn start(&mut self, stage: Stage) -> TaskProgressDto {
        if self.snapshot.stage != stage {
            self.stage_started = Instant::now();
            self.estimate = StageEstimate::new();
            self.snapshot.completed_units = 0;
            self.snapshot.total_units = None;
            self.snapshot.rate = None;
            self.snapshot.eta_seconds = None;
        }
        self.snapshot.stage = stage;
        self.snapshot.mode = Mode::Indeterminate;
        self.stage_mut(stage).state = State::Running;
        self.emit()
    }

    pub fn observe(
        &mut self,
        stage: Stage,
        completed: u64,
        total: Option<u64>,
        unit: Unit,
    ) -> Option<TaskProgressDto> {
        if self.snapshot.stage != stage || self.stage_mut(stage).state == State::Pending {
            self.start(stage);
            self.last_emit = None;
        }
        self.advance(completed, total, unit)
    }

    pub fn finish_stage(&mut self, stage: Stage, skipped: bool) -> TaskProgressDto {
        self.stage_mut(stage).state = if skipped {
            State::Skipped
        } else {
            State::Completed
        };
        self.emit()
    }

    pub fn advance(
        &mut self,
        completed: u64,
        total: Option<u64>,
        unit: Unit,
    ) -> Option<TaskProgressDto> {
        self.estimate.record(completed, total, unit);
        self.snapshot.completed_units = completed;
        self.snapshot.total_units = total;
        self.snapshot.unit = unit;
        self.snapshot.mode = if total.is_some_and(|total| total > 0) {
            Mode::Estimated
        } else {
            Mode::Indeterminate
        };
        if self
            .last_emit
            .is_some_and(|last| last.elapsed() < Duration::from_millis(250))
        {
            return None;
        }
        Some(self.emit())
    }

    pub fn heartbeat(&mut self, cancelling: bool) -> TaskProgressDto {
        if cancelling {
            self.snapshot.state = State::Cancelling;
            self.snapshot.mode = Mode::Determinate;
        }
        self.snapshot.heartbeat = true;
        let event = self.emit();
        self.snapshot.heartbeat = false;
        event
    }

    pub fn finish(&mut self, state: State) -> TaskProgressDto {
        self.snapshot.state = state;
        self.snapshot.cancellable = false;
        self.snapshot.eta_seconds = None;
        self.snapshot.rate = None;
        if state == State::Completed {
            for stage in &mut self.snapshot.stages {
                if stage.state != State::Skipped {
                    stage.state = State::Completed;
                }
            }
            self.snapshot.progress = 100;
            self.snapshot.mode = Mode::Determinate;
        } else {
            self.stage_mut(self.snapshot.stage).state = state;
            self.snapshot.mode = Mode::Determinate;
        }
        self.emit()
    }

    fn stage_mut(&mut self, stage: Stage) -> &mut TaskStageProgressDto {
        self.snapshot
            .stages
            .iter_mut()
            .find(|entry| entry.key == stage)
            .expect("stage belongs to task plan")
    }

    fn emit(&mut self) -> TaskProgressDto {
        self.snapshot.seq += 1;
        self.snapshot.elapsed_ms = self.started.elapsed().as_millis() as u64;
        self.snapshot.stage_elapsed_ms = self.stage_started.elapsed().as_millis() as u64;
        self.snapshot.heartbeat_at = chrono::Utc::now().to_rfc3339();
        let completed: u32 = self
            .snapshot
            .stages
            .iter()
            .filter(|stage| matches!(stage.state, State::Completed | State::Skipped))
            .map(|stage| stage.weight)
            .sum();
        let current = self
            .snapshot
            .stages
            .iter()
            .find(|stage| stage.key == self.snapshot.stage)
            .unwrap();
        let fraction = self
            .snapshot
            .total_units
            .filter(|total| *total > 0)
            .map_or(0.0, |total| {
                (self.snapshot.completed_units as f64 / total as f64).min(1.0)
            });
        let active = if current.state == State::Running {
            (current.weight as f64 * fraction) as u32
        } else {
            0
        };
        if self.snapshot.state != State::Completed {
            self.snapshot.progress = self.snapshot.progress.max((completed + active).min(99));
        }
        if self.snapshot.state == State::Running {
            (self.snapshot.rate, self.snapshot.eta_seconds) = self.estimate.values();
        } else {
            self.snapshot.rate = None;
            self.snapshot.eta_seconds = None;
        }
        self.last_emit = Some(Instant::now());
        self.snapshot.clone()
    }
}
