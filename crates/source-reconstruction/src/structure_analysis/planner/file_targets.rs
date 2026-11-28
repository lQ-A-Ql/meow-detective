use domain::{
    structure::{
        ByteRange, ByteRangeSet, ReadDemand, ReadIntent, ReadVerification, StructureWorkItem,
    },
    CaseId, DataSourceKind,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageRepo;

use crate::structure_analysis::{
    adapter::{AdapterRegistry, BTRFS_ADAPTER_ID, CEPHFS_ADAPTER_ID},
    graph_builder::SourceContext,
    request::{StructureAnalysisError, StructureAnalysisRequest},
};

pub(super) fn append(
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    source: &SourceContext,
    request: &StructureAnalysisRequest,
    registry: &AdapterRegistry,
    items: &mut Vec<StructureWorkItem>,
) -> Result<(), StructureAnalysisError> {
    let btrfs = source.data_source.kind == DataSourceKind::BtrfsPool;
    let adapter_id = if btrfs {
        BTRFS_ADAPTER_ID
    } else {
        CEPHFS_ADAPTER_ID
    };
    let adapter = registry
        .get(adapter_id)
        .ok_or_else(|| StructureAnalysisError::UnsupportedAdapter(adapter_id.into()))?;
    super::require_features(adapter, &request.required_features)?;
    if !btrfs && !cephfs_admitted(conn, source, request)? {
        return Ok(());
    }
    let ready = source_kernel::source_db::open_ready_source_read_only_by_id(
        conn,
        root,
        &CaseId(source.revision_ref.case_id.clone()),
        &source.data_source.id,
    )
    .map_err(|error| error.into_db_error())?;
    FileRepo::new(&ready.connection).visit_all(Some(&source.data_source.id.0), |entry| {
        if !entry.is_file() || entry.size.unwrap_or_default() == 0 {
            return Ok(());
        }
        if items.len() >= super::super::scheduler::MAX_WORK_ITEMS {
            return Err(persistence_sqlite::DbError::System(
                "file-target work-item bound exceeded".into(),
            ));
        }
        let size = entry.size.expect("nonempty file size");
        let length = if request.intent == ReadIntent::ContentRead {
            size
        } else {
            size.min(4096)
        };
        let fingerprint = format!(
            "{}{}",
            if btrfs {
                evidence_access::structure_access::BTRFS_FILE_TARGET_PREFIX
            } else {
                evidence_access::structure_access::CEPHFS_FILE_TARGET_PREFIX
            },
            entry.id.0
        );
        let ranges =
            ByteRangeSet::new(size, [ByteRange::new(0, length).map_err(error)?]).map_err(error)?;
        let mut demand = ReadDemand::new(
            source.revision_ref.clone(),
            fingerprint.clone(),
            request.intent,
            ranges.clone(),
            request.priority,
            None,
            true,
            request.consumers.clone(),
            if request.intent == ReadIntent::ContentRead {
                ReadVerification::ContentDigest
            } else {
                ReadVerification::RangeCoverage
            },
        )
        .map_err(error)?;
        if let Some(attempt) = &request.attempt {
            demand = demand.with_attempt(attempt.clone()).map_err(error)?;
        }
        items.push(StructureWorkItem {
            id: format!("{adapter_id}:{}:{}", source.data_source.id.0, entry.id.0),
            source_revision: source.revision_ref.clone(),
            target_fingerprint: fingerprint,
            adapter_id: adapter.id.clone(),
            adapter_version: adapter.version.clone(),
            ranges,
            intent: request.intent,
            priority: request.priority,
            dependencies: Vec::new(),
            consumers: request.consumers.clone(),
            demand: Some(demand),
        });
        Ok(())
    })?;
    Ok(())
}

fn cephfs_admitted(
    conn: &rusqlite::Connection,
    source: &SourceContext,
    request: &StructureAnalysisRequest,
) -> Result<bool, StructureAnalysisError> {
    let lineage = CephFsDerivedLineageRepo::new(conn)
        .find_by_data_source(&source.data_source.id.0)?
        .ok_or_else(|| StructureAnalysisError::Plan("CephFS lineage is missing".into()))?;
    if lineage.lineage.source_capability == "bounded-preview" {
        return Ok(true);
    }
    if request.intent == ReadIntent::ContentRead {
        return Err(StructureAnalysisError::UnsupportedFeature {
            adapter: CEPHFS_ADAPTER_ID.into(),
            feature: "bounded-preview".into(),
        });
    }
    Ok(false)
}

fn error(error: impl std::fmt::Display) -> persistence_sqlite::DbError {
    persistence_sqlite::DbError::System(error.to_string())
}
