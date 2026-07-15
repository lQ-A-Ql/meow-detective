use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use evidence_core::{EvidenceReader, ReaderInfo};
use persistence_sqlite::repositories::{
    ceph_osd_device_binding_repo::{
        CephOsdDeviceBindingAggregate, CephOsdDeviceBindingRecord, CephOsdPvBindingRecord,
    },
    ceph_osd_repo::{CephOsdInventoryRecord, CephOsdRepo},
};

use super::*;

const DATA_SOURCE_ID: &str = "source-1";
const INVENTORY_ID: &str = "inventory-1";
const VG_UUID: &str = "vg-test-uuid";
const VG_NAME: &str = "ceph-vg";
const LV_UUID: &str = "lv-test-uuid";
const LV_NAME: &str = "osd-block";
const PV_UUID: &str = "abcdef1234567890abcdef1234567890";
const PV_SIZE: u64 = 2_097_152;
const LV_SIZE: u64 = 512;
const LV_DATA_OFFSET: usize = 2560;

struct MemoryReader {
    data: Vec<u8>,
    position: u64,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(data: Vec<u8>, path: PathBuf) -> Self {
        Self {
            info: ReaderInfo {
                path,
                size: data.len() as u64,
                kind: "synthetic".to_string(),
            },
            data,
            position: 0,
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let start = self.position as usize;
        let end = start.saturating_add(buffer.len()).min(self.data.len());
        let length = end.saturating_sub(start);
        buffer[..length].copy_from_slice(&self.data[start..end]);
        self.position += length as u64;
        Ok(length)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.data.len() as i64 + offset).max(0) as u64,
            SeekFrom::Current(offset) => (self.position as i64 + offset).max(0) as u64,
        };
        Ok(self.position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

struct SyntheticOpener {
    disk: Vec<u8>,
}

impl SourceBoundEvidenceOpener for SyntheticOpener {
    fn open(
        &self,
        path: &Path,
        _kind: &DataSourceKind,
    ) -> Result<Box<dyn EvidenceReader>, BoundEvidenceOpenError> {
        Ok(Box::new(MemoryReader::new(
            self.disk.clone(),
            path.to_path_buf(),
        )))
    }
}

#[test]
fn filesystem_opener_rejects_ceph_rbd_as_host_evidence() {
    let error = match FilesystemEvidenceOpener.open(
        Path::new("C:/path-that-must-not-be-opened"),
        &DataSourceKind::CephRbd,
    ) {
        Ok(_) => panic!("Ceph RBD must not be opened as host evidence"),
        Err(error) => error,
    };

    assert_eq!(error.kind, std::io::ErrorKind::Unsupported);
}

#[test]
fn reopens_source_bound_lvm_with_synthetic_reader() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let evidence_path = create_evidence_placeholder(temp.path(), "source.raw");
    let conn = setup_source_db(DATA_SOURCE_ID, &evidence_path);
    persist_binding(&conn, DATA_SOURCE_ID, INVENTORY_ID, &evidence_path, PV_UUID);
    let opener = SyntheticOpener {
        disk: synthetic_lvm_disk(PV_UUID),
    };

    let mut reader = open_source_bound_bluestore_lvm_with_opener(
        &conn,
        &DataSourceId(DATA_SOURCE_ID.to_string()),
        INVENTORY_ID,
        &opener,
    )
    .expect("reopen source-bound LVM");

    assert_eq!(reader.info().size, LV_SIZE);
    let mut marker = [0u8; 8];
    reader.read_exact(&mut marker).expect("read logical volume");
    assert_eq!(&marker, b"BLUESTOR");
}

#[test]
fn rejects_stale_binding_when_source_registration_changes() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let original = create_evidence_placeholder(temp.path(), "original.raw");
    let replacement = create_evidence_placeholder(temp.path(), "replacement.raw");
    let conn = setup_source_db(DATA_SOURCE_ID, &original);
    persist_binding(&conn, DATA_SOURCE_ID, INVENTORY_ID, &original, PV_UUID);
    conn.execute(
        "UPDATE data_sources SET source_path = ?1 WHERE id = ?2",
        rusqlite::params![replacement.display().to_string(), DATA_SOURCE_ID],
    )
    .expect("change source registration");

    let error = expect_open_error(
        open_source_bound_bluestore_lvm_with_opener(
            &conn,
            &DataSourceId(DATA_SOURCE_ID.to_string()),
            INVENTORY_ID,
            &SyntheticOpener {
                disk: synthetic_lvm_disk(PV_UUID),
            },
        ),
        "stale binding must fail",
    );
    assert!(matches!(
        error,
        SourceBoundLvmError::NeedReassociate {
            reason: NeedReassociateReason::BindingRegistrationMismatch
        }
    ));
}

#[test]
fn missing_registered_path_requires_reassociation() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let missing = temp.path().join("missing.raw");
    let conn = setup_source_db_with_canonical(DATA_SOURCE_ID, &missing, &missing);
    persist_binding_with_canonical(
        &conn,
        DATA_SOURCE_ID,
        INVENTORY_ID,
        &missing,
        &missing,
        PV_UUID,
    );

    let error = expect_open_error(
        open_source_bound_bluestore_lvm_with_opener(
            &conn,
            &DataSourceId(DATA_SOURCE_ID.to_string()),
            INVENTORY_ID,
            &SyntheticOpener {
                disk: synthetic_lvm_disk(PV_UUID),
            },
        ),
        "missing source path must fail",
    );
    assert!(matches!(
        error,
        SourceBoundLvmError::NeedReassociate {
            reason: NeedReassociateReason::RegisteredSourcePathMissing
        }
    ));
}

#[test]
fn rejects_physical_volume_uuid_mismatch() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let evidence_path = create_evidence_placeholder(temp.path(), "source.raw");
    let conn = setup_source_db(DATA_SOURCE_ID, &evidence_path);
    persist_binding(
        &conn,
        DATA_SOURCE_ID,
        INVENTORY_ID,
        &evidence_path,
        "ffffffffffffffffffffffffffffffff",
    );

    let error = expect_open_error(
        open_source_bound_bluestore_lvm_with_opener(
            &conn,
            &DataSourceId(DATA_SOURCE_ID.to_string()),
            INVENTORY_ID,
            &SyntheticOpener {
                disk: synthetic_lvm_disk(PV_UUID),
            },
        ),
        "PV UUID mismatch must fail",
    );
    assert!(matches!(
        error,
        SourceBoundLvmError::PhysicalVolumeUuidMismatch { ordinal: 0 }
    ));
}

#[test]
fn source_id_cannot_route_to_another_sources_binding() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let source_one = create_evidence_placeholder(temp.path(), "source-one.raw");
    let source_two = create_evidence_placeholder(temp.path(), "source-two.raw");
    let conn = setup_source_db(DATA_SOURCE_ID, &source_one);
    insert_source_metadata(&conn, "source-2", &source_two, &canonical(&source_two));
    persist_binding(&conn, DATA_SOURCE_ID, INVENTORY_ID, &source_one, PV_UUID);

    let error = expect_open_error(
        open_source_bound_bluestore_lvm_with_opener(
            &conn,
            &DataSourceId("source-2".to_string()),
            INVENTORY_ID,
            &SyntheticOpener {
                disk: synthetic_lvm_disk(PV_UUID),
            },
        ),
        "cross-source routing must fail",
    );
    assert!(matches!(error, SourceBoundLvmError::BindingNotFound));
}

fn setup_source_db(data_source_id: &str, source_path: &Path) -> rusqlite::Connection {
    let canonical_path = canonical(source_path);
    setup_source_db_with_canonical(data_source_id, source_path, &canonical_path)
}

fn setup_source_db_with_canonical(
    data_source_id: &str,
    source_path: &Path,
    canonical_path: &Path,
) -> rusqlite::Connection {
    let conn = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&conn).expect("run source migrations");
    insert_source_metadata(&conn, data_source_id, source_path, canonical_path);
    conn
}

fn insert_source_metadata(
    conn: &rusqlite::Connection,
    data_source_id: &str,
    source_path: &Path,
    canonical_path: &Path,
) {
    conn.execute(
        "INSERT INTO data_sources (
            id, case_id, name, kind, source_path, canonical_source_path, imported_at
         ) VALUES (?1, 'case-1', ?1, 'raw', ?2, ?3, '2026-07-15T00:00:00Z')",
        rusqlite::params![
            data_source_id,
            source_path.display().to_string(),
            canonical_path.display().to_string()
        ],
    )
    .expect("insert source metadata");
}

fn persist_binding(
    conn: &rusqlite::Connection,
    data_source_id: &str,
    inventory_id: &str,
    source_path: &Path,
    pv_uuid: &str,
) {
    let canonical_path = canonical(source_path);
    persist_binding_with_canonical(
        conn,
        data_source_id,
        inventory_id,
        source_path,
        &canonical_path,
        pv_uuid,
    );
}

fn persist_binding_with_canonical(
    conn: &rusqlite::Connection,
    data_source_id: &str,
    inventory_id: &str,
    source_path: &Path,
    canonical_path: &Path,
    pv_uuid: &str,
) {
    let inventory = inventory(inventory_id, data_source_id);
    let binding = binding(
        inventory_id,
        data_source_id,
        source_path,
        canonical_path,
        pv_uuid,
    );
    CephOsdRepo::new(conn)
        .replace_for_data_source_with_device_bindings(
            data_source_id,
            std::slice::from_ref(&inventory),
            &[],
            std::slice::from_ref(&binding),
        )
        .expect("persist source-bound device");
}

fn inventory(inventory_id: &str, data_source_id: &str) -> CephOsdInventoryRecord {
    CephOsdInventoryRecord {
        id: inventory_id.to_string(),
        data_source_id: data_source_id.to_string(),
        partition_index: None,
        lvm_vg_uuid: Some(VG_UUID.to_string()),
        lvm_vg_name: Some(VG_NAME.to_string()),
        lvm_lv_uuid: Some(LV_UUID.to_string()),
        lvm_lv_name: Some(LV_NAME.to_string()),
        osd_uuid: "osd-test".to_string(),
        ceph_fsid: None,
        whoami: Some(1),
        device_role: "block".to_string(),
        device_size: LV_SIZE,
        birth_time_seconds: 0,
        birth_time_nanoseconds: 0,
        description: "test".to_string(),
        is_multi: false,
        selected_epoch: None,
        valid_label_count: 1,
        label_health: "singleReplica".to_string(),
        osd_key_present: false,
        kv_backend: None,
        bluefs_enabled: Some(false),
        ceph_version_when_created: None,
        require_osd_release: None,
        sanitized_metadata_json: "{}".to_string(),
    }
}

fn binding(
    inventory_id: &str,
    data_source_id: &str,
    source_path: &Path,
    canonical_path: &Path,
    pv_uuid: &str,
) -> CephOsdDeviceBindingAggregate {
    let source_path = source_path.display().to_string();
    let canonical_source_path = canonical_path.display().to_string();
    CephOsdDeviceBindingAggregate {
        device: CephOsdDeviceBindingRecord {
            inventory_id: inventory_id.to_string(),
            data_source_id: data_source_id.to_string(),
            source_path: source_path.clone(),
            canonical_source_path: canonical_source_path.clone(),
            source_kind: "raw".to_string(),
            lvm_vg_uuid: VG_UUID.to_string(),
            lvm_vg_name: VG_NAME.to_string(),
            lvm_lv_uuid: LV_UUID.to_string(),
            lvm_lv_name: LV_NAME.to_string(),
            device_size: LV_SIZE,
        },
        physical_volumes: vec![CephOsdPvBindingRecord {
            inventory_id: inventory_id.to_string(),
            ordinal: 0,
            source_path,
            canonical_source_path,
            source_kind: "raw".to_string(),
            pv_offset: 0,
            pv_uuid: pv_uuid.to_string(),
            pv_name: Some("pv0".to_string()),
        }],
    }
}

fn create_evidence_placeholder(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    std::fs::write(&path, b"placeholder").expect("write evidence placeholder");
    path
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).expect("canonicalize evidence path")
}

fn expect_open_error(
    result: Result<Box<dyn EvidenceReader>, SourceBoundLvmError>,
    message: &str,
) -> SourceBoundLvmError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

fn synthetic_lvm_disk(pv_uuid: &str) -> Vec<u8> {
    let mut disk = vec![0u8; PV_SIZE as usize];
    write_lvm_label(&mut disk, pv_uuid);
    write_lvm_metadata(&mut disk, pv_uuid);
    disk[LV_DATA_OFFSET..LV_DATA_OFFSET + 8].copy_from_slice(b"BLUESTOR");
    disk
}

fn write_lvm_label(disk: &mut [u8], pv_uuid: &str) {
    let sector = &mut disk[512..1024];
    sector[0..8].copy_from_slice(b"LABELONE");
    sector[8..16].copy_from_slice(&1u64.to_le_bytes());
    sector[20..24].copy_from_slice(&32u32.to_le_bytes());
    sector[24..32].copy_from_slice(b"LVM2 001");
    sector[32..64].copy_from_slice(format!("{pv_uuid:32}").as_bytes());
    sector[64..72].copy_from_slice(&PV_SIZE.to_le_bytes());
    sector[72..80].copy_from_slice(&(LV_DATA_OFFSET as u64).to_le_bytes());
    sector[80..88].copy_from_slice(&(PV_SIZE - LV_DATA_OFFSET as u64).to_le_bytes());
    sector[104..112].copy_from_slice(&1024u64.to_le_bytes());
    sector[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());
    let checksum = fs_lvm::crc::lvm_crc32(&sector[20..512]);
    sector[16..20].copy_from_slice(&checksum.to_le_bytes());
}

fn write_lvm_metadata(disk: &mut [u8], pv_uuid: &str) {
    let metadata = format!(
        r#"{VG_NAME} {{
id="{VG_UUID}"
seqno=1
extent_size=1
physical_volumes {{ pv0 {{ id="{pv_uuid}" pe_start=5 pe_count=128 }} }}
logical_volumes {{
{LV_NAME} {{ id="{LV_UUID}" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 {{ start_extent=0 extent_count=1 type="striped" stripe_count=1 stripes=["pv0",0] }} }}
}}
}}
"#
    );
    let bytes = metadata.as_bytes();
    let text_offset = 1536usize;
    disk[text_offset..text_offset + bytes.len()].copy_from_slice(bytes);

    let mda = &mut disk[1024..1536];
    mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
    mda[20..24].copy_from_slice(&1u32.to_le_bytes());
    mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
    mda[32..40].copy_from_slice(&1536u64.to_le_bytes());
    mda[40..48].copy_from_slice(&512u64.to_le_bytes());
    mda[48..56].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
    mda[56..60].copy_from_slice(&fs_lvm::crc::lvm_crc32(bytes).to_le_bytes());
    let checksum = fs_lvm::crc::lvm_crc32(&mda[4..512]);
    mda[0..4].copy_from_slice(&checksum.to_le_bytes());
}
