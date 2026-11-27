use crate::migrations::runner::{run_migrations, MIGRATIONS};

#[test]
fn migration_preserves_existing_storage_relations_and_memberships() {
    let conn = crate::open_in_memory().unwrap();
    let boundary = MIGRATIONS
        .iter()
        .position(|(name, _)| *name == "0072_storage_object_versions")
        .unwrap();
    run_migrations(&conn, &MIGRATIONS[..boundary]).unwrap();
    conn.execute_batch("INSERT INTO cases(id,name) VALUES('case','C');
        INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES('source','case','S','raw','s.raw');
        INSERT INTO storage_objects(id,case_id,object_kind,name) VALUES('pv','case','lvm_physical_volume','PV'),('vg','case','volume_group','VG');
        INSERT INTO storage_object_memberships(storage_object_id,data_source_id,role) VALUES('pv','source','physical');
        INSERT INTO storage_relations(source_object_id,target_object_id,relation_kind) VALUES('vg','pv','contains');").unwrap();
    conn.execute_batch("INSERT INTO provenance_assertions(id,case_id,subject_domain,subject_id,relation_kind,confidence,basis,parser,parser_version)
        VALUES('assertion','case','storage','pv','derived_from','proven','native','fs-lvm','1');
        INSERT INTO analysis_object_links(analysis_object_id,analysis_kind,storage_object_id,relation_kind,provenance_assertion_id)
        VALUES('file','file_entry','pv','stored_in','assertion');").unwrap();
    run_migrations(&conn, MIGRATIONS).unwrap();
    for table in [
        "storage_object_memberships",
        "storage_relations",
        "analysis_object_links",
    ] {
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
