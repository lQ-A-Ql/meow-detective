//! Explicit export of private WOF compressed bytes for an independent decoder.
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

use app_services::source_db::GlobalFileId;
use domain::{CaseMeta, FileEntryId};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, partition_repo::PartitionRepo,
};

#[test]
#[ignore = "requires a private imported case, WOF file and explicit oracle export directory"]
fn export_owned_wof_stream_for_independent_oracle() {
    let root = PathBuf::from(
        std::env::var_os("FORENSICS_MCP_CASE_ROOT").expect("set FORENSICS_MCP_CASE_ROOT"),
    );
    let id = std::env::var("FORENSICS_MCP_WOF_FILE_ID").expect("set FORENSICS_MCP_WOF_FILE_ID");
    let output_root = PathBuf::from(
        std::env::var_os("FORENSICS_WOF_ORACLE_EXPORT_DIR")
            .expect("set FORENSICS_WOF_ORACLE_EXPORT_DIR"),
    );
    let meta: CaseMeta =
        serde_json::from_slice(&std::fs::read(root.join("case.json")).unwrap()).unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let global = GlobalFileId::parse(&FileEntryId(id)).unwrap();
    let source = DataSourceRepo::new(&conn)
        .find_by_case(&meta.id)
        .unwrap()
        .into_iter()
        .find(|source| source.id == global.data_source_id)
        .unwrap();
    let local: Vec<_> = global.local_id.0.split(':').collect();
    assert_eq!(local[0], "mft");
    let index: usize = local[1].parse().unwrap();
    let inode: u64 = local[2].parse().unwrap();
    let ready = app_services::source_db::open_ready_source_read_only_by_id(
        &conn,
        &root,
        &meta.id,
        &global.data_source_id,
    )
    .unwrap();
    let partition = PartitionRepo::new(&ready.connection)
        .find_by_data_source(&global.data_source_id.0)
        .unwrap()
        .into_iter()
        .find(|partition| partition.partition_index == index as u32)
        .unwrap();
    let mut image = image_e01::E01Reader::open(&source.source_path).unwrap();
    image.seek(SeekFrom::Start(partition.offset)).unwrap();
    let mut boot = [0; 512];
    image.read_exact(&mut boot).unwrap();
    let exponent = boot[64] as i8;
    let record_size = if exponent < 0 {
        1usize << exponent.unsigned_abs()
    } else {
        exponent as usize * boot[13] as usize * u16::from_le_bytes([boot[11], boot[12]]) as usize
    };
    let filesystem = fs_ntfs::NtfsReader::open(Box::new(image), partition.offset).unwrap();
    let mut record = filesystem
        .read_file_range_by_inode(0, inode * record_size as u64, record_size)
        .unwrap();
    let usa = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    let sector = u16::from_le_bytes([boot[11], boot[12]]) as usize;
    for index in 1..usa_count {
        let replacement = [record[usa + index * 2], record[usa + index * 2 + 1]];
        record[index * sector - 2..index * sector].copy_from_slice(&replacement);
    }
    let mut position = u16::from_le_bytes([record[20], record[21]]) as usize;
    let algorithm = loop {
        let kind = u32::from_le_bytes(record[position..position + 4].try_into().unwrap());
        assert_ne!(kind, u32::MAX, "WOF reparse metadata is required");
        let length =
            u32::from_le_bytes(record[position + 4..position + 8].try_into().unwrap()) as usize;
        if kind == 0xc0 {
            let offset =
                u16::from_le_bytes(record[position + 20..position + 22].try_into().unwrap())
                    as usize;
            let data = &record[position + offset..position + length];
            assert_eq!(
                u32::from_le_bytes(data[..4].try_into().unwrap()),
                0x80000017
            );
            assert_eq!(u32::from_le_bytes(data[12..16].try_into().unwrap()), 2);
            break u32::from_le_bytes(data[20..24].try_into().unwrap());
        }
        assert!(length >= 24);
        position += length;
    };
    let data = filesystem
        .read_ads_by_inode(inode, "WofCompressedData")
        .unwrap();
    let logical_size = filesystem.file_size_by_inode(inode).unwrap().unwrap();
    let export = tempfile::Builder::new()
        .prefix("wof-oracle-")
        .tempdir_in(output_root)
        .unwrap()
        .keep();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(export.join("compressed.bin"))
        .unwrap();
    file.write_all(&data).unwrap();
    std::fs::write(export.join("metadata.json"),serde_json::json!({"algorithm":algorithm,"logicalSize":logical_size,"compressedSize":data.len()}).to_string()).unwrap();
    println!(
        "WOF_ORACLE_EXPORT={} algorithm={algorithm} logical_size={logical_size} compressed_size={}",
        export.display(),
        data.len()
    );
}
