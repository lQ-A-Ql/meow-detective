//! Opt-in inventory only. No workstation paths and no private dependencies in CI.
use evidence_core::FileSystemReader;
use serde_json::json;
use std::io::{Read, Seek, SeekFrom};

#[test]
#[ignore = "requires explicit FORENSICS_IMPORT_FIXTURE; private input inventory outside timings"]
fn inspect_import_benchmark_input() {
    let path = std::path::PathBuf::from(
        std::env::var_os("FORENSICS_IMPORT_FIXTURE").expect("set FORENSICS_IMPORT_FIXTURE"),
    );
    let is_e01 = path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("e01"));
    let kind = if is_e01 {
        domain::DataSourceKind::E01
    } else {
        domain::DataSourceKind::Raw
    };
    let factory = app_services::source_runtime::open_factory(&path, &kind).unwrap();
    let mut reader = factory.open_reader().unwrap();
    let logical_size = reader.info().size;
    let probe = app_services::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let mut volumes = Vec::new();
    for candidate in probe.candidates {
        if candidate.kind == app_services::datasource_service::ImageFilesystemKind::Fat {
            volumes.push(match fs_fat::FatReader::open(factory.open_reader().unwrap(), candidate.offset) {
                Ok(fat) => fat_inventory(&fat, candidate.offset),
                Err(error) => json!({"filesystem":"FatSignatureOnly","offset":candidate.offset,
                    "ntfsBenchmark":"not-applicable","readerStatus":"rejected","readerErrorKind":format!("{:?}",error.kind()),
                    "readerError":error.to_string(),"directoryInventory":"unavailable"}),
            });
            continue;
        }
        if candidate.kind != app_services::datasource_service::ImageFilesystemKind::Ntfs {
            volumes.push(json!({"filesystem":format!("{:?}",candidate.kind),"offset":candidate.offset,"ntfsBenchmark":"not-applicable"}));
            continue;
        }
        let mut boot_reader = factory.open_reader().unwrap();
        boot_reader
            .seek(SeekFrom::Start(candidate.offset + 11))
            .unwrap();
        let mut bps = [0; 2];
        boot_reader.read_exact(&mut bps).unwrap();
        let ntfs =
            fs_ntfs::NtfsReader::open(factory.open_reader().unwrap(), candidate.offset).unwrap();
        let mft = ntfs.inspect_file_by_inode(0).unwrap();
        let record_size = mft.record_size as usize;
        let mft_size = ntfs.file_size_by_inode(0).unwrap().unwrap();
        let mft_runs: usize = mft
            .attributes
            .iter()
            .filter(|a| a.attribute_type == 0x80 && a.name.is_none())
            .map(|a| a.data_runs.len())
            .sum();
        let inspector =
            fs_ntfs::NtfsReader::open(factory.open_reader().unwrap(), candidate.offset).unwrap();
        let mut stream = ntfs.into_file_stream_by_inode(0).unwrap();
        let mut parser =
            fs_ntfs::mft_scanner::MftRecordParser::new(record_size as u32, u16::from_le_bytes(bps));
        let mut buf = vec![0; record_size * 4096];
        let mut slots = 0u64;
        let mut valid = 0u64;
        let mut directories = 0u64;
        let mut deleted = 0u64;
        let mut external = 0u64;
        let mut inspected = 0u64;
        let mut fragmented = 0u64;
        let mut index_allocations = 0u64;
        loop {
            let mut used = 0;
            while used < buf.len() {
                let n = stream.read(&mut buf[used..]).unwrap();
                if n == 0 {
                    break;
                }
                used += n;
            }
            if used == 0 {
                break;
            }
            assert_eq!(used % record_size, 0);
            for raw in buf[..used].chunks_exact(record_size) {
                if let Some(record) = parser.parse(raw, slots) {
                    valid += 1;
                    directories += u64::from(record.is_dir);
                    deleted += u64::from(record.deleted);
                    external += u64::from(record.has_attribute_list);
                    if inspected < 128 {
                        let detail = inspector.inspect_file_by_inode(slots).unwrap();
                        inspected += 1;
                        fragmented +=
                            u64::from(detail.attributes.iter().any(|a| a.data_runs.len() > 1));
                        index_allocations +=
                            u64::from(detail.attributes.iter().any(|a| a.attribute_type == 0xa0));
                    }
                }
                slots += 1;
            }
        }
        volumes.push(json!({"filesystem":"Ntfs","offset":candidate.offset,"mftBytes":mft_size,"recordSize":record_size,
            "recordSlots":slots,"validNamedRecords":valid,"directories":directories,"deleted":deleted,"attributeListRecords":external,"mftDataRuns":mft_runs,
            "fragmentationSample":{"policy":"first 128 valid records in MFT order; not whole-volume fragmentation","records":inspected,"multipleRunRecords":fragmented,"indexAllocationRecords":index_allocations}}));
    }
    let geometry = if is_e01 {
        let plan = image_e01::E01Reader::open(&path)
            .unwrap()
            .open_plan()
            .unwrap();
        json!({"segments":plan.segment_count(),"chunks":plan.chunk_count(),"logicalBytes":plan.logical_size(),"perChunkCompression":"not exposed by reader; report actual container/logical ratio separately"})
    } else {
        json!({"logicalBytes":logical_size,"segments":1,"compression":"none"})
    };
    eprintln!(
        "[input-inventory] {}",
        json!({"schemaVersion":1,"geometry":geometry,"volumes":volumes})
    );
}

fn fat_inventory(reader: &fs_fat::FatReader, offset: u64) -> serde_json::Value {
    let mut pending = std::collections::VecDeque::from([(String::new(), 0usize)]);
    let mut visited = std::collections::HashSet::new();
    let mut files = 0u64;
    let mut directories = 0u64;
    let mut bytes = 0u64;
    while let Some((path, depth)) = pending.pop_front() {
        assert!(
            depth < 64 && visited.insert(path.clone()),
            "invalid FAT directory topology"
        );
        for node in reader.list_children(&path).unwrap() {
            assert!(
                files + directories < 1_000_000,
                "FAT inventory bound exceeded"
            );
            if node.is_dir {
                directories += 1;
                pending.push_back((node.path, depth + 1));
            } else {
                files += 1;
                bytes += node.size;
            }
        }
    }
    json!({"filesystem":reader.data_source_name(),"offset":offset,"ntfsBenchmark":"not-applicable",
        "files":files,"directoriesExcludingRoot":directories,"logicalFileBytes":bytes,
        "scope":"complete visible directory traversal with native FAT reader; deleted entries and fragmentation topology not exposed"})
}
