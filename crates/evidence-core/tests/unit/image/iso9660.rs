use super::*;

fn put_both_endian_u32(target: &mut [u8], value: u32) {
    target[..4].copy_from_slice(&value.to_le_bytes());
    target[4..8].copy_from_slice(&value.to_be_bytes());
}

fn directory_record(extent: u32, size: u32, flags: u8, name: &[u8]) -> Vec<u8> {
    let padding = usize::from(name.len().is_multiple_of(2));
    let length = 33 + name.len() + padding;
    let mut record = vec![0u8; length];
    record[0] = length as u8;
    put_both_endian_u32(&mut record[2..10], extent);
    put_both_endian_u32(&mut record[10..18], size);
    record[25] = flags;
    record[28..30].copy_from_slice(&1u16.to_le_bytes());
    record[30..32].copy_from_slice(&1u16.to_be_bytes());
    record[32] = name.len() as u8;
    record[33..33 + name.len()].copy_from_slice(name);
    record
}

fn write_tiny_iso(path: &Path) {
    const BLOCKS: usize = 24;
    let mut image = vec![0u8; BLOCKS * BLOCK_SIZE as usize];
    let pvd = &mut image[16 * BLOCK_SIZE as usize..17 * BLOCK_SIZE as usize];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    put_both_endian_u32(&mut pvd[80..88], BLOCKS as u32);
    pvd[128..130].copy_from_slice(&(BLOCK_SIZE as u16).to_le_bytes());
    pvd[130..132].copy_from_slice(&(BLOCK_SIZE as u16).to_be_bytes());
    let root = directory_record(20, BLOCK_SIZE as u32, 0x02, &[0]);
    pvd[156..156 + root.len()].copy_from_slice(&root);

    let terminator = &mut image[17 * BLOCK_SIZE as usize..18 * BLOCK_SIZE as usize];
    terminator[0] = 255;
    terminator[1..6].copy_from_slice(b"CD001");
    terminator[6] = 1;

    let mut records = Vec::new();
    records.extend(directory_record(20, BLOCK_SIZE as u32, 0x02, &[0]));
    records.extend(directory_record(20, BLOCK_SIZE as u32, 0x02, &[1]));
    records.extend(directory_record(21, 5, 0, b"HELLO.TXT;1"));
    let directory = &mut image[20 * BLOCK_SIZE as usize..21 * BLOCK_SIZE as usize];
    directory[..records.len()].copy_from_slice(&records);
    image[21 * BLOCK_SIZE as usize..21 * BLOCK_SIZE as usize + 5].copy_from_slice(b"hello");
    std::fs::write(path, image).expect("write ISO fixture");
}

fn write_joliet_iso(path: &Path) {
    const BLOCKS: usize = 25;
    let mut image = vec![0u8; BLOCKS * BLOCK_SIZE as usize];
    for (block, descriptor_type, root_extent) in [(16, 1, 20), (17, 2, 22)] {
        let descriptor = &mut image[block * BLOCK_SIZE as usize..(block + 1) * BLOCK_SIZE as usize];
        descriptor[0] = descriptor_type;
        descriptor[1..6].copy_from_slice(b"CD001");
        descriptor[6] = 1;
        put_both_endian_u32(&mut descriptor[80..88], BLOCKS as u32);
        descriptor[128..130].copy_from_slice(&(BLOCK_SIZE as u16).to_le_bytes());
        descriptor[130..132].copy_from_slice(&(BLOCK_SIZE as u16).to_be_bytes());
        let root = directory_record(root_extent, BLOCK_SIZE as u32, 0x02, &[0]);
        descriptor[156..156 + root.len()].copy_from_slice(&root);
    }
    image[17 * BLOCK_SIZE as usize + 88..17 * BLOCK_SIZE as usize + 91].copy_from_slice(b"%/E");
    let terminator = &mut image[18 * BLOCK_SIZE as usize..19 * BLOCK_SIZE as usize];
    terminator[0] = 255;
    terminator[1..6].copy_from_slice(b"CD001");
    terminator[6] = 1;

    let joliet_name = "测试.TXT;1"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect::<Vec<_>>();
    let mut records = Vec::new();
    records.extend(directory_record(22, BLOCK_SIZE as u32, 0x02, &[0]));
    records.extend(directory_record(22, BLOCK_SIZE as u32, 0x02, &[1]));
    records.extend(directory_record(23, 6, 0, &joliet_name));
    let directory = &mut image[22 * BLOCK_SIZE as usize..23 * BLOCK_SIZE as usize];
    directory[..records.len()].copy_from_slice(&records);
    image[23 * BLOCK_SIZE as usize..23 * BLOCK_SIZE as usize + 6]
        .copy_from_slice("内容".as_bytes());
    std::fs::write(path, image).expect("write Joliet fixture");
}

#[test]
fn enumerates_and_reads_iso9660_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("tiny.iso");
    write_tiny_iso(&path);

    let fs = Iso9660Reader::open(&path).expect("open ISO9660");
    let children = fs.list_children("").expect("list root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "HELLO.TXT");
    assert!(children[0].read_only);

    let mut file = fs.open_file("/HELLO.TXT").expect("open file");
    let mut content = String::new();
    file.read_to_string(&mut content).expect("read file");
    assert_eq!(content, "hello");
}

#[test]
fn prefers_joliet_names_and_supports_seekable_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("joliet.iso");
    write_joliet_iso(&path);

    let fs = Iso9660Reader::open(&path).expect("open Joliet ISO");
    let children = fs.list_children("").expect("list root");
    assert_eq!(children[0].name, "测试.TXT");
    let mut file = fs.open_file_seekable("测试.TXT").expect("seekable file");
    file.seek(SeekFrom::Start(3)).expect("seek UTF-8 payload");
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).expect("read tail");
    assert_eq!(tail, "容".as_bytes());
}

#[test]
fn reads_iso_from_a_nonzero_partition_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    let iso_path = dir.path().join("tiny.iso");
    write_tiny_iso(&iso_path);
    let iso = std::fs::read(&iso_path).expect("read ISO fixture");
    let disk_path = dir.path().join("disk.raw");
    let mut disk = vec![0x5a; 4096];
    disk.extend_from_slice(&iso);
    std::fs::write(&disk_path, disk).expect("write partitioned image");

    let reader = RawImageReader::open(&disk_path).expect("open raw image");
    let window = crate::PartitionWindowReader::new(Box::new(reader), 4096, Some(iso.len() as u64))
        .expect("open partition window");
    let filesystem =
        Iso9660Reader::from_reader(Box::new(window), Some("partition.iso")).expect("open ISO");

    let children = filesystem.list_children("").expect("list root");
    assert_eq!(children[0].name, "HELLO.TXT");
}

#[test]
fn rejects_non_iso_and_truncated_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let invalid = dir.path().join("not.iso");
    std::fs::write(&invalid, vec![0u8; 18 * BLOCK_SIZE as usize]).expect("write invalid");
    assert_eq!(
        Iso9660Reader::open(&invalid)
            .expect_err("missing PVD")
            .kind(),
        io::ErrorKind::InvalidData
    );

    let truncated = dir.path().join("truncated.iso");
    write_tiny_iso(&truncated);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&truncated)
        .expect("open fixture");
    file.set_len(20 * BLOCK_SIZE).expect("truncate directory");
    assert_eq!(
        Iso9660Reader::open(&truncated)
            .expect_err("truncated root directory")
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn rejects_file_extent_outside_declared_volume() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("outside.iso");
    write_tiny_iso(&path);
    let mut image = std::fs::read(&path).expect("read fixture");
    let directory_offset = 20 * BLOCK_SIZE as usize;
    let first = image[directory_offset] as usize;
    let second = image[directory_offset + first] as usize;
    let file_record = directory_offset + first + second;
    put_both_endian_u32(&mut image[file_record + 2..file_record + 10], 24);
    std::fs::write(&path, image).expect("rewrite fixture");

    assert_eq!(
        Iso9660Reader::open(&path)
            .expect_err("extent outside declared volume")
            .kind(),
        io::ErrorKind::InvalidData
    );
}
