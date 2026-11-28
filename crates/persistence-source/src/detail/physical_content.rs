use domain::structure::{PhysicalContentBinding, SourceRevisionRef};
use persistence_fjall::{encode_key, KeyPart, KeyspaceName, PersistMode, VersionedEnvelope};
use persistence_sqlite::{DbError, DbResult};
use sha2::{Digest, Sha256};

pub fn write_physical_content(
    conn: &rusqlite::Connection,
    binding: &PhysicalContentBinding,
    manifest: &[u8],
) -> DbResult<()> {
    validate_owner(conn, &binding.source)?;
    if binding.backing_paths.is_empty()
        || binding.manifest_sha256 != hex::encode(Sha256::digest(manifest))
    {
        return Err(DbError::System(
            "physical content identity is invalid".into(),
        ));
    }
    let store = super::open_store_for_write(conn)?
        .ok_or_else(|| DbError::System("physical content generation is missing".into()))?;
    let snapshot = store.database().read_snapshot();
    for (family, payload) in [
        ("physical.content.manifest", manifest.to_vec()),
        (
            "physical.content.binding",
            serde_json::to_vec(binding).map_err(error)?,
        ),
    ] {
        let key = key(family, &binding.source)?;
        let envelope = VersionedEnvelope::new(
            family,
            1,
            binding.source.source_id.clone(),
            binding.source.source_id.clone(),
            binding.source.revision,
            payload,
        );
        if let Some(prior) = store
            .read_envelope(&snapshot, KeyspaceName::Meta, &key)
            .map_err(error)?
        {
            if prior.payload != envelope.payload || prior.revision != envelope.revision {
                return Err(DbError::System("physical content identity changed".into()));
            }
        } else {
            store
                .write_envelope(KeyspaceName::Meta, key, &envelope, PersistMode::SyncAll)
                .map_err(error)?;
        }
    }
    Ok(())
}

pub fn read_physical_content_binding(
    conn: &rusqlite::Connection,
    source: &SourceRevisionRef,
) -> DbResult<PhysicalContentBinding> {
    let binding: PhysicalContentBinding =
        serde_json::from_slice(&read(conn, source, "physical.content.binding")?).map_err(error)?;
    if binding.source != *source || binding.backing_paths.is_empty() {
        return Err(DbError::System(
            "physical content binding differs from revision".into(),
        ));
    }
    Ok(binding)
}

pub fn read_physical_content(
    conn: &rusqlite::Connection,
    source: &SourceRevisionRef,
) -> DbResult<(PhysicalContentBinding, Vec<u8>)> {
    let binding = read_physical_content_binding(conn, source)?;
    let manifest = read(conn, source, "physical.content.manifest")?;
    if binding.manifest_sha256 != hex::encode(Sha256::digest(&manifest)) {
        return Err(DbError::System(
            "physical content manifest digest changed".into(),
        ));
    }
    Ok((binding, manifest))
}

fn read(
    conn: &rusqlite::Connection,
    source: &SourceRevisionRef,
    family: &str,
) -> DbResult<Vec<u8>> {
    validate_owner(conn, source)?;
    let store = super::require_source_store(conn)?;
    let envelope = store
        .read_envelope(
            &store.database().read_snapshot(),
            KeyspaceName::Meta,
            &key(family, source)?,
        )
        .map_err(error)?
        .ok_or_else(|| DbError::System("physical content identity is missing".into()))?;
    if envelope.record_kind != family
        || envelope.source_id != source.source_id
        || envelope.revision != source.revision
    {
        return Err(DbError::System(
            "physical content record identity differs".into(),
        ));
    }
    Ok(envelope.payload)
}

fn validate_owner(conn: &rusqlite::Connection, source: &SourceRevisionRef) -> DbResult<()> {
    if super::source_id(conn)?.as_deref() != Some(source.source_id.as_str()) {
        return Err(DbError::System("physical content owner differs".into()));
    }
    Ok(())
}

fn key(family: &str, source: &SourceRevisionRef) -> DbResult<Vec<u8>> {
    encode_key(
        family,
        &[
            KeyPart::Text(source.source_id.clone()),
            KeyPart::U64(source.revision),
        ],
    )
    .map_err(error)
}

fn error(error: impl std::fmt::Display) -> DbError {
    DbError::System(error.to_string())
}
