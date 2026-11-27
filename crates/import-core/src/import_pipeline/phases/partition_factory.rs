use crate::import_pipeline::context::ImportJobContext;
use evidence_access::datasource_service;
use transport::CommandError;

pub(super) fn partition_factory(
    ctx: &ImportJobContext<'_>,
    candidates: &[datasource_service::ImageFilesystemCandidate],
    index: usize,
) -> Result<Option<evidence_core::SharedReaderFactory>, CommandError> {
    let indices = datasource_service::assign_effective_partition_indices(candidates);
    let candidate = candidates
        .iter()
        .enumerate()
        .find(|(position, candidate)| {
            datasource_service::effective_partition_index(candidate, *position, &indices) == index
        })
        .map(|(_, candidate)| candidate);
    if let Some(candidate) = candidate.filter(|candidate| candidate.lvm_identity.is_some()) {
        let Some(plan) = &ctx.import_config.collection_plan else {
            return Ok(None);
        };
        let factory = plan
            .logical_volume_factory(candidate)
            .map_err(CommandError::from_typed_service_error)?;
        if let Some(scope) = ctx
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.factory().reader_scope())
        {
            factory.bind_reader_scope(scope);
        }
        return Ok(Some(factory));
    }
    Ok(ctx.runtime.as_ref().map(|runtime| runtime.factory()))
}
