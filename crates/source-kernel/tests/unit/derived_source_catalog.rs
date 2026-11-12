use super::*;

#[test]
fn loads_only_current_catalog_identity() {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    let fingerprint = catalog_fingerprint("lineage-1");
    let manifest = serde_json::json!({
        "materializerVersion": CATALOG_MATERIALIZER_VERSION,
        "inputFingerprint": fingerprint,
    });
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES ('derived.catalog.manifest', ?1)",
            [manifest.to_string()],
        )
        .expect("insert manifest");

    assert_eq!(
        load_catalog_fingerprint(&connection).expect("load identity"),
        Some(fingerprint)
    );

    connection
        .execute(
            "UPDATE source_meta SET value = ?1 WHERE key = 'derived.catalog.manifest'",
            [serde_json::json!({
                "materializerVersion": CATALOG_MATERIALIZER_VERSION + 1,
                "inputFingerprint": "stale",
            })
            .to_string()],
        )
        .expect("make manifest stale");
    assert_eq!(
        load_catalog_fingerprint(&connection).expect("load stale identity"),
        None
    );
}
