use crate::types::{FatReader, FatType};
use evidence_core::filesystem::invalid_fs_data;
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

impl FatReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut boot = [0u8; 512];
        reader.read_exact(&mut boot)?;

        let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
        let sectors_per_cluster = boot[13];
        let reserved_sectors = u16::from_le_bytes([boot[14], boot[15]]);
        let fat_count = boot[16];
        let root_entries = u16::from_le_bytes([boot[17], boot[18]]);

        if bytes_per_sector == 0 || sectors_per_cluster == 0 {
            return Err(invalid_fs_data("invalid BPB"));
        }

        let fat16_sectors = u16::from_le_bytes([boot[22], boot[23]]) as u32;
        let sectors_per_fat = if fat16_sectors > 0 {
            fat16_sectors
        } else {
            u32::from_le_bytes(boot[36..40].try_into().unwrap_or([0; 4]))
        };
        let total16 = u16::from_le_bytes([boot[19], boot[20]]) as u32;
        let total_sectors = if total16 > 0 {
            total16
        } else {
            u32::from_le_bytes(boot[32..36].try_into().unwrap_or([0; 4]))
        };
        let root_dir_sectors = (root_entries as u32 * 32).div_ceil(bytes_per_sector as u32);
        let first_data_sector =
            reserved_sectors as u32 + fat_count as u32 * sectors_per_fat + root_dir_sectors;
        let data_sectors = total_sectors.saturating_sub(first_data_sector);
        let cluster_count = data_sectors / sectors_per_cluster as u32;
        let fat_type = detect_fat_type(&boot, cluster_count);
        let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
        let root_cluster = if fat_type == FatType::Fat32 {
            u32::from_le_bytes(boot[44..48].try_into().unwrap_or([2, 0, 0, 0])).max(2)
        } else {
            0
        };

        Ok(Self {
            reader: RefCell::new(reader),
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            fat_count,
            root_entries,
            sectors_per_fat,
            first_data_sector,
            cluster_size,
            fat_type,
            cluster_count,
            root_cluster,
            volume_offset: offset,
        })
    }
}

fn detect_fat_type(boot: &[u8; 512], cluster_count: u32) -> FatType {
    if matches!(boot.get(0x42), Some(0x28) | Some(0x29)) {
        FatType::Fat32
    } else if cluster_count < 4085 {
        FatType::Fat12
    } else if cluster_count < 65525 {
        FatType::Fat16
    } else {
        FatType::Fat32
    }
}
