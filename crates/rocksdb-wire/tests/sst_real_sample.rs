use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use rocksdb_wire::{
    inspect_sst, ChecksumType, IndexKeyKind, KeySpaceCensusContext, RangeReader, SstReadOptions,
    BLOCK_BASED_TABLE_MAGIC,
};

const REPRESENTATIVE_FILE_NUMBER: u64 = 146;
const REPRESENTATIVE_SST_ENV: &str = "FORENSICS_PVE_SST_FIXTURE";

#[test]
#[ignore = "requires an exported private PVE live SST fixture"]
fn representative_pve_sst_matches_independent_sst_dump_oracle() {
    let path = representative_sst_path();
    assert!(
        path.is_file(),
        "{REPRESENTATIVE_SST_ENV} must name the exported 000146.sst fixture"
    );
    let file_size = std::fs::metadata(&path).expect("read SST metadata").len();
    let mut reader = FileRangeReader::open(&path).expect("open representative SST");

    let context = KeySpaceCensusContext::unclassified("default", "oracle.unclassified")
        .expect("valid real-oracle context");
    let inspected = inspect_sst(&mut reader, file_size, SstReadOptions::default(), &context)
        .expect("inspect real SST");

    assert_eq!(file_size, 307_253);
    assert_eq!(inspected.footer.format_version, 5);
    assert_eq!(inspected.footer.checksum_type, ChecksumType::Xxh3);
    assert_eq!(inspected.footer.table_magic, BLOCK_BASED_TABLE_MAGIC);
    assert_eq!(
        inspected.properties.original_file_number,
        REPRESENTATIVE_FILE_NUMBER
    );
    assert_eq!(inspected.properties.column_family_id, 0);
    assert_eq!(inspected.properties.column_family_name, "default");
    assert_eq!(inspected.properties.properties_format_version, 0);
    assert!(inspected.properties.index_key_is_user_key);
    assert!(inspected.properties.index_value_is_delta_encoded);
    assert_eq!(inspected.properties.index_type, 0);
    assert_eq!(inspected.properties.index_partitions, 0);
    assert_eq!(
        inspected.properties.comparator_name,
        "leveldb.BytewiseComparator"
    );
    assert_eq!(inspected.properties.num_data_blocks, 148);
    assert_eq!(inspected.properties.num_entries, 23_364);
    assert_eq!(inspected.properties.deleted_keys, 0);
    assert_eq!(inspected.properties.merge_operands, 0);
    assert_eq!(inspected.properties.num_range_deletions, 0);
    assert_eq!(inspected.properties.raw_key_size, 420_609);
    assert_eq!(inspected.properties.raw_value_size, 298_145);
    assert_eq!(inspected.properties.data_size, 245_834);
    assert_eq!(inspected.properties.index_size, 3_106);
    assert_eq!(inspected.properties.filter_size, 58_437);
    assert_eq!(inspected.properties.compression_name, "LZ4");
    assert_eq!(
        inspected.properties.db_identity.as_deref(),
        Some("318c61d3-7d8b-497a-b02a-d3683123595d")
    );
    assert_eq!(
        inspected.properties.db_session_identity.as_deref(),
        Some("1XCA20Z4TSA1B37NG18K")
    );
    assert_eq!(inspected.data_blocks.len(), 148);
    assert_eq!(inspected.counts.entries, 23_364);
    assert_eq!(inspected.counts.deletions, 0);
    assert_eq!(inspected.raw_key_size, 420_609);
    assert_eq!(inspected.raw_value_size, 298_145);
    assert_eq!(inspected.first_index_key.kind, IndexKeyKind::User);
    assert_eq!(inspected.first_index_key.sequence, None);
    assert_eq!(inspected.last_index_key.kind, IndexKeyKind::User);
    assert!(inspected.census.complete);
}

fn representative_sst_path() -> std::path::PathBuf {
    std::env::var_os(REPRESENTATIVE_SST_ENV)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new("__missing_pve_sst_fixture__").to_path_buf())
}

struct FileRangeReader {
    file: File,
}

impl FileRangeReader {
    fn open(path: &Path) -> std::io::Result<Self> {
        File::open(path).map(|file| Self { file })
    }
}

impl RangeReader for FileRangeReader {
    type Error = FileRangeError;

    fn read_range(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, Self::Error> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(FileRangeError)?;
        let mut bytes = vec![0; length];
        self.file.read_exact(&mut bytes).map_err(FileRangeError)?;
        Ok(bytes)
    }
}

#[derive(Debug)]
struct FileRangeError(std::io::Error);

impl Display for FileRangeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl std::error::Error for FileRangeError {}
