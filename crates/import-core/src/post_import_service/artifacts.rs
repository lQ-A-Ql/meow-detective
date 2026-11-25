use super::{CapabilityOutcome, CapabilityWork};
use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::sync::{atomic::AtomicBool, Arc};

mod android;
mod host;
mod linux_facts;

pub(super) fn run(
    work: &CapabilityWork,
    conn: &rusqlite::Connection,
    cancel: &Arc<AtomicBool>,
) -> CapabilityOutcome {
    match platform(conn, &work.row.data_source_id) {
        Ok(DataSourcePlatform::Android) => android::run(work, conn, cancel),
        Ok(platform @ (DataSourcePlatform::Windows | DataSourcePlatform::Linux)) => {
            host::run(work, conn, cancel, platform)
        }
        Ok(DataSourcePlatform::Unknown) => CapabilityOutcome::terminal("notApplicable"),
        Err(error) => {
            tracing::error!(error = %error, "post-import source platform unavailable");
            CapabilityOutcome::failed("SOURCE_UNAVAILABLE")
        }
    }
}

fn platform(
    conn: &rusqlite::Connection,
    source: &str,
) -> Result<DataSourcePlatform, persistence_sqlite::DbError> {
    let storage = DataSourceRepo::new(conn)
        .find_storage(&DataSourceId(source.into()))?
        .ok_or_else(|| {
            persistence_sqlite::DbError::System("source storage record missing".into())
        })?;
    DataSourcePlatform::from_storage_str(Some(&storage.platform))
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
}
