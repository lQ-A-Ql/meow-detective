use std::{collections::BTreeMap, sync::Arc};

use domain::structure::{ActivityState, TargetResult};
use persistence_sqlite::repositories::structure_activity_repo::StructureActivityRepo;

use super::{
    ActivityDag, ActivityProof, LayerAnalysisResult, RuntimeError, RuntimeReport, RuntimeStart,
};

pub(super) struct ReportContext<'a> {
    pub start: &'a RuntimeStart<'a>,
    pub dag: &'a ActivityDag,
    pub states: &'a BTreeMap<String, ActivityState>,
    pub proofs: &'a BTreeMap<String, Arc<ActivityProof>>,
    pub results: BTreeMap<String, TargetResult>,
    pub analysis_results: BTreeMap<String, LayerAnalysisResult>,
    pub pending_bindings: usize,
}

pub(super) fn collect(mut context: ReportContext<'_>) -> Result<RuntimeReport, RuntimeError> {
    let repository = StructureActivityRepo::new(context.start.connection);
    let activities = context
        .states
        .keys()
        .map(|key| {
            repository
                .get(&context.start.request.case_id, context.start.run_id, key)?
                .ok_or(RuntimeError::LostOwner)
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    for (key, proof) in context.proofs {
        if let (Some(definition), Err(error)) = (context.dag.get(key), proof.validate()) {
            let Some(failure) = super::proof_state::proof_failure(&error) else {
                return Err(RuntimeError::Io(error));
            };
            let result = definition.failure_result(failure)?;
            context.results.insert(result.target_id.clone(), result);
            context.analysis_results.remove(key);
        }
    }
    Ok(RuntimeReport {
        run_id: context.start.run_id.into(),
        activities,
        activity_stages: context
            .dag
            .definitions()
            .map(|(key, definition)| (key.clone(), definition.stage))
            .collect(),
        target_results: context.results.into_values().collect(),
        analysis_results: context.analysis_results,
        pending_bindings: context.pending_bindings,
    })
}
