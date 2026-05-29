//! exFAT filesystem reader.
//!
//! Implements the `FileSystemReader` trait for exFAT formatted volumes.
//! Based on the Microsoft exFAT specification.

pub mod boot;
pub mod dir;
pub mod fat;
pub mod types;

use boot::ExfatBootSector;
use dir::FileEntrySet;
use evidence_core::filesystem::{FileSystemReader, FsNode};
use evidence_core::EvidenceReader;
use fat::FatEntry;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

/// exFAT filesystem reader.
pub struct ExfatReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    boot: ExfatBootSector,
    /// Offset of the exFAT volume within the evidence (e.g., partition offset).
    volume_offset: u64,
}

impl ExfatReader {
    /// Open an exFAT volume at the given offset.
    ///
    /// Reads and validates the boot sector, then prepares for filesystem operations.
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut boot_buf = [0u8; 512];
        reader.read_exact(&mut boot_buf)?;

        let boot = ExfatBootSector::parse(&boot_buf)?;

        // Validate revision (must be 1.xx)
        if boot.revision_major() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "unsupported exFAT revision {}.{}",
                    boot.revision_major(),
                    boot.revision_minor()
                ),
            ));
        }

        Ok(Self {
            reader: RefCell::new(reader),
            boot,
            volume_offset: offset,
        })
    }

    /// Read a FAT entry for a given cluster.
    fn read_fat_entry(&self, cluster: u32) -> io::Result<FatEntry> {
        let fat_reader = fat::FatReader::new(
            self.volume_offset + self.boot.fat_byte_offset(),
            self.boot.bytes_per_sector(),
        );

        let offset = fat_reader.entry_offset(cluster);
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;

        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;

        Ok(FatReader::parse_entry(&buf))
    }

    /// Walk a cluster chain and return all cluster indices.
    fn walk_cluster_chain(&self, start_cluster: u32) -> io::Result<Vec<u32>> {
        fat::walk_cluster_chain(start_cluster, |cluster| self.read_fat_entry(cluster))
    }

    /// Convert a cluster index to an absolute byte offset in the evidence.
    fn cluster_to_abs_offset(&self, cluster: u32) -> u64 {
        self.volume_offset + self.boot.cluster_to_offset(cluster)
    }

    /// Read data from a cluster chain.
    fn read_cluster_chain_data(&self, start_cluster: u32) -> io::Result<Vec<u8>> {
        let clusters = self.walk_cluster_chain(start_cluster)?;
        let cluster_size = self.boot.cluster_size() as usize;
        let mut data = Vec::with_capacity(clusters.len() * cluster_size);

        for &cluster in &clusters {
            let offset = self.cluster_to_abs_offset(cluster);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(offset))?;

            let mut buf = vec![0u8; cluster_size];
            reader.read_exact(&mut buf)?;
            data.extend_from_slice(&buf);
        }

        Ok(data)
    }

    /// Read directory entries from a directory's cluster chain.
    fn read_directory_entries(&self, cluster: u32) -> io::Result<Vec<FileEntrySet>> {
        let data = self.read_cluster_chain_data(cluster)?;
        dir::parse_directory_entries(&data)
    }

    /// Resolve a path to a (cluster, is_dir, size) tuple.
    ///
    /// Returns None if the path doesn't exist.
    fn resolve_path(&self, path: &str) -> io::Result<Option<(u32, bool, u64)>> {
        let components: Vec<&str> = path
            .trim_matches(|c| c == '\\' || c == '/')
            .split(['\\', '/'])
            .filter(|c| !c.is_empty())
            .collect();

        if components.is_empty() {
            // Root directory
            return Ok(Some((self.boot.first_cluster_of_root, true, 0)));
        }

        let mut current_cluster = self.boot.first_cluster_of_root;
        let mut is_dir = true;
        let mut size = 0u64;

        for (i, component) in components.iter().enumerate() {
            if !is_dir {
                return Ok(None); // Can't traverse into a file
            }

            let entries = self.read_directory_entries(current_cluster)?;
            let lower_component = component.to_lowercase();

            let found = entries.iter().find(|e| {
                e.name.to_lowercase() == lower_component
            });

            match found {
                Some(entry) => {
                    let is_last = i == components.len() - 1;
                    current_cluster = entry.first_cluster;
                    is_dir = entry.is_directory();
                    size = if is_dir { 0 } else { entry.valid_data_length };

                    if is_last {
                        return Ok(Some((current_cluster, is_dir, size)));
                    }
                }
                None => return Ok(None),
            }
        }

        Ok(Some((current_cluster, is_dir, size)))
    }
}

impl FileSystemReader for ExfatReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(FsNode {
            name: "\\".into(),
            path: String::new(),
            is_dir: true,
            size: 0,
            created_at: None,
            modified_at: None,
            accessed_at: None,
        })
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let (cluster, is_dir, _) = self.resolve_path(path)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("path not found: {}", path))
        })?;

        if !is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a directory", path),
            ));
        }

        let entries = self.read_directory_entries(cluster)?;
        let mut nodes = Vec::new();

        for entry in entries {
            // Skip special entries
            if entry.name == "." || entry.name == ".." {
                continue;
            }

            let child_path = if path.is_empty() {
                entry.name.clone()
            } else {
                format!("{}\\{}", path.trim_end_matches('\\'), entry.name)
            };
            let is_dir = entry.is_directory();

            nodes.push(FsNode {
                name: entry.name,
                path: child_path,
                is_dir,
                size: entry.valid_data_length,
                created_at: entry.created_at,
                modified_at: entry.modified_at,
                accessed_at: entry.accessed_at,
            });
        }

        Ok(nodes)
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let (cluster, is_dir, size) = self.resolve_path(path)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("file not found: {}", path))
        })?;

        if is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is a directory", path),
            ));
        }

        let data = self.read_cluster_chain_data(cluster)?;

        // Truncate to valid data length
        let truncated = if (size as usize) < data.len() {
            data[..size as usize].to_vec()
        } else {
            data
        };

        Ok(Box::new(io::Cursor::new(truncated)))
    }

    fn data_source_name(&self) -> &str {
        "exFAT"
    }
}

// Re-export FatReader for use in tests
use fat::FatReader;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::io::{Read, Seek};

    /// A fake reader that wraps a byte vector for testing.
    struct FakeReader {
        data: Vec<u8>,
        pos: u64,
    }

    impl FakeReader {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }
    }

    impl Read for FakeReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let start = self.pos.min(self.data.len() as u64) as usize;
            let end = (start + buf.len()).min(self.data.len());
            let n = end - start;
            buf[..n].copy_from_slice(&self.data[start..end]);
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for FakeReader {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.pos = match pos {
                SeekFrom::Start(p) => p,
                SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
                SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
            };
            Ok(self.pos)
        }
    }

    impl EvidenceReader for FakeReader {
        fn info(&self) -> &evidence_core::ReaderInfo {
            unimplemented!()
        }
    }

    /// Build a minimal exFAT fixture with:
    /// - Boot sector at offset 0
    /// - FAT at sector 24
    /// - Cluster heap at sector 32
    /// - Root directory at cluster 2
    /// - A file "TEST.TXT" at cluster 3
    fn build_exfat_fixture() -> Vec<u8> {
        let sector_size = 512;
        let sectors_per_cluster = 1;
        let total_sectors = 1024u64; // 512KB

        let mut data = vec![0u8; (total_sectors * sector_size as u64) as usize];

        // === Boot Sector (sector 0) ===
        let boot = &mut data[0..512];
        boot[0..3].copy_from_slice(&JUMP_BOOT);
        boot[3..11].copy_from_slice(EXFAT_MAGIC);
        // PartitionOffset = 0
        boot[72..80].copy_from_slice(&total_sectors.to_le_bytes()); // VolumeLength
        boot[80..84].copy_from_slice(&24u32.to_le_bytes()); // FatOffset
        boot[84..88].copy_from_slice(&1u32.to_le_bytes()); // FatLength
        boot[88..92].copy_from_slice(&32u32.to_le_bytes()); // ClusterHeapOffset
        boot[92..96].copy_from_slice(&100u32.to_le_bytes()); // ClusterCount
        boot[96..100].copy_from_slice(&2u32.to_le_bytes()); // FirstClusterOfRootDirectory
        boot[100..104].copy_from_slice(&0x12345678u32.to_le_bytes()); // VolumeSerialNumber
        boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes()); // FileSystemRevision (1.00)
        boot[106..108].copy_from_slice(&0u16.to_le_bytes()); // VolumeFlags
        boot[108] = 9; // BytesPerSectorShift (512 = 2^9)
        boot[109] = 0; // SectorsPerClusterShift (1 = 2^0)
        boot[110] = 1; // NumberOfFats
        boot[111] = 0x80; // DriveSelect
        boot[112] = 0xFF; // PercentInUse (unknown)
        boot[510..512].copy_from_slice(&BOOT_SIGNATURE.to_le_bytes());

        // === FAT (sector 24, offset 12288) ===
        let fat_offset = 24 * sector_size;
        let fat = &mut data[fat_offset..fat_offset + sector_size];
        // FatEntry[0]: Media type
        fat[0..4].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF]);
        // FatEntry[1]: Reserved
        fat[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        // FatEntry[2]: Root directory (EOC)
        fat[8..12].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        // FatEntry[3]: TEST.TXT file data (EOC)
        fat[12..16].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);

        // === Cluster Heap (sector 32, offset 16384) ===
        let cluster_heap_offset = 32 * sector_size;
        let cluster_size = sector_size * sectors_per_cluster;

        // Cluster 2: Root directory
        let root_offset = cluster_heap_offset;
        let root = &mut data[root_offset..root_offset + cluster_size];

        // File Directory Entry for TEST.TXT
        let mut pos = 0;

        // File entry
        root[pos] = 0x85; // In-use, type 5 (File)
        root[pos + 1] = 0x02; // SecondaryCount = 2
        root[pos + 4] = 0x20; // FileAttributes = Archive
        root[pos + 5] = 0x00;
        pos += 32;

        // Stream extension
        root[pos] = 0xC0; // In-use, type 0 (Stream)
        root[pos + 3] = 8; // NameLength = 8 ("TEST.TXT")
        root[pos + 8] = 11; // ValidDataLength = 11
        root[pos + 9] = 0;
        root[pos + 10] = 0;
        root[pos + 11] = 0;
        root[pos + 12] = 0;
        root[pos + 13] = 0;
        root[pos + 14] = 0;
        root[pos + 15] = 0;
        root[pos + 20] = 3; // FirstCluster = 3
        root[pos + 21] = 0;
        root[pos + 22] = 0;
        root[pos + 23] = 0;
        root[pos + 24] = 11; // DataLength = 11
        root[pos + 25] = 0;
        root[pos + 26] = 0;
        root[pos + 27] = 0;
        pos += 32;

        // File Name entry
        root[pos] = 0xC1; // In-use, type 1 (FileName)
        // "TEST.TXT" in UTF-16LE
        let name = "TEST.TXT";
        for (i, c) in name.encode_utf16().enumerate() {
            let offset = pos + 2 + i * 2;
            root[offset] = (c & 0xFF) as u8;
            root[offset + 1] = ((c >> 8) & 0xFF) as u8;
        }

        // Cluster 3: TEST.TXT content
        let file_offset = cluster_heap_offset + cluster_size; // Second cluster
        data[file_offset..file_offset + 11].copy_from_slice(b"Hello World");

        data
    }

    #[test]
    fn exfat_open_valid() {
        let img = build_exfat_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let fat = ExfatReader::open(reader, 0).unwrap();

        assert_eq!(fat.boot.bytes_per_sector(), 512);
        assert_eq!(fat.boot.cluster_size(), 512);
        assert_eq!(fat.boot.first_cluster_of_root, 2);
    }

    #[test]
    fn exfat_list_root() {
        let img = build_exfat_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let fat = ExfatReader::open(reader, 0).unwrap();

        // Debug: read the root directory data directly
        let root_cluster = fat.boot.first_cluster_of_root;
        let root_data = fat.read_cluster_chain_data(root_cluster).unwrap();
        println!("Root cluster: {}, data len: {}", root_cluster, root_data.len());
        println!("First 96 bytes: {:?}", &root_data[..96.min(root_data.len())]);

        // Debug: parse directory entries
        let entries = dir::parse_directory_entries(&root_data).unwrap();
        println!("Parsed {} entries", entries.len());
        for e in &entries {
            println!("  Entry: name='{}', first_cluster={}, is_dir={}", e.name, e.first_cluster, e.is_directory());
        }

        let children = fat.list_children("").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "TEST.TXT");
        assert!(!children[0].is_dir);
        assert_eq!(children[0].size, 11);
    }

    #[test]
    fn exfat_open_file() {
        let img = build_exfat_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let fat = ExfatReader::open(reader, 0).unwrap();

        let mut file = fat.open_file("TEST.TXT").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "Hello World");
    }

    #[test]
    fn exfat_open_nonexistent() {
        let img = build_exfat_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let fat = ExfatReader::open(reader, 0).unwrap();

        assert!(fat.open_file("NOFILE.TXT").is_err());
    }

    #[test]
    fn exfat_root_properties() {
        let img = build_exfat_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let fat = ExfatReader::open(reader, 0).unwrap();

        let root = fat.root().unwrap();
        assert_eq!(root.name, "\\");
        assert!(root.is_dir);
        assert_eq!(root.size, 0);
    }

    #[test]
    fn exfat_data_source_name() {
        let img = build_exfat_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let fat = ExfatReader::open(reader, 0).unwrap();

        assert_eq!(fat.data_source_name(), "exFAT");
    }
}
