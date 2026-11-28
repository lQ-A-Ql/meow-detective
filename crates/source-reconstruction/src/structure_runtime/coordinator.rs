use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use domain::structure::{
    ActivityState, CancellationScope, StructureAttemptRef, StructureRequest, TargetFailure,
    TargetResult,
};
use persistence_sqlite::repositories::structure_activity_repo::{
    ActivityRecord, StructureActivityRepo,
};
use rusqlite::Connection;
use source_kernel::source_runtime::verification::VerificationBatch;

use super::{
    frontier::{CapabilityEntry, CapabilityIndex, Frontier, PendingBindings},
    source_scope_id, ActivityDag, ActivityDefinition, ActivityOutput, ActivityProof,
    BindingContext, BindingDecision, ExecutableRegistry, ExecutionContext, RuntimeError,
};

pub struct RuntimeReport {
    pub run_id: String,
    pub activities: Vec<ActivityRecord>,
    pub activity_stages: BTreeMap<String, domain::structure::StructureStage>,
    pub target_results: Vec<TargetResult>,
    pub analysis_results: BTreeMap<String, super::LayerAnalysisResult>,
    pub pending_bindings: usize,
}

pub struct RuntimeStart<'a> {
    pub connection: &'a Connection,
    pub case_root: &'a Path,
    pub request: &'a StructureRequest,
    pub registry: &'a ExecutableRegistry,
    pub run_id: &'a str,
    pub attempt_id: &'a str,
    pub owner_id: &'a str,
    pub cancel: &'a AtomicBool,
}

pub fn run(
    start: RuntimeStart<'_>,
    seeds: Vec<CapabilityEntry>,
    initial: Vec<ActivityDefinition>,
) -> Result<RuntimeReport, RuntimeError> {
    start
        .request
        .validate()
        .map_err(|_| RuntimeError::InvalidDefinition)?;
    if [start.run_id, start.attempt_id, start.owner_id]
        .iter()
        .any(|value| value.trim().is_empty())
    {
        return Err(RuntimeError::InvalidDefinition);
    }
    if !start.connection.is_autocommit() {
        return Err(RuntimeError::InvalidDefinition);
    }
    let scope_id = source_scope_id(start.request)?;
    if seeds.iter().any(|entry| entry.scope_id != scope_id) {
        return Err(RuntimeError::InvalidDefinition);
    }
    if let domain::structure::VersionSelection::Exact { view_id } = &start.request.version_selection
    {
        if !seeds
            .iter()
            .any(|entry| &entry.capability.view_id == view_id)
        {
            return Err(RuntimeError::InvalidDefinition);
        }
    }
    let limit = start.request.resource_policy.active_work_items as usize;
    let attempt = Some(StructureAttemptRef {
        attempt_id: start.attempt_id.into(),
        owner: start.owner_id.into(),
    });
    let batch = if start
        .request
        .goals
        .contains(&domain::structure::StructureGoal::DeepVerify)
    {
        VerificationBatch::new_deep(attempt)?
    } else {
        VerificationBatch::new(attempt)?
    };
    let mut runtime = Coordinator {
        start,
        batch,
        dag: ActivityDag::new(limit, limit * 4)?,
        capabilities: CapabilityIndex::with_limits(limit, limit * 8)?,
        frontier: Frontier::new(limit)?,
        pending: PendingBindings::with_limits(limit, limit * 8)?,
        waiting: BTreeMap::new(),
        proofs: BTreeMap::new(),
        states: BTreeMap::new(),
        results: BTreeMap::new(),
        readiness: super::readiness::Readiness::default(),
        analysis_results: BTreeMap::new(),
    };
    runtime
        .capabilities
        .publish_batch_to_frontier(seeds, &mut runtime.frontier)?;
    runtime.insert(initial)?;
    runtime.drive()?;
    runtime.report()
}

struct Coordinator<'a> {
    start: RuntimeStart<'a>,
    batch: VerificationBatch,
    dag: ActivityDag,
    capabilities: CapabilityIndex,
    frontier: Frontier,
    pending: PendingBindings,
    waiting: BTreeMap<String, (String, String, String)>,
    proofs: BTreeMap<String, Arc<ActivityProof>>,
    states: BTreeMap<String, ActivityState>,
    results: BTreeMap<String, TargetResult>,
    readiness: super::readiness::Readiness,
    analysis_results: BTreeMap<String, super::LayerAnalysisResult>,
}

impl Coordinator<'_> {
    fn insert(&mut self, definitions: Vec<ActivityDefinition>) -> Result<(), RuntimeError> {
        for definition in &definitions {
            let handler = self
                .start
                .registry
                .get(&definition.work.adapter_id, &definition.work.operation_id)
                .ok_or(RuntimeError::MissingOperation)?;
            let descriptor = handler.descriptor();
            if descriptor.adapter_version != definition.work.adapter_version
                || descriptor.stage != definition.stage
            {
                return Err(RuntimeError::InvalidDefinition);
            }
            for id in &definition.work.input_capability_versions {
                let entry = self
                    .capabilities
                    .get(id)?
                    .ok_or(RuntimeError::InvalidDefinition)?;
                if entry.scope_id != definition.scope_id {
                    return Err(RuntimeError::InvalidDefinition);
                }
            }
        }
        let mut staged = self.dag.clone();
        let inserted = staged.insert_batch(definitions)?;
        let states = super::activity_registration::register(&self.start, &staged, &inserted)?;
        self.dag = staged;
        self.states.extend(states);
        self.readiness
            .register(&inserted, &self.dag, &self.states)?;
        Ok(())
    }

    fn drive(&mut self) -> Result<(), RuntimeError> {
        loop {
            if self.start.cancel.load(Ordering::Acquire) && !self.states.is_empty() {
                self.cancel_active()?;
                break;
            }
            while let Some(id) = self.frontier.pop() {
                self.discover(&id)?;
            }
            if self.start.cancel.load(Ordering::Acquire) {
                self.cancel_active()?;
                break;
            }
            while let Some(key) = self.readiness.pop_blocked() {
                self.fail(
                    &key,
                    ActivityState::Blocked,
                    TargetFailure::RequiredDependencyFailed,
                )?;
            }
            if let Some(key) = self.readiness.pop_ready() {
                if self.start.cancel.load(Ordering::Acquire) {
                    self.fail(
                        &key,
                        ActivityState::Cancelled,
                        TargetFailure::Cancelled {
                            scope: CancellationScope::Run,
                        },
                    )?;
                    continue;
                }
                self.execute(&key)?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn cancel_active(&mut self) -> Result<(), RuntimeError> {
        let active = self
            .states
            .iter()
            .filter(|(_, state)| !state.is_terminal())
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in active {
            self.fail(
                &key,
                ActivityState::Cancelled,
                TargetFailure::Cancelled {
                    scope: CancellationScope::Run,
                },
            )?;
        }
        Ok(())
    }

    fn discover(&mut self, id: &str) -> Result<(), RuntimeError> {
        let changed = self
            .capabilities
            .get(id)?
            .ok_or(RuntimeError::InvalidOutput)?
            .clone();
        let mut operations = self
            .start
            .registry
            .matching(&changed)
            .into_iter()
            .map(|handler| {
                let descriptor = handler.descriptor();
                (
                    descriptor.adapter_id,
                    descriptor.operation_id,
                    id.to_owned(),
                    None,
                )
            })
            .collect::<Vec<_>>();
        for candidate in self.pending.on_publish(&changed)? {
            let (adapter, operation, anchor) = self
                .waiting
                .get(&candidate)
                .ok_or(RuntimeError::InvalidDefinition)?
                .clone();
            operations.push((adapter, operation, anchor, Some(candidate)));
        }
        for (adapter, operation, anchor, pending_id) in operations {
            if !self.start.request.adapter_hints.is_empty()
                && !self.start.request.adapter_hints.contains(&adapter)
            {
                continue;
            }
            let handler = self
                .start
                .registry
                .get(&adapter, &operation)
                .ok_or(RuntimeError::MissingOperation)?;
            let anchor_entry = self
                .capabilities
                .get(&anchor)?
                .ok_or(RuntimeError::InvalidDefinition)?;
            let decision = handler.bind(&BindingContext {
                request: self.start.request,
                changed: anchor_entry,
                capabilities: &self.capabilities,
            })?;
            self.apply_binding(decision, adapter, operation, anchor, pending_id)?;
        }
        Ok(())
    }

    fn apply_binding(
        &mut self,
        decision: BindingDecision,
        adapter: String,
        operation: String,
        anchor: String,
        pending_id: Option<String>,
    ) -> Result<(), RuntimeError> {
        if let Some(id) = &pending_id {
            self.pending.remove(id)?;
            self.waiting.remove(id);
        }
        match decision {
            BindingDecision::NotMatched => {}
            BindingDecision::Rejected(results) => {
                for result in results {
                    result.validate().map_err(|_| RuntimeError::InvalidOutput)?;
                    if result.status == domain::structure::TargetStatus::Verified {
                        return Err(RuntimeError::InvalidOutput);
                    }
                    self.results.insert(result.target_id.clone(), result);
                }
            }
            BindingDecision::Bound(definitions) => self.insert(definitions)?,
            BindingDecision::RequiresEvidence {
                candidate_id,
                prerequisites,
                result,
            } => {
                let result = *result;
                result.validate().map_err(|_| RuntimeError::InvalidOutput)?;
                if result.status != domain::structure::TargetStatus::NeedsEvidence {
                    return Err(RuntimeError::InvalidOutput);
                }
                if let Some(existing) = self.waiting.get(&candidate_id) {
                    if existing != &(adapter.clone(), operation.clone(), anchor.clone()) {
                        return Err(RuntimeError::ConflictingWork);
                    }
                }
                self.pending.register(candidate_id.clone(), prerequisites)?;
                self.waiting
                    .insert(candidate_id, (adapter, operation, anchor));
                self.results.insert(result.target_id.clone(), result);
            }
        }
        Ok(())
    }

    fn execute(&mut self, key: &str) -> Result<(), RuntimeError> {
        self.transition(key, ActivityState::Ready, None, None)?;
        self.transition(key, ActivityState::Admitted, None, None)?;
        self.transition(key, ActivityState::Running, None, None)?;
        let definition = self.dag.get(key).expect("ready activity is absent").clone();
        let handler = self
            .start
            .registry
            .get(&definition.work.adapter_id, &definition.work.operation_id)
            .ok_or(RuntimeError::MissingOperation)?;
        let context = ExecutionContext {
            connection: self.start.connection,
            case_root: self.start.case_root,
            request: self.start.request,
            attempt_id: self.start.attempt_id,
            owner_id: self.start.owner_id,
            batch: &self.batch,
            input_proofs: &self.proofs,
            cancel: self.start.cancel,
        };
        let outcome = super::execution::execute_in_worker(handler, &definition, &context)
            .and_then(|output| self.complete(key, &definition, output));
        match outcome {
            Ok(()) => Ok(()),
            Err(RuntimeError::Target(failure)) => {
                let state = match failure {
                    TargetFailure::Cancelled { .. } => ActivityState::Cancelled,
                    TargetFailure::MissingCapabilities { .. }
                    | TargetFailure::MissingEvidence { .. }
                    | TargetFailure::RequiredDependencyFailed => ActivityState::Blocked,
                    _ => ActivityState::Failed,
                };
                self.fail(key, state, failure)
            }
            Err(error) => Err(error),
        }
    }

    fn complete(
        &mut self,
        key: &str,
        definition: &ActivityDefinition,
        output: ActivityOutput,
    ) -> Result<(), RuntimeError> {
        output.validate(definition).map_err(|error| match error {
            RuntimeError::Io(error) => super::proof_state::classify(error),
            error => error,
        })?;
        let receipt = output.proof.id().to_owned();
        if output.proof.attempt().attempt_id != self.start.attempt_id
            || output.proof.attempt().owner != self.start.owner_id
            || output.proof.source_revision().case_id != self.start.request.case_id
            || !self
                .start
                .request
                .source_scope
                .iter()
                .any(|capture| &capture.source == output.proof.source_revision())
        {
            return Err(RuntimeError::InvalidOutput);
        }
        for result in &output.target_results {
            if self.results.get(&result.target_id).is_some_and(|existing| {
                existing != result
                    && existing.status != domain::structure::TargetStatus::NeedsEvidence
            }) {
                return Err(RuntimeError::InvalidOutput);
            }
        }
        let entries = output.capabilities;
        let ids = self
            .capabilities
            .publish_batch_to_frontier(entries, &mut self.frontier)
            .map_err(|error| match error {
                super::frontier::FrontierError::CapacityExceeded {
                    required, limit, ..
                } => RuntimeError::Target(TargetFailure::ResourceLimit {
                    resource: domain::structure::ResourceKind::ActiveWork,
                    required: required as u64,
                    limit: limit as u64,
                }),
                error => RuntimeError::Frontier(error),
            })?;
        for id in ids {
            self.proofs.insert(id, output.proof.clone());
        }
        self.transition(key, ActivityState::Succeeded, Some(&receipt), None)?;
        if let Some(analysis) = output.analysis {
            self.analysis_results.insert(key.into(), analysis);
        }
        for result in output.target_results {
            self.results.insert(result.target_id.clone(), result);
        }
        self.proofs.insert(key.into(), output.proof);
        Ok(())
    }

    fn fail(
        &mut self,
        key: &str,
        state: ActivityState,
        failure: TargetFailure,
    ) -> Result<(), RuntimeError> {
        let result = self
            .dag
            .get(key)
            .expect("failed activity is absent")
            .failure_result(failure.clone())?;
        self.transition(key, state, None, Some(&failure))?;
        self.results.insert(result.target_id.clone(), result);
        Ok(())
    }

    fn transition(
        &mut self,
        key: &str,
        next: ActivityState,
        receipt: Option<&str>,
        failure: Option<&TargetFailure>,
    ) -> Result<(), RuntimeError> {
        let expected = *self
            .states
            .get(key)
            .ok_or(RuntimeError::InvalidDefinition)?;
        if !StructureActivityRepo::new(self.start.connection).transition(
            &self.start.request.case_id,
            self.start.run_id,
            key,
            self.start.attempt_id,
            self.start.owner_id,
            expected,
            next,
            receipt,
            failure,
        )? {
            return Err(RuntimeError::LostOwner);
        }
        self.states.insert(key.into(), next);
        if next.is_terminal() {
            self.readiness.completed(key, next);
        }
        Ok(())
    }

    fn report(self) -> Result<RuntimeReport, RuntimeError> {
        super::report::collect(super::report::ReportContext {
            start: &self.start,
            dag: &self.dag,
            states: &self.states,
            proofs: &self.proofs,
            results: self.results,
            analysis_results: self.analysis_results,
            pending_bindings: self.pending.len(),
        })
    }
}
