use app_services::datasource_service::{detect_image_filesystem, ImageFilesystemKind};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::{Read, Seek, SeekFrom};

fn sample_path() -> std::path::PathBuf {
    "E:/pangushi/刘洋/liuyang_pc.E01".into()
}

fn skip() -> bool {
    if !sample_path().exists() {
        eprintln!("SKIP");
        true
    } else {
        false
    }
}

#[test]
fn detects_supported_filesystem_in_real_e01() {
    if skip() {
        return;
    }

    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    eprintln!("probe_warnings={:?}", probe.warnings);
    for partition in &probe.partitions {
        eprintln!(
            "partition idx={} name='{}' kind_label={} status={:?} offset={} length={} guid={:?}",
            partition.index,
            partition.name,
            partition.kind_label,
            partition.status,
            partition.offset,
            partition.length,
            partition.type_guid
        );
    }
    for candidate in &probe.candidates {
        eprintln!(
            "candidate idx={:?} name={:?} kind={:?} offset={} source={:?}",
            candidate.partition_index,
            candidate.partition_name,
            candidate.kind,
            candidate.offset,
            candidate.source
        );
    }

    let candidate = probe
        .candidates
        .first()
        .expect("expected supported filesystem");
    assert!(matches!(
        candidate.kind,
        ImageFilesystemKind::Ntfs | ImageFilesystemKind::Fat
    ));
}

#[test]
fn opens_detected_filesystem_from_real_e01() {
    if skip() {
        return;
    }

    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .first()
        .expect("expected supported filesystem");

    let boxed_reader: Box<dyn EvidenceReader> = Box::new(reader);
    match candidate.kind {
        ImageFilesystemKind::Ntfs => {
            let fs = fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset).unwrap();
            let root = fs.root().unwrap();
            assert!(root.is_dir);
        }
        ImageFilesystemKind::Fat => {
            let fs = fs_fat::FatReader::open(boxed_reader, candidate.offset).unwrap();
            let root = fs.root().unwrap();
            assert!(root.is_dir);
        }
        ImageFilesystemKind::BitLocker => {
            panic!("expected first real sample candidate to be directly readable");
        }
    }
}

#[test]
fn dumps_real_e01_partition_accessibility() {
    if skip() {
        return;
    }

    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();

    for candidate in probe.candidates {
        let boxed_reader: Box<dyn EvidenceReader> =
            Box::new(E01Reader::open(&sample_path()).unwrap());
        match candidate.kind {
            ImageFilesystemKind::Ntfs => {
                let fs = fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset).unwrap();
                let root = fs.root().unwrap();
                let children = match fs.list_children("") {
                    Ok(children) => children,
                    Err(error) => {
                        eprintln!(
                            "accessible idx={:?} name={:?} kind=NTFS root={} error={}",
                            candidate.partition_index, candidate.partition_name, root.name, error
                        );
                        continue;
                    }
                };
                eprintln!(
                    "accessible idx={:?} name={:?} kind=NTFS root={} children={}",
                    candidate.partition_index,
                    candidate.partition_name,
                    root.name,
                    children.len()
                );
            }
            ImageFilesystemKind::Fat => {
                let fs = fs_fat::FatReader::open(boxed_reader, candidate.offset).unwrap();
                let root = fs.root().unwrap();
                let children = fs.list_children("").unwrap_or_default();
                eprintln!(
                    "accessible idx={:?} name={:?} kind=FAT root={} children={}",
                    candidate.partition_index,
                    candidate.partition_name,
                    root.name,
                    children.len()
                );
            }
            ImageFilesystemKind::BitLocker => {}
        }
    }
}

#[test]
fn dump_probe_context_for_real_e01() {
    if skip() {
        return;
    }

    let mut reader = E01Reader::open(&sample_path()).unwrap();
    eprintln!("reader_size={}", reader.info().size);

    let mut sector0 = [0u8; 512];
    reader.seek(SeekFrom::Start(0)).unwrap();
    reader.read_exact(&mut sector0).unwrap();
    eprintln!("sector0[0..16]={:02X?}", &sector0[0..16]);
    eprintln!("sector0[3..11]={:02X?}", &sector0[3..11]);
    eprintln!("mbr_sig={:02X}{:02X}", sector0[510], sector0[511]);

    let entries = evidence_core::volume::mbr::parse_partition_table(&sector0);
    for (idx, entry) in entries.iter().enumerate() {
        eprintln!(
            "mbr[{}]: type={:02X} lba_start={} sectors={}",
            idx, entry.partition_type, entry.lba_start, entry.sector_count
        );
    }

    let mut sector1 = [0u8; 512];
    reader.seek(SeekFrom::Start(512)).unwrap();
    match reader.read_exact(&mut sector1) {
        Ok(()) => {
            eprintln!("sector1[0..8]={:02X?}", &sector1[0..8]);
            eprintln!(
                "gpt_header={}",
                evidence_core::volume::gpt::parse_gpt_header(&sector1).is_some()
            );
        }
        Err(error) => {
            eprintln!("sector1_read_error={error}");
        }
    }
}

#[test]
fn dump_real_e01_ntfs_root_record_details() {
    if skip() {
        return;
    }

    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .iter()
        .find(|candidate| matches!(candidate.kind, ImageFilesystemKind::Ntfs))
        .expect("expected NTFS candidate");

    let mut boot = [0u8; 512];
    reader.seek(SeekFrom::Start(candidate.offset)).unwrap();
    reader.read_exact(&mut boot).unwrap();

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());
    let mft_record_size = {
        let raw = i32::from_le_bytes(boot[0x40..0x44].try_into().unwrap());
        if raw > 0 {
            1024u32
        } else if raw < 0 && (-raw) < 32 {
            (1u32 << (-raw as u32)).max(512)
        } else {
            1024u32
        }
    };
    let index_record_size = {
        let raw = boot[0x44] as i8;
        if raw > 0 {
            (cluster_size as u32).saturating_mul(raw as u32)
        } else if raw < 0 && (-raw as u32) < 32 {
            (1u32 << (-raw as u32)).max(512)
        } else {
            mft_record_size
        }
    };
    let rec5_offset = candidate.offset + mft_cluster * cluster_size + 5 * mft_record_size as u64;
    eprintln!(
        "ntfs_boot idx={:?} offset={} bps={} spc={} cluster_size={} mft_cluster={} mft_record_size={} index_record_size={}",
        candidate.partition_index,
        candidate.offset,
        bytes_per_sector,
        sectors_per_cluster,
        cluster_size,
        mft_cluster,
        mft_record_size,
        index_record_size
    );

    let mut rec = vec![0u8; mft_record_size as usize];
    reader.seek(SeekFrom::Start(rec5_offset)).unwrap();
    reader.read_exact(&mut rec).unwrap();
    eprintln!(
        "record5_raw_magic={:02X?} first64={:02X?}",
        &rec[0..4],
        &rec[0..64]
    );

    apply_record_fixup_for_test(&mut rec, bytes_per_sector as usize).unwrap();
    eprintln!(
        "record5_fixed magic={:?} attr_off={} flags={:02X?}",
        std::str::from_utf8(&rec[0..4]).unwrap_or("????"),
        u16::from_le_bytes([rec[0x14], rec[0x15]]),
        &rec[0x16..0x18]
    );

    let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < rec.len() {
        let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap());
        if typ == 0xFFFF_FFFF {
            eprintln!("attr_end pos=0x{pos:X}");
            break;
        }
        let len = u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let nonresident = rec[pos + 8];
        eprintln!(
            "attr pos=0x{pos:X} type=0x{typ:02X} len=0x{len:X} nonresident={} head={:02X?}",
            nonresident,
            &rec[pos..(pos + len.min(64)).min(rec.len())]
        );
        if typ == 0x90 && pos + 0x20 <= rec.len() {
            let content_size =
                u32::from_le_bytes(rec[pos + 0x10..pos + 0x14].try_into().unwrap()) as usize;
            let content_off =
                u16::from_le_bytes(rec[pos + 0x14..pos + 0x16].try_into().unwrap()) as usize;
            let content_start = pos + content_off;
            let content_end = (content_start + content_size).min(rec.len());
            let content = &rec[content_start..content_end];
            eprintln!(
                "index_root content_size={} content_off=0x{:X} first96={:02X?}",
                content_size,
                content_off,
                &content[..content.len().min(96)]
            );
            if content.len() >= 0x20 {
                let entries_off =
                    u32::from_le_bytes(content[0x10..0x14].try_into().unwrap()) as usize;
                let seq_end = u32::from_le_bytes(content[0x14..0x18].try_into().unwrap()) as usize;
                let buf_end = u32::from_le_bytes(content[0x18..0x1C].try_into().unwrap()) as usize;
                eprintln!(
                    "index_root list entries_off=0x{:X} seq_end=0x{:X} buf_end=0x{:X} flags={:02X?}",
                    entries_off,
                    seq_end,
                    buf_end,
                    &content[0x1C..0x20]
                );
                let start = 0x10 + entries_off;
                let end = (0x10 + seq_end).min(content.len());
                if start < end {
                    eprintln!(
                        "index_root entries={:02X?}",
                        &content[start..end.min(start + 128)]
                    );
                }
            }
        }
        if typ == 0xA0 && pos + 0x40 <= rec.len() {
            let run_off =
                u16::from_le_bytes(rec[pos + 0x20..pos + 0x22].try_into().unwrap()) as usize;
            let alloc_size = u64::from_le_bytes(rec[pos + 0x28..pos + 0x30].try_into().unwrap());
            let real_size = u64::from_le_bytes(rec[pos + 0x30..pos + 0x38].try_into().unwrap());
            eprintln!(
                "index_alloc run_off=0x{:X} alloc_size={} real_size={} runs={:02X?}",
                run_off,
                alloc_size,
                real_size,
                &rec[(pos + run_off)..(pos + len).min(pos + run_off + 64)]
            );
        }

        if len == 0 {
            break;
        }
        pos += len;
    }
}

fn apply_record_fixup_for_test(record: &mut [u8], sector_size: usize) -> std::io::Result<()> {
    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }

    let expected = [record[usa_offset], record[usa_offset + 1]];
    for i in 1..usa_count {
        let fixup_pos = i * sector_size - 2;
        if record[fixup_pos..fixup_pos + 2] != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "update sequence signature mismatch",
            ));
        }
        let replacement = usa_offset + i * 2;
        record[fixup_pos] = record[replacement];
        record[fixup_pos + 1] = record[replacement + 1];
    }
    Ok(())
}
