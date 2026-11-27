use domain::{CaseId, DataSource};

use crate::cluster_service::structure_publication::{
    StructurePublicationError, StructurePublicationValidator,
};

pub(super) struct LvmValidator<'a> {
    pub case_root: &'a std::path::Path,
}

impl StructurePublicationValidator for LvmValidator<'_> {
    fn validate(
        &self,
        conn: &rusqlite::Connection,
        case_id: &CaseId,
        source: &DataSource,
        fingerprint: &str,
    ) -> Result<(), StructurePublicationError> {
        let plan =
            evidence_access::lvm::read_lvm_descriptor(&source.source_path).map_err(map_error)?;
        if evidence_access::lvm::lvm_volume_fingerprint(&plan).map_err(map_error)? != fingerprint {
            return Err(StructurePublicationError::InvalidPublication);
        }
        super::inputs::validate(conn, source).map_err(map_error)?;
        super::object_version::validate(conn, &case_id.0, &source.id.0, fingerprint)
            .map_err(map_error)?;
        let _ = self.case_root;
        Ok(())
    }
}

fn map_error(error: evidence_access::lvm::LvmPlanError) -> StructurePublicationError {
    match error {
        evidence_access::lvm::LvmPlanError::Database(error) => {
            StructurePublicationError::Database(error)
        }
        evidence_access::lvm::LvmPlanError::Storage(error) => {
            StructurePublicationError::Storage(error)
        }
        evidence_access::lvm::LvmPlanError::Generation(error) => {
            StructurePublicationError::Generation(error)
        }
        evidence_access::lvm::LvmPlanError::Io(error) => StructurePublicationError::Io(error),
        evidence_access::lvm::LvmPlanError::Cancelled => StructurePublicationError::Cancelled,
        other => StructurePublicationError::Validation(other.to_string()),
    }
}
