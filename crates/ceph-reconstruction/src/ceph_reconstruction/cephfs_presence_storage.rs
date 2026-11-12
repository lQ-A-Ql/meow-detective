use domain::DataSourceId;
use persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo;
use serde::de::DeserializeOwned;

use super::cephfs_presence::{
    CephFsMapPresenceSnapshot, CephFsMdsMapPresenceSnapshot, CephFsPresenceError,
    CephFsPresenceEvidence, FSMAP_PRESENCE_KEY, MDSMAP_PRESENCE_KEY,
};

pub(super) fn read_presence_evidence(
    data_source_id: &DataSourceId,
    connection: &rusqlite::Connection,
) -> Result<CephFsPresenceEvidence, CephFsPresenceError> {
    let repo = SourceMetaRepo::new(connection);
    let (fsmap, fsmap_error) =
        read_snapshot::<CephFsMapPresenceSnapshot>(&repo, FSMAP_PRESENCE_KEY)?;
    let (mdsmap, mdsmap_error) =
        read_snapshot::<CephFsMdsMapPresenceSnapshot>(&repo, MDSMAP_PRESENCE_KEY)?;
    Ok(CephFsPresenceEvidence {
        source_id: data_source_id.0.clone(),
        fsmap,
        mdsmap,
        fsmap_error,
        mdsmap_error,
    })
}

fn read_snapshot<T: DeserializeOwned>(
    repo: &SourceMetaRepo<'_>,
    key: &str,
) -> Result<(Option<T>, Option<String>), CephFsPresenceError> {
    let Some(value) = repo.read(key)? else {
        return Ok((None, None));
    };
    match serde_json::from_str(&value) {
        Ok(snapshot) => Ok((Some(snapshot), None)),
        Err(error) => Ok((None, Some(error.to_string()))),
    }
}
