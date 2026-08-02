use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub(crate) const PROCESSING_PHASE_VERSION: u32 = 1;
pub(crate) const CATALOG_MATERIALIZER_VERSION: u32 = 3;
pub(crate) const CATALOG_SCHEMA_DEPENDENCY: &str = "source_015_ceph_bluestore_rbd_header_context";
pub(crate) const CATALOG_POLICY_VERSION: &str = "rbd-filesystem-catalog-v2-xfs-macb";

const SOURCE_META_KEY: &str = "derived.catalog.manifest";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCatalogIdentity {
    materializer_version: u32,
    input_fingerprint: String,
}

pub(crate) fn catalog_fingerprint(lineage_fingerprint: &str) -> String {
    processing_phase_fingerprint(
        lineage_fingerprint,
        "catalog",
        CATALOG_SCHEMA_DEPENDENCY,
        CATALOG_POLICY_VERSION,
    )
}

pub(crate) fn processing_phase_fingerprint(
    seed: &str,
    phase: &str,
    schema_dependency: &str,
    policy_version: &str,
) -> String {
    let mut hasher = Sha256::new();
    update_field(&mut hasher, b"derived-rbd-processing-phase");
    update_field(&mut hasher, seed.as_bytes());
    update_field(&mut hasher, schema_dependency.as_bytes());
    update_field(&mut hasher, phase.as_bytes());
    update_field(&mut hasher, &PROCESSING_PHASE_VERSION.to_le_bytes());
    update_field(&mut hasher, policy_version.as_bytes());
    hex::encode(hasher.finalize())
}

pub(crate) fn load_catalog_fingerprint(
    connection: &Connection,
) -> Result<Option<String>, persistence_sqlite::DbError> {
    let stored = connection
        .query_row(
            "SELECT value FROM source_meta WHERE key = ?1",
            [SOURCE_META_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(persistence_sqlite::DbError::from)?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let identity: StoredCatalogIdentity = serde_json::from_str(&stored).map_err(|error| {
        persistence_sqlite::DbError::System(format!("Decode derived Catalog identity: {error}"))
    })?;
    Ok(
        (identity.materializer_version == CATALOG_MATERIALIZER_VERSION)
            .then_some(identity.input_fingerprint),
    )
}

fn update_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(test)]
#[path = "../tests/unit/derived_source_catalog.rs"]
mod tests;
