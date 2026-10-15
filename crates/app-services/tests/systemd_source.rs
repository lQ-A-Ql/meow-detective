#[path = "support/evidence_case.rs"]
mod fixture;
use app_services::analysis_service::run_analysis_extraction_with_reader_limits;
use std::collections::HashMap;
#[test]
fn source_analysis_compiles_links_and_dropins_and_invalidates_multi_file_checkpoints() {
    let case = fixture::EvidenceCase::new();
    let evidence = case.temp.path().join("linux-units");
    std::fs::create_dir(&evidence).unwrap();
    case.active.with_conn(|conn| {
        let source=case.source(conn,"linux-services",&evidence,domain::DataSourceKind::LogicalDirectory);
        let rows=[
            ("base","[P0]/usr/lib/systemd/system/worker.service",false,"[Unit]\nDescription=Worker\n[Service]\nExecStart=/worker\n[Install]\nWantedBy=multi-user.target\n"),
            ("enable","[P0]/etc/systemd/system/multi-user.target.wants/worker.service",true,"/usr/lib/systemd/system/worker.service"),
            ("drop","[P0]/etc/systemd/system/worker.service.d/20-user.conf",false,"[Service]\nUser=first\n"),
            ("blocked","[P0]/etc/systemd/system/blocked.service",true,"/dev/null"),
            ("blocked-vendor","[P0]/usr/lib/systemd/system/blocked.service",false,"[Service]\nExecStart=/vendor\n"),
        ];
        let mut bytes=HashMap::new();
        for (id,path,link,text) in rows {
            fixture::entry(&source,"linux-services",id,path,text.len() as u64);
            source.execute("UPDATE file_entries SET partition_index=0,unix_mode=?2 WHERE id=?1",rusqlite::params![id,if link{0o120777}else{0o100644}]).unwrap();
            bytes.insert(id.to_owned(),text.as_bytes().to_vec());
        }
        let run=|values:&HashMap<String,Vec<u8>>|run_analysis_extraction_with_reader_limits(&source,&case.active.meta.id.0,domain::DataSourcePlatform::Linux,&["LinuxArtifacts"],|id,maximum|->Result<Box<dyn std::io::Read>,std::io::Error>{let value=values.get(&id.0).expect("source-bound dependency");Ok(Box::new(std::io::Cursor::new(value[..value.len().min(maximum)].to_vec())))}).unwrap();
        run(&bytes);
        let records=persistence_sqlite::repositories::artifact_repo::ArtifactRepo::new(&source).list_by_family(Some("LinuxSystemConfig"))?;
        let worker=records.iter().find(|row|row.attrs.get("name").and_then(|value|value.as_str())==Some("worker.service")).expect("compiled worker");
        assert_eq!(worker.attrs.get("state").and_then(|value|value.as_str()),Some("enabled"));assert_eq!(worker.attrs.get("user").and_then(|value|value.as_str()),Some("first"));
        assert_eq!(worker.attrs.get("sourceFiles").and_then(|value|value.as_array()).unwrap().len(),3);
        assert_eq!(records.iter().filter(|row|row.extractor_id.as_deref()==Some("linux.systemd_unit")).count(),2);
        let marker=persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(&source).read("systemd.inventory:linux-services")?.unwrap();assert!(serde_json::from_str::<serde_json::Value>(&marker).unwrap()["complete"].as_bool().unwrap());
        bytes.insert("drop".into(),b"[Service]\nUser=other\n".to_vec());run(&bytes);
        let changed=persistence_sqlite::repositories::artifact_repo::ArtifactRepo::new(&source).list_by_family(Some("LinuxSystemConfig"))?;
        let worker=changed.iter().find(|row|row.attrs.get("name").and_then(|value|value.as_str())==Some("worker.service")).unwrap();assert_eq!(worker.attrs.get("user").and_then(|value|value.as_str()),Some("other")); Ok(())
    }).unwrap();
}
