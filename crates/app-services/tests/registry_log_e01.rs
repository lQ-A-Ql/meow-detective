//! Private opt-in log replay validation; controlled copies are explicitly labelled.
#[path = "support/registry_browser_case.rs"]
mod fixture;
use app_services::{
    datasource_service::{detect_image_filesystem, ImageFilesystemKind},
    registry_browser_service::*,
};
use artifacts_windows::registry::log_replay::{
    prepare_log_replay, HiveLog, LogSlot, RecoveryState,
};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    file_repo::FileRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use transport::dto::*;

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE private Windows E01"]
fn validates_original_clean_hives_and_controlled_real_log_recovery() {
    let image = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE");
    let output = std::env::var_os("FORENSICS_REGISTRY_LOG_VERIFICATION")
        .map(PathBuf::from)
        .expect("set FORENSICS_REGISTRY_LOG_VERIFICATION");
    std::fs::create_dir_all(&output).unwrap();
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
        .unwrap();
    let case = fixture::BrowserCase::new();
    assert!(case.temp.path().exists());
    let registry = RegistryBrowserRegistry::default();
    let mut paths = vec![
        "Windows/System32/config/SYSTEM".into(),
        "Windows/System32/config/SOFTWARE".into(),
        "Windows/System32/config/SAM".into(),
        "Windows/System32/config/SECURITY".into(),
        "Windows/System32/config/DEFAULT".into(),
        "Windows/appcompat/Programs/Amcache.hve".into(),
    ];
    for user in fs
        .list_children("Users")
        .unwrap()
        .into_iter()
        .filter(|c| c.is_dir)
    {
        paths.push(format!("Users/{}/NTUSER.DAT", user.name));
        paths.push(format!(
            "Users/{}/AppData/Local/Microsoft/Windows/UsrClass.dat",
            user.name
        ));
    }
    let mut report = Vec::new();
    case.active.with_conn(|conn| {
        let source=case.source(conn,"real-source",&image,domain::DataSourceKind::E01);
        PartitionRepo::new(&source).insert_batch(&[DataSourcePartitionRecord {id:"real-partition".into(),data_source_id:"real-source".into(),partition_index:0,name:"Windows".into(),kind_label:"NTFS".into(),status:"ready".into(),offset,length:0,filesystem:Some("NTFS".into()),type_guid:None,unlock_hint:None,lvm_vg_uuid:None,lvm_vg_name:None,lvm_lv_uuid:None,lvm_lv_name:None,lvm_pv_offsets_json:None,lvm_pv_sources_json:None}]).unwrap();
        for (index,path) in paths.iter().enumerate() {
            let Ok(file)=fs.preview_file(path) else {continue};let id=format!("mft:0:{}",file.inode());
            let mut bytes=Vec::new();fs.open_file(path).unwrap().read_to_end(&mut bytes).unwrap();
            fixture::entry(&source,"real-source",&id,&format!("[P0]/{path}"),bytes.len() as u64);FileRepo::new(&source).set_partition_index_by_id(&domain::FileEntryId(id.clone()),0).unwrap();
            let mut logs=Vec::new();
            for (slot,suffix) in [(LogSlot::Log1,"LOG1"),(LogSlot::Log2,"LOG2")] {
                let log_path=format!("{path}.{suffix}");let Ok(file)=fs.preview_file(&log_path) else {continue};let log_id=format!("mft:0:{}",file.inode());
                let parent=log_path.rsplit_once('/').unwrap().0;
                let size=fs.list_children(parent).unwrap().into_iter().find(|node|node.name.eq_ignore_ascii_case(log_path.rsplit('/').next().unwrap())).unwrap().size;
                let mut data=Vec::new();if size>0 {fs.open_file(&log_path).unwrap().read_to_end(&mut data).unwrap();}
                fixture::entry(&source,"real-source",&log_id,&format!("[P0]/{log_path}"),data.len() as u64);FileRepo::new(&source).set_partition_index_by_id(&domain::FileEntryId(log_id),0).unwrap();logs.push((slot,data));
            }
            let ctx=case.context(conn);let opened=open_hive(&registry,&ctx,RegistryHiveOpenRequestDto {file_id:format!("ds:real-source:{id}"),view:RegistryHiveViewDto::Recovered}).unwrap();let details=opened.recovery.unwrap();
            assert_eq!(details.status,RegistryRecoveryStatusDto::NotNeeded);assert_eq!(details.base_sha256,details.result_sha256);assert_eq!(details.applied_entries,0);
            assert_eq!(details.base_sha256.as_deref(),Some(hex::encode(Sha256::digest(&bytes)).as_str()));
            let reads=registry.stats().unwrap();list_subkeys(&registry,&ctx,RegistrySubkeysRequestDto {session_id:opened.session_id.clone(),key_id:opened.root_key.key_id,limit:100,cursor:None}).unwrap();assert_eq!(registry.stats().unwrap().hive_reads,reads.hive_reads);assert_eq!(registry.stats().unwrap().log_reads,reads.log_reads);
            close_hive(&registry,&case.active.meta.id.0,RegistryHiveCloseRequestDto {session_id:opened.session_id}).unwrap();
            let prefix=format!("hive-{index}");write_new(&output.join(format!("{prefix}.base.bin")),&bytes);
            for (slot,data) in &logs {write_new(&output.join(format!("{prefix}.{}.bin",if *slot==LogSlot::Log1 {"log1"}else{"log2"})),data);}
            let inputs: Vec<_>=logs.iter().map(|(slot,data)|HiveLog {slot:*slot,bytes:Some(data)}).collect();
            let mut simulation=None;
            if let Some(earliest)=logs.iter().filter(|(_,data)|data.len()>=512).map(|(_,data)|u32::from_le_bytes(data[4..8].try_into().unwrap())).min() {
                let mut dirty=bytes.clone();dirty[4..8].copy_from_slice(&earliest.wrapping_add(1).to_le_bytes());dirty[8..12].copy_from_slice(&earliest.to_le_bytes());seal(&mut dirty);
                write_new(&output.join(format!("{prefix}.controlled-dirty.bin")),&dirty);
                let plan=prepare_log_replay(&dirty,&inputs,128*1024*1024).unwrap();let recovered=plan.apply(dirty).unwrap();
                assert!(matches!(recovered.report.state,RecoveryState::Recovered|RecoveryState::Partial));write_new(&output.join(format!("{prefix}.controlled-recovered.bin")),&recovered.bytes);
                simulation=Some(serde_json::json!({"controlledHeaderChangeOnly":true,"entries":recovered.report.applied_entries,"pages":recovered.report.applied_pages,"resultSequence":recovered.report.result_sequence,"sha256":hex::encode(Sha256::digest(&recovered.bytes)),"status":format!("{:?}",recovered.report.state)}));
            }
            report.push(serde_json::json!({"prefix":prefix,"originalRecovery":details,"controlledSimulation":simulation}));
        }
        assert_eq!(registry.stats().unwrap().session_count,0);Ok(())
    }).unwrap();
    assert!(report.len() >= 9);
    write_new(
        &output.join("verification.json"),
        &serde_json::to_vec_pretty(&report).unwrap(),
    );
    let after = std::fs::metadata(&image).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    eprintln!(
        "Validated {} original hives; controlled recovery files are derived copies",
        report.len()
    );
}
fn seal(bytes: &mut [u8]) {
    let value = bytes[..508]
        .chunks_exact(4)
        .fold(0u32, |s, b| s ^ u32::from_le_bytes(b.try_into().unwrap()));
    let value = match value {
        0 => 1,
        u32::MAX => u32::MAX - 1,
        _ => value,
    };
    bytes[508..512].copy_from_slice(&value.to_le_bytes());
}
fn write_new(path: &Path, data: &[u8]) {
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap()
        .write_all(data)
        .unwrap();
}
