use crate::{checksum::adler32, paged_index::PagedIndex, sections, table_validation, E01Reader};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

struct Fixture {
    _directory: tempfile::TempDir,
    path: PathBuf,
    table: u64,
    count: u32,
}

fn descriptor(file: &mut File, kind: &str, next: u64, size: u64) {
    let mut bytes = [0; 76];
    bytes[..kind.len()].copy_from_slice(kind.as_bytes());
    bytes[16..24].copy_from_slice(&next.to_le_bytes());
    bytes[24..32].copy_from_slice(&size.to_le_bytes());
    let checksum = adler32(&bytes[..72]);
    bytes[72..76].copy_from_slice(&checksum.to_le_bytes());
    file.write_all(&bytes).unwrap();
}

fn fixture(count: u32, bad_primary: bool, bad_backup: bool) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("paged.E01");
    let data_start = 13 + 76 + 32 + 76;
    let table = data_start + u64::from(count) * 512;
    let table_size = 76 + 24 + u64::from(count) * 4 + 4;
    let mut file = File::create(&path).unwrap();
    file.write_all(b"EVF\t\r\n\x01\0\0\x01\0\x01\0").unwrap();
    descriptor(&mut file, "volume", 121, 108);
    let mut geometry = [0; 32];
    geometry[8..12].copy_from_slice(&1u32.to_le_bytes());
    geometry[12..16].copy_from_slice(&512u32.to_le_bytes());
    geometry[16..24].copy_from_slice(&u64::from(count).to_le_bytes());
    file.write_all(&geometry).unwrap();
    descriptor(&mut file, "sectors", table, 76 + u64::from(count) * 512);
    for index in 0..count {
        file.write_all(&[marker(index); 512]).unwrap();
    }
    for (kind, corrupted) in [("table", bad_primary), ("table2", bad_backup)] {
        let next = file.stream_position().unwrap() + table_size;
        descriptor(&mut file, kind, next, table_size);
        let mut header = [0; 24];
        header[..4].copy_from_slice(&count.to_le_bytes());
        let checksum = adler32(&header[..20]);
        header[20..24].copy_from_slice(&checksum.to_le_bytes());
        file.write_all(&header).unwrap();
        let offsets: Vec<u8> = (0..count)
            .flat_map(|index| (data_start as u32 + index * 512).to_le_bytes())
            .collect();
        file.write_all(&offsets).unwrap();
        file.write_all(&(adler32(&offsets) ^ u32::from(corrupted)).to_le_bytes())
            .unwrap();
    }
    descriptor(&mut file, "done", 0, 76);
    Fixture {
        _directory: directory,
        path,
        table,
        count,
    }
}

fn marker(index: u32) -> u8 {
    (index % 251) as u8
}

fn paged_reader(fixture: &Fixture) -> std::io::Result<E01Reader> {
    let mut files = vec![File::open(&fixture.path)?];
    let inventory = sections::inventory(&mut files, None)?;
    let tables = table_validation::validate_segment(&mut files[0], &inventory[0], 0, None)?;
    let index = PagedIndex::new(tables, u64::from(fixture.count))?;
    Ok(E01Reader::from_parts(
        evidence_core::ReaderInfo {
            path: fixture.path.clone(),
            size: u64::from(fixture.count) * 512,
            kind: "e01".into(),
        },
        u64::from(fixture.count) * 512,
        1,
        512,
        index,
        files,
    ))
}

#[test]
fn paged_index_reads_page_boundary_and_last_chunk() {
    let fixture = fixture(crate::paged_index::PAGE_ENTRIES as u32 + 2, false, false);
    let mut paged = paged_reader(&fixture).unwrap();
    let mut independent = E01Reader::open(&fixture.path).unwrap();
    for index in [
        0,
        7,
        crate::paged_index::PAGE_ENTRIES - 1,
        u64::from(fixture.count) - 1,
    ] {
        let length = if index + 1 < u64::from(fixture.count) {
            700
        } else {
            512
        };
        let mut actual = vec![0; length];
        let mut expected = vec![0; length];
        paged.read_exact_at(index * 512, &mut actual).unwrap();
        independent
            .read_exact_at(index * 512, &mut expected)
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual[0], marker(index as u32));
    }
    let plan = paged.open_plan().unwrap();
    assert!(plan.metadata_bytes() < 34 * 1024 * 1024);
}

#[test]
fn mirrored_table_excludes_primary_metadata_from_last_chunk() {
    let fixture = fixture(3, true, false);
    let mut reader = paged_reader(&fixture).unwrap();
    let mut last = [0; 512];
    reader.read_exact_at(1024, &mut last).unwrap();
    assert_eq!(last, [marker(2); 512]);
    assert_eq!(reader.open_plan().unwrap().mirrored_table_count(), 1);
}

#[test]
fn invalid_primary_and_mirror_are_rejected() {
    let fixture = fixture(3, true, true);
    let error = paged_reader(&fixture).err().unwrap();
    assert!(error.to_string().contains("offset array checksum mismatch"));
}

#[test]
fn paged_readers_share_plan_and_keep_independent_handles_and_cursors() {
    let fixture = fixture(32, false, false);
    let plan = Arc::new(paged_reader(&fixture).unwrap().open_plan().unwrap());
    let mut workers = Vec::new();
    for lane in 0..4u64 {
        let plan = Arc::clone(&plan);
        workers.push(std::thread::spawn(move || {
            let mut reader = plan.open_reader().unwrap();
            for round in 0..128 {
                let index = (round * 7 + lane) % 32;
                let mut bytes = [0; 512];
                reader.read_exact_at(index * 512, &mut bytes).unwrap();
                assert_eq!(bytes, [marker(index as u32); 512]);
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn changed_offset_page_is_rejected_before_use() {
    let fixture = fixture(3, false, false);
    let mut reader = paged_reader(&fixture).unwrap();
    let mut writer = OpenOptions::new().write(true).open(&fixture.path).unwrap();
    writer
        .seek(SeekFrom::Start(fixture.table + 76 + 24))
        .unwrap();
    writer.write_all(&0u32.to_le_bytes()).unwrap();
    writer.flush().unwrap();
    let error = reader.read(&mut [0; 1]).unwrap_err();
    assert!(error
        .to_string()
        .contains("table page changed since planning"));
}

#[test]
fn cancellable_planning_stops_before_open_and_during_table_walk() {
    let fixture = fixture(3, false, false);
    let cancel = AtomicBool::new(true);
    let error = E01Reader::open_with_segment_limit_cancellable(&fixture.path, 1, Some(&cancel))
        .err()
        .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    let mut file = File::open(&fixture.path).unwrap();
    let inventory = sections::inventory(std::slice::from_mut(&mut file), None).unwrap();
    let error = table_validation::validate_segment(&mut file, &inventory[0], 0, Some(&cancel))
        .err()
        .unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
}

#[test]
fn large_geometry_uses_paged_validation_and_reports_missing_chunks() {
    let fixture = fixture(3, false, false);
    let mut writer = OpenOptions::new().write(true).open(&fixture.path).unwrap();
    writer.seek(SeekFrom::Start(13 + 76 + 16)).unwrap();
    writer.write_all(&8_388_609u64.to_le_bytes()).unwrap();
    drop(writer);
    let error = E01Reader::open(&fixture.path).err().unwrap();
    assert!(error
        .to_string()
        .contains("chunk table count mismatch: expected 8388609, found 3"));
}

#[test]
fn adler_streaming_matches_known_vector() {
    assert_eq!(adler32(b"Wikipedia"), 0x11e60398);
    let mut state = crate::checksum::Adler32::new();
    state.update(b"Wiki");
    state.update(b"pedia");
    assert_eq!(state.finish(), adler32(b"Wikipedia"));
}

#[test]
fn hostile_entry_count_is_rejected_before_offset_allocation() {
    let fixture = fixture(3, false, false);
    let mut writer = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fixture.path)
        .unwrap();
    let table_size = 76 + 24 + u64::from(fixture.count) * 4 + 4;
    for table in [fixture.table, fixture.table + table_size] {
        let mut header = [0; 24];
        writer.seek(SeekFrom::Start(table + 76)).unwrap();
        writer.read_exact(&mut header).unwrap();
        header[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        let checksum = adler32(&header[..20]);
        header[20..24].copy_from_slice(&checksum.to_le_bytes());
        writer.seek(SeekFrom::Start(table + 76)).unwrap();
        writer.write_all(&header).unwrap();
    }
    drop(writer);
    let error = E01Reader::open(&fixture.path).err().unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .contains("entry count exceeds section length"));
}

#[test]
fn page_eviction_keeps_borrowed_bytes_alive_and_reloads_verified_offsets() {
    let fixture = fixture(crate::paged_index::PAGE_ENTRIES as u32 + 2, false, false);
    let mut reader = paged_reader(&fixture).unwrap();
    let index = Arc::get_mut(&mut reader.chunk_table).unwrap();
    index.cache_limit_pages = 1;
    let first = index.page(0, 0, &mut reader.segment_files).unwrap();
    let expected = first[..4].to_vec();
    let second = index.page(0, 1, &mut reader.segment_files).unwrap();
    assert_eq!(index.cache.lock().unwrap().len(), 1);
    assert_eq!(first[..4], expected);
    assert!(!second.is_empty());
    let reloaded = index.page(0, 0, &mut reader.segment_files).unwrap();
    assert_eq!(reloaded.as_ref(), first.as_ref());
    assert_eq!(index.cache.lock().unwrap().len(), 1);
    for offset in [0, crate::paged_index::PAGE_ENTRIES * 512, 512] {
        let mut bytes = [0; 512];
        reader.read_exact_at(offset, &mut bytes).unwrap();
        assert_eq!(bytes, [marker((offset / 512) as u32); 512]);
    }
}
