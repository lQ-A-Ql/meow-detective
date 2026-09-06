use crate::boot::{verify_boot_region, ExfatBootSector};
use crate::dir::{self, FileEntrySet};
use crate::fat::{FatEntry, FatReader};
use crate::types::MIN_CLUSTER;
use crate::upcase::{self, UpcaseTable};
use evidence_core::filesystem::{invalid_fs_data, unsupported_fs};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

/// exFAT filesystem reader.
pub struct ExfatReader {
    pub(crate) reader: RefCell<Box<dyn EvidenceReader>>,
    pub(crate) boot: ExfatBootSector,
    pub(crate) volume_offset: u64,
    pub(crate) upcase: UpcaseTable,
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
        if boot.revision_major() != 1 {
            return Err(unsupported_fs(format!(
                "unsupported exFAT revision {}.{}",
                boot.revision_major(),
                boot.revision_minor()
            )));
        }
        if reader.info().size != 0 {
            let volume_bytes = boot
                .volume_length
                .checked_mul(boot.bytes_per_sector() as u64)
                .ok_or_else(|| invalid_fs_data("exFAT volume length overflows"))?;
            let available = reader
                .info()
                .size
                .checked_sub(offset)
                .ok_or_else(|| invalid_fs_data("exFAT offset exceeds reader length"))?;
            if volume_bytes > available {
                return Err(invalid_fs_data(format!(
                    "exFAT volume exceeds reader bounds: {} bytes declared, {} available",
                    volume_bytes, available
                )));
            }

            let boot_region_bytes = u64::from(boot.bytes_per_sector())
                .checked_mul(24)
                .ok_or_else(|| invalid_fs_data("exFAT boot region size overflows"))?;
            if boot_region_bytes > available {
                return Err(invalid_fs_data(
                    "exFAT volume is too small to contain main and backup boot regions",
                ));
            }
            verify_boot_region(&mut reader, offset, boot.bytes_per_sector())?;
        }

        let filesystem = Self {
            reader: RefCell::new(reader),
            boot,
            volume_offset: offset,
            upcase: UpcaseTable::fallback(),
        };
        let upcase = upcase::load(&filesystem)?;
        Ok(Self {
            upcase,
            ..filesystem
        })
    }

    pub(crate) fn validate_cluster(&self, cluster: u32) -> io::Result<()> {
        let max_cluster = self.boot.cluster_count.saturating_add(1);
        if cluster < MIN_CLUSTER || cluster > max_cluster {
            return Err(invalid_fs_data(format!(
                "cluster {} out of range 2..={}",
                cluster, max_cluster
            )));
        }
        Ok(())
    }

    pub(crate) fn read_fat_entry(&self, cluster: u32) -> io::Result<FatEntry> {
        self.validate_cluster(cluster)?;

        let fat_reader = FatReader::new(
            self.volume_offset + self.boot.active_fat_byte_offset(),
            self.boot.bytes_per_sector(),
        );
        let offset = fat_reader.entry_offset(cluster);
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;

        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(FatReader::parse_entry(&buf))
    }

    pub(crate) fn next_cluster_in_chain(
        &self,
        current: u32,
        start_cluster: u32,
        visited: &std::collections::HashSet<u32>,
    ) -> io::Result<Option<u32>> {
        match self.read_fat_entry(current)? {
            FatEntry::EndOfChain => Ok(None),
            FatEntry::BadCluster => Err(invalid_fs_data(format!(
                "bad cluster marker in chain starting at {} after cluster {}",
                start_cluster, current
            ))),
            FatEntry::Free => Err(invalid_fs_data(format!(
                "unexpected free cluster {} in chain starting at {}",
                current, start_cluster
            ))),
            FatEntry::Cluster(next) => {
                self.validate_cluster(next)?;
                if visited.contains(&next) {
                    return Err(invalid_fs_data(format!(
                        "cycle detected in cluster chain: cluster {} points to already-visited cluster {}",
                        current, next
                    )));
                }
                Ok(Some(next))
            }
            FatEntry::Reserved(value) => Err(invalid_fs_data(format!(
                "reserved FAT marker {value:#010x} in chain starting at {} after cluster {}",
                start_cluster, current
            ))),
        }
    }

    pub(crate) fn cluster_to_abs_offset(&self, cluster: u32) -> u64 {
        self.volume_offset + self.boot.cluster_to_offset(cluster)
    }

    pub(crate) fn names_equal(&self, left: &str, right: &str) -> bool {
        self.upcase.fold(left) == self.upcase.fold(right)
    }

    pub(crate) fn read_directory_entries(
        &self,
        cluster: u32,
        no_fat_chain: bool,
        data_length: u64,
    ) -> io::Result<Vec<FileEntrySet>> {
        let data = if no_fat_chain {
            self.read_no_fat_chain_data(cluster, data_length)?
        } else {
            self.read_cluster_chain_data(cluster)?
        };
        dir::parse_directory_entries(&data)
    }
}
