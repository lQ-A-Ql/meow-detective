//! Private, opt-in end-to-end source catalog/session regression.
#[path = "support/registry_browser_case.rs"]
mod fixture;
use app_services::{
    datasource_service::{detect_image_filesystem, ImageFilesystemKind},
    registry_browser_service::*,
};
use artifacts_windows::registry::browser::HiveSnapshot;
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    file_repo::FileRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf, sync::Arc, time::Instant};
use transport::dto::*;

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE private Windows E01"]
fn registry_browser_reads_real_hives_and_hot_navigation_preserves_source_bytes() {
    let image = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE");
    let before = std::fs::metadata(&image).unwrap();
    let mut reader = E01Reader::open(&image).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let (fs, offset) = probe
        .candidates
        .iter()
        .filter(|c| matches!(c.kind, ImageFilesystemKind::Ntfs))
        .find_map(|candidate| {
            let fs = fs_ntfs::NtfsReader::open(
                Box::new(E01Reader::open(&image).unwrap()),
                candidate.offset,
            )
            .unwrap();
            fs.preview_file("Windows/System32/config/SOFTWARE")
                .is_ok()
                .then_some((fs, candidate.offset))
        })
        .expect("Windows NTFS partition");
    let case = fixture::BrowserCase::new();
    let registry = RegistryBrowserRegistry::default();
    assert!(case.temp.path().exists());
    case.active.with_conn(|conn| {
        let source=case.source(conn,"sample-source",&image,domain::DataSourceKind::E01);
        PartitionRepo::new(&source).insert_batch(&[DataSourcePartitionRecord {id:"sample-partition".into(),data_source_id:"sample-source".into(),partition_index:0,name:"Windows".into(),kind_label:"NTFS".into(),status:"ready".into(),offset,length:0,filesystem:Some("NTFS".into()),type_guid:None,unlock_hint:None,lvm_vg_uuid:None,lvm_vg_name:None,lvm_lv_uuid:None,lvm_lv_name:None,lvm_pv_offsets_json:None,lvm_pv_sources_json:None}]).unwrap();
        let mut paths=vec!["Windows/System32/config/SYSTEM".to_string(),"Windows/System32/config/SOFTWARE".to_string(),"Windows/System32/config/SAM".to_string(),"Windows/System32/config/SECURITY".to_string(),"Windows/System32/config/DEFAULT".to_string(),"Windows/appcompat/Programs/Amcache.hve".to_string()];
        for user in fs.list_children("Users").unwrap().into_iter().filter(|c|c.is_dir) {
            paths.push(format!("Users/{}/NTUSER.DAT",user.name));paths.push(format!("Users/{}/AppData/Local/Microsoft/Windows/UsrClass.dat",user.name));
        }
        let mut present=Vec::new();
        for path in paths {
            let Ok(file)=fs.preview_file(&path) else {continue};
            let id=format!("mft:0:{}",file.inode());
            let mut bytes=Vec::new();fs.open_file(&path).unwrap().read_to_end(&mut bytes).unwrap();
            fixture::entry(&source,"sample-source",&id,&format!("[P0]/{path}"),bytes.len() as u64);
            FileRepo::new(&source).set_partition_index_by_id(&domain::FileEntryId(id.clone()),0).unwrap();
            present.push((id,path,bytes));
        }
        assert!(present.len()>=6);
        let ctx=case.context(conn);let mut all=Vec::new();let mut cursor=None;
        loop {
            let page=list_hives(&registry,&ctx,RegistryHiveListRequestDto {data_source_id:"sample-source".into(),limit:2,cursor}).unwrap();all.extend(page.hives);cursor=page.next_cursor;if cursor.is_none(){break;}
        }
        assert_eq!(all.len(),present.len());
        let mut report=Vec::new();
        for (id,path,bytes) in present {
            let started=Instant::now();
            let session=open_hive(&registry,&ctx,RegistryHiveOpenRequestDto {file_id:format!("ds:sample-source:{id}"),view:RegistryHiveViewDto::Base}).unwrap();
            let cold_ms=started.elapsed().as_millis();
            let baseline=HiveSnapshot::open(Arc::new(bytes.clone())).unwrap();
            assert_eq!(session.root_key.name,baseline.root_key().unwrap().name);
            let children=list_subkeys(&registry,&ctx,RegistrySubkeysRequestDto {session_id:session.session_id.clone(),key_id:session.root_key.key_id.clone(),limit:200,cursor:None}).unwrap();
            assert_eq!(children.coverage.declared_count,baseline.root_key().unwrap().subkey_count as u64);
            let target=if path.ends_with("/SOFTWARE") {"Microsoft\\Windows NT\\CurrentVersion"}
                else if path.ends_with("/SYSTEM") {"ControlSet001\\Control\\Session Manager\\AppCompatCache"} else{""};
            let hot=Instant::now();let reads=registry.stats().unwrap().hive_reads;
            let mut compared=0;
            let mut oracle=Vec::new();
            for pass in 0..3 {
                let key=resolve_key(&registry,&ctx,RegistryKeyResolveRequestDto {session_id:session.session_id.clone(),key_path:target.into()}).unwrap();
                let values=list_values(&registry,&ctx,RegistryValuesRequestDto {session_id:session.session_id.clone(),key_id:key.key_id.clone(),limit:200,cursor:None}).unwrap();
                if path.ends_with("/SOFTWARE") {assert!(values.values.iter().any(|v|v.name=="ProductName"));}
                for value in values.values.iter().filter(|v|v.read_state!=RegistryItemReadStateDto::Corrupt) {
                    let raw=read_value_range(&registry,&ctx,RegistryValueRangeRequestDto {session_id:session.session_id.clone(),key_id:key.key_id.clone(),value_id:value.value_id.clone(),offset:0,length:65536}).unwrap();
                    let expected=baseline.value_range(&baseline.value(value.cell_offset).unwrap(),0,65536).unwrap();assert_eq!(raw.bytes,expected.bytes);compared+=1;
                    if pass==0 {
                        let mut full=raw.bytes.clone();
                        while (full.len() as u64)<value.byte_length {
                            let next=read_value_range(&registry,&ctx,RegistryValueRangeRequestDto {session_id:session.session_id.clone(),key_id:key.key_id.clone(),value_id:value.value_id.clone(),offset:full.len() as u64,length:65536}).unwrap();
                            assert!(!next.bytes.is_empty());full.extend(next.bytes);compared+=1;
                        }
                        oracle.push(serde_json::json!({"name":value.name,"typeCode":value.type_code,"byteLength":value.byte_length,"rawHex":hex::encode(&raw.bytes),"payloadSha256":hex::encode(Sha256::digest(&full))}));
                    }
                }
            }
            assert_eq!(registry.stats().unwrap().hive_reads,reads);
            let file=format!("hive-{}.bin",report.len());
            if let Some(dir)=std::env::var_os("FORENSICS_REGISTRY_BROWSER_ORACLE_DIR") {std::fs::create_dir_all(&dir).unwrap();use std::io::Write;std::fs::OpenOptions::new().create_new(true).write(true).open(PathBuf::from(dir).join(&file)).unwrap().write_all(&bytes).unwrap();}
            report.push(serde_json::json!({"hive":path,"oracleFile":file,"targetKey":target,"values":oracle,"bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(&bytes)),"rootName":session.root_key.name,"rootChildren":children.coverage.declared_count,"coldMs":cold_ms,"threeHotPassesMs":hot.elapsed().as_millis(),"rangeComparisons":compared,"diagnostics":session.diagnostics}));
            close_hive(&registry,&case.active.meta.id.0,RegistryHiveCloseRequestDto {session_id:session.session_id}).unwrap();
        }
        assert_eq!(registry.stats().unwrap().session_count,0);
        let output=serde_json::to_string_pretty(&report).unwrap();eprintln!("{output}");
        if let Some(output_path)=std::env::var_os("FORENSICS_REGISTRY_BROWSER_REPORT") {use std::io::Write;std::fs::OpenOptions::new().create_new(true).write(true).open(output_path).unwrap().write_all(output.as_bytes()).unwrap();}
        Ok(())
    }).unwrap();
    let after = std::fs::metadata(&image).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
}
