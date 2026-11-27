use crate::migrations::runner::{run_migrations, MIGRATIONS};

#[test]
fn migration_preserves_existing_storage_relations() {
    let conn = crate::open_in_memory().unwrap();
    let boundary = MIGRATIONS
        .iter()
        .position(|(name, _)| *name == "0072_storage_object_versions")
        .unwrap();
    run_migrations(&conn, &MIGRATIONS[..boundary]).unwrap();
    conn.execute_batch("INSERT INTO cases(id,name) VALUES('case','C');
        INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES('source','case','S','raw','s.raw');
        INSERT INTO storage_objects(id,case_id,object_kind,name) VALUES('pv','case','lvm_physical_volume','PV'),('vg','case','volume_group','VG');
        INSERT INTO storage_relations(source_object_id,target_object_id,relation_kind) VALUES('vg','pv','contains');").unwrap();
    conn.execute_batch("INSERT INTO provenance_assertions(id,case_id,subject_domain,subject_id,relation_kind,confidence,basis,parser,parser_version)
        VALUES('assertion','case','storage','pv','derived_from','proven','native','fs-lvm','1');
        INSERT INTO analysis_object_links(analysis_object_id,analysis_kind,storage_object_id,relation_kind,provenance_assertion_id)
        VALUES('file','file_entry','pv','stored_in','assertion');").unwrap();
    run_migrations(&conn, MIGRATIONS).unwrap();
    for table in ["storage_relations", "analysis_object_links"] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, u64>(0))
                .unwrap(),
            1
        );
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get::<_, u64>(0)
        })
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn structure_s01_migration_preserves_existing_object_versions() {
    let conn = crate::open_in_memory().unwrap();
    let boundary = MIGRATIONS
        .iter()
        .position(|(name, _)| *name == "0073_structure_core_s01")
        .unwrap();
    run_migrations(&conn, &MIGRATIONS[..boundary]).unwrap();
    conn.execute_batch(
        "INSERT INTO cases(id,name) VALUES('case','C');
         INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES
             ('physical','case','P','raw','p.raw'),
             ('derived','case','D','md_raid','array.json');
         INSERT INTO storage_objects(id,case_id,object_kind,name)
             VALUES('object','case','md_raid','Native MD');
         INSERT INTO storage_object_versions(
             id,storage_object_id,derived_source_id,adapter_version,
             mapping_sha256,native_versions_json
         ) VALUES(
             'version','object','derived','fs-lvm/mdraid-v2',
             'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
             '[{\"kind\":\"md\",\"metadata_version\":\"v1_2\",\"events\":2}]'
         );",
    )
    .unwrap();
    run_migrations(&conn, MIGRATIONS).unwrap();
    let row: (String, String, String, String, String) = conn
        .query_row(
            "SELECT owner_source_id, acquisition_ids_json, native_object_id_json,
                    policy_version, version_kind
             FROM storage_object_versions WHERE id='version'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row.0, "derived");
    assert_eq!(row.1, "[\"derived\"]");
    assert!(row.2.contains("\"adapter\":\"legacy\""));
    assert_eq!(row.3, "legacy-v1");
    assert_eq!(row.4, "derived_output");
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get::<_, u64>(0)
        })
        .unwrap(),
        0
    );
}
