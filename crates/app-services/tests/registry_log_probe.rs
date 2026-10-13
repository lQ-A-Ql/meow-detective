//! Opt-in private evidence inventory for registry log replay validation.
use app_services::datasource_service::{detect_image_filesystem, ImageFilesystemKind};
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use serde_json::json;
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE private Windows E01"]
fn inventory_real_registry_logs() {
    let fixture = std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_E01_FIXTURE");
    let output = std::env::var_os("FORENSICS_REGISTRY_LOG_OUTPUT")
        .map(PathBuf::from)
        .expect("set FORENSICS_REGISTRY_LOG_OUTPUT");
    std::fs::create_dir_all(&output).unwrap();
    let mut image = E01Reader::open(&fixture).unwrap();
    let probe = detect_image_filesystem(&mut image).unwrap();
    let fs = probe
        .candidates
        .iter()
        .filter(|c| matches!(c.kind, ImageFilesystemKind::Ntfs))
        .find_map(|candidate| {
            let fs = fs_ntfs::NtfsReader::open(
                Box::new(E01Reader::open(&fixture).unwrap()),
                candidate.offset,
            )
            .unwrap();
            fs.preview_file("Windows/System32/config/SOFTWARE")
                .is_ok()
                .then_some(fs)
        })
        .unwrap();
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
    for (index, path) in paths.into_iter().enumerate() {
        if fs.preview_file(&path).is_err() {
            continue;
        }
        let mut files = Vec::new();
        for suffix in ["", ".LOG1", ".LOG2"] {
            let candidate = format!("{path}{suffix}");
            let Ok(mut file) = fs.open_file(&candidate) else {
                continue;
            };
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            let name = format!("hive-{index}{}.bin", suffix.to_lowercase());
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join(&name))
                .unwrap()
                .write_all(&bytes)
                .unwrap();
            let field = |at: usize| {
                bytes
                    .get(at..at + 4)
                    .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            };
            files.push(json!({"file":name,"length":bytes.len(),"magic":hex::encode(&bytes[..4]),"primary":field(4),"secondary":field(8),"version":field(24),"type":field(28),"root":field(36),"binsLength":field(40),"cluster":field(44),"nameHex":hex::encode(&bytes[48..112]),"rmId":hex::encode(&bytes[112..128]),"logId":hex::encode(&bytes[128..144]),"timestamp":hex::encode(&bytes[12..20]),"entryMagic":bytes.get(512..516).map(hex::encode)}));
        }
        report.push(json!({"hiveIndex":index,"files":files}));
    }
    let report = serde_json::to_vec_pretty(&report).unwrap();
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("inventory.json"))
        .unwrap()
        .write_all(&report)
        .unwrap();
    eprintln!("{}", String::from_utf8(report).unwrap());
}
