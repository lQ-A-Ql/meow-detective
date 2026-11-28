mod capability;
mod catalog;
mod error;
mod lineage;
mod materialization;
mod model;
mod presence_gate;
mod preview;
mod range_reader;
pub use range_reader::CephFsEvidenceReader;
mod projection;
mod projection_validation;
mod publication;
mod recovery;
mod registration;
mod source_build;

pub use error::{CephFsSourceError, CephFsSourceResult};
pub use materialization::load_ready_summary as verify_cephfs_published_source;
pub use materialization::materialize_cephfs_source;
pub use model::{
    CephFsSourceCapability, CephFsSourceMaterializationRequest, MaterializedCephFsSource,
};
pub use preview::{open_cephfs_file_reader, CephFsFileReadRequest, PreparedCephFsFileReader};
