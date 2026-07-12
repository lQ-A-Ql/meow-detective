use super::*;
use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

const PV_SIZE: u64 = 2_097_152;
const DATA_START: u64 = 2560;
const PV0_UUID: &str = "00000000000000000000000000000000";
const PV1_UUID: &str = "11111111111111111111111111111111";

#[test]
fn probe_detects_lvm() {
    let disk = synthetic_disk();
    assert!(probe_lvm(&mut Cursor::new(&disk), 0).unwrap());
}

#[test]
fn probe_rejects_non_lvm() {
    assert!(!probe_lvm(&mut Cursor::new(vec![0u8; 2048]), 0).unwrap());
}

#[test]
fn discover_parses_volume_group() {
    let pool = discover(synthetic_disk()).unwrap();
    let vg = pool.volume_group();
    assert_eq!(
        (vg.name.as_str(), vg.extent_size, vg.seqno),
        ("test_vg", 1, 1)
    );
    assert_eq!(vg.physical_volumes[0].name, "pv0");
    let volumes = pool.list_volumes();
    assert_eq!(volumes.len(), 1);
    assert_eq!(volumes[0].name, "root");
    assert_eq!(volumes[0].size_bytes, 5 * 512);
    assert_eq!(volumes[0].role, "public");
    assert!(volumes[0].visible);
    assert!(volumes[0].directly_mappable);
    assert!(volumes[0].unsupported_reason.is_none());
    assert_eq!(pool.list_direct_volumes()[0].1.name, "root");
    assert_eq!(pool.physical_volume_offsets(), &[("pv0".to_string(), 0)]);
    assert_eq!(
        pool.physical_volume_data_offsets(),
        &[("pv0".to_string(), DATA_START)]
    );
}

#[test]
fn discover_matches_pv_uuid_case_insensitively() {
    let mut disk = synthetic_disk();
    write_metadata(
        &mut disk,
        &metadata_with(
            "ABCDEF1234567890ABCDEF1234567890",
            2,
            r#"root { id="lv-root" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=5 type="linear" stripe_count=1 stripes=["pv0",0] } }"#,
        ),
    );
    assert_eq!(discover(disk).unwrap().list_direct_volumes().len(), 1);
}

#[test]
fn discover_uses_label_data_area_as_authoritative_offset() {
    let mut disk = synthetic_disk();
    disk[584..592].copy_from_slice(&(5u64 * 512).to_le_bytes());
    refresh_label_crc(&mut disk);
    assert_eq!(
        discover(disk).unwrap().physical_volume_data_offsets(),
        &[("pv0".to_string(), 5 * 512)]
    );
}

#[test]
fn discover_fails_when_label_data_area_disagrees_with_metadata_pe_start() {
    let mut disk = synthetic_disk();
    disk[584..592].copy_from_slice(&(6u64 * 512).to_le_bytes());
    refresh_label_crc(&mut disk);
    let error = expect_discover_error(disk);
    assert!(matches!(error, LvmError::MetadataParseError { .. }));
    assert!(error.to_string().contains("data area mismatch"));
}

#[test]
fn discover_fails_when_first_extent_starts_outside_label_data_area() {
    let mut disk = synthetic_disk();
    disk[592..600].copy_from_slice(&512u64.to_le_bytes());
    refresh_label_crc(&mut disk);
    let error = expect_discover_error(disk);
    assert!(matches!(error, LvmError::MetadataParseError { .. }));
    assert!(error.to_string().contains("falls outside PV"));
}

#[test]
fn list_direct_volumes_filters_internal_and_unsupported_lvs() {
    let mut disk = synthetic_disk();
    let logical_volumes = r#"
root { id="lv-root" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="striped" stripe_count=1 stripes=["pv0",0] } }
pool_tdata { id="lv-tdata" status=["READ","WRITE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="striped" stripe_count=1 stripes=["pv0",1] } }
thin_root { id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="thin" thin_pool="pool" transaction_id=1 device_id=2 } }
"#;
    write_metadata(
        &mut disk,
        &metadata_with(DEFAULT_PV_UUID, 3, logical_volumes),
    );
    let pool = discover(disk).unwrap();
    let all = pool.list_volumes();
    assert_eq!(all.len(), 3);
    assert!(all[0].directly_mappable);
    assert_eq!(all[1].role, "thin-data");
    assert!(!all[1].visible);
    assert_eq!(all[2].role, "thin");
    assert!(all[2].visible);
    assert!(!all[2].directly_mappable);
    assert_eq!(pool.list_direct_volumes().len(), 1);
}

#[test]
fn unsupported_reason_preserves_advanced_segment_dependencies() {
    let mut disk = synthetic_disk();
    write_metadata(
        &mut disk,
        &metadata_with(DEFAULT_PV_UUID, 7, advanced_logical_volumes()),
    );
    let volumes = discover(disk).unwrap().list_volumes();
    assert_reason(&volumes, "thin_root", &["thin", "dependencies=thin_pool"]);
    assert_eq!(
        find_volume(&volumes, "thin_pool")
            .unsupported_reason
            .as_deref(),
        Some("logical volume is hidden or internal")
    );
    assert_reason(
        &volumes,
        "cached_root",
        &[
            "cache",
            "areas=origin, cache_pool",
            "dependencies=cache_pool, origin",
        ],
    );
    assert_reason(
        &volumes,
        "origin_snap",
        &[
            "snapshot",
            "areas=origin, snap_cow",
            "dependencies=origin, snap_cow",
        ],
    );
    assert_reason(
        &volumes,
        "mirrored_root",
        &[
            "raid1",
            "areas=root_rmeta_0, root_rimage_0, root_rmeta_1, root_rimage_1",
            "dependencies=root_rimage_0, root_rimage_1, root_rmeta_0, root_rmeta_1",
        ],
    );
}

#[test]
fn list_readable_volumes_includes_supported_thin_lvs_without_changing_direct_list() {
    let mut disk = synthetic_disk();
    let logical_volumes = r#"
pool_tmeta { id="lv-pool-tmeta" status=["READ","WRITE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",0] } }
pool_tdata { id="lv-pool-tdata" status=["READ","WRITE"] segment_count=1
segment1 { start_extent=0 extent_count=4 type="linear" stripe_count=1 stripes=["pv0",1] } }
thin_pool { id="lv-thin-pool" status=["READ","WRITE"] segment_count=1
segment1 { start_extent=0 extent_count=4 type="thin-pool" metadata="pool_tmeta" pool="pool_tdata" transaction_id=1 chunk_size=128 } }
thin_root { id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=2 type="thin" thin_pool="thin_pool" transaction_id=1 device_id=7 } }
origin { id="lv-origin" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",10] } }
"#;
    write_metadata(
        &mut disk,
        &metadata_with(DEFAULT_PV_UUID, 8, logical_volumes),
    );
    let pool = discover(disk).unwrap();
    assert_eq!(volume_names(pool.list_direct_volumes()), vec!["origin"]);
    assert_eq!(
        volume_names(pool.list_readable_volumes()),
        vec!["thin_root", "origin"]
    );
}

#[test]
fn discover_resolves_component_lv_area_backed_by_physical_volume() {
    let mut disk = synthetic_disk();
    let logical_volumes = r#"
component_lv { id="lv-component" status=["READ","WRITE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",0] } }
direct_root { id="lv-direct" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",1] } }
component_backed { id="lv-component-backed" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["component_lv",0] } }
"#;
    write_metadata(
        &mut disk,
        &metadata_with(DEFAULT_PV_UUID, 4, logical_volumes),
    );
    let marker = b"COMPONENT-BACKED";
    disk[DATA_START as usize..DATA_START as usize + marker.len()].copy_from_slice(marker);
    let pool = discover(disk).unwrap();
    let direct = pool.list_direct_volumes();
    assert_eq!(
        volume_names(direct.clone()),
        vec!["direct_root", "component_backed"]
    );
    assert_eq!((direct[0].0, direct[1].0), (1, 2));
    let mut reader = pool.open_volume(2).unwrap();
    let mut bytes = vec![0u8; marker.len()];
    reader.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, marker);
}

#[test]
fn discover_fails_closed_on_cyclic_component_lv_area() {
    let mut disk = synthetic_disk();
    let logical_volumes = r#"
cycle_a { id="lv-cycle-a" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["cycle_b",0] } }
cycle_b { id="lv-cycle-b" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["cycle_a",0] } }
"#;
    write_metadata(
        &mut disk,
        &metadata_with(DEFAULT_PV_UUID, 5, logical_volumes),
    );
    let error = expect_discover_error(disk);
    assert!(matches!(error, LvmError::MetadataParseError { .. }));
    assert!(error.to_string().contains("cyclic LVM"));
}

#[test]
fn discover_fails_closed_when_component_lv_depends_on_thin_volume() {
    let mut disk = synthetic_disk();
    let logical_volumes = r#"
thin_component { id="lv-thin-component" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="thin" thin_pool="pool" transaction_id=1 device_id=7 } }
component_backed { id="lv-component-backed" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["thin_component",0] } }
"#;
    write_metadata(
        &mut disk,
        &metadata_with(DEFAULT_PV_UUID, 6, logical_volumes),
    );
    assert!(expect_discover_error(disk).to_string().contains("thin"));
}

#[test]
fn discover_matches_label_uuid_to_dashed_metadata_uuid() {
    let mut disk = synthetic_disk();
    write_metadata(
        &mut disk,
        &metadata_with(
            "abcdef12-3456-7890-abcd-ef1234567890",
            2,
            r#"root { id="lv-root" segment_count=1
segment1 { start_extent=0 extent_count=5 type="striped" stripe_count=1 stripes=["pv0",0] } }"#,
        ),
    );
    let pool = discover(disk).unwrap();
    assert_eq!(
        pool.volume_group().physical_volumes[0].uuid,
        "abcdef12-3456-7890-abcd-ef1234567890"
    );
    assert_eq!(pool.list_volumes().len(), 1);
}

#[test]
fn open_volume_reads_data() {
    let mut disk = synthetic_disk();
    let marker = b"FORENSIC TEST DATA AT LV OFFSET 0";
    disk[DATA_START as usize..DATA_START as usize + marker.len()].copy_from_slice(marker);
    let mut reader = discover(disk).unwrap().open_volume(0).unwrap();
    let mut bytes = vec![0u8; marker.len()];
    reader.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, marker);
}

#[test]
fn discover_binds_readers_in_metadata_pv_order() {
    let (mut pv0, mut pv1) = multi_pv_disks();
    pv0[DATA_START as usize..DATA_START as usize + 512].fill(b'A');
    pv1[DATA_START as usize..DATA_START as usize + 512].fill(b'B');
    let pool = LvmPool::discover(vec![boxed_reader(pv1), boxed_reader(pv0)], vec![0, 0]).unwrap();
    assert_eq!(
        pool.physical_volume_offsets(),
        &[("pv0".to_string(), 0), ("pv1".to_string(), 0)]
    );
    let mut reader = pool.open_volume(0).unwrap();
    let mut bytes = vec![0u8; 1024];
    reader.read_exact(&mut bytes).unwrap();
    assert!(bytes[..512].iter().all(|byte| *byte == b'A'));
    assert!(bytes[512..].iter().all(|byte| *byte == b'B'));
}

#[test]
fn discover_missing_metadata_pv_reader_fails_closed() {
    let (pv0, _) = multi_pv_disks();
    let error = match LvmPool::discover(vec![boxed_reader(pv0)], vec![0]) {
        Ok(_) => panic!("missing PV reader must fail closed"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        LvmError::MissingPhysicalVolumeReader { pv_name, pv_uuid }
            if pv_name == "pv1" && pv_uuid == PV1_UUID
    ));
}

#[test]
fn discover_uses_complete_lower_seqno_copy_when_higher_copy_is_incomplete() {
    let complete = multi_pv_metadata(10);
    let incomplete = complete.replace("seqno=10", "seqno=99").replace(
        &format!("pv0 {{ id=\"{PV0_UUID}\" pe_start=5 pe_count=16 }}"),
        &format!("pv0 {{ id=\"{PV0_UUID}\" pe_count=16 }}"),
    );
    let mut pv0 = empty_pv(PV0_UUID);
    let mut pv1 = empty_pv(PV1_UUID);
    write_metadata(&mut pv0, &incomplete);
    write_metadata(&mut pv1, &complete);
    let pool = LvmPool::discover(vec![boxed_reader(pv0), boxed_reader(pv1)], vec![0, 0]).unwrap();
    assert_eq!(pool.volume_group().seqno, 10);
    assert_eq!(pool.list_direct_volumes().len(), 1);
}

const DEFAULT_PV_UUID: &str = "abcdef1234567890abcdef1234567890";

fn synthetic_disk() -> Vec<u8> {
    let mut disk = empty_pv(DEFAULT_PV_UUID);
    write_metadata(
        &mut disk,
        &metadata_with(
            DEFAULT_PV_UUID,
            1,
            r#"root { id="lv-root-uuid-1234-5678" segment_count=1
segment1 { start_extent=0 extent_count=5 type="striped" stripe_count=1 stripes=["pv0",0] } }"#,
        ),
    );
    disk
}

fn empty_pv(uuid: &str) -> Vec<u8> {
    let mut disk = vec![0u8; PV_SIZE as usize];
    let sector = &mut disk[512..1024];
    sector[0..8].copy_from_slice(b"LABELONE");
    sector[8..16].copy_from_slice(&1u64.to_le_bytes());
    sector[20..24].copy_from_slice(&32u32.to_le_bytes());
    sector[24..32].copy_from_slice(b"LVM2 001");
    sector[32..64].copy_from_slice(format!("{uuid:32}").as_bytes());
    sector[64..72].copy_from_slice(&PV_SIZE.to_le_bytes());
    sector[72..80].copy_from_slice(&DATA_START.to_le_bytes());
    sector[80..88].copy_from_slice(&(PV_SIZE - DATA_START).to_le_bytes());
    sector[104..112].copy_from_slice(&1024u64.to_le_bytes());
    sector[112..120].copy_from_slice(&(4 * 512u64).to_le_bytes());
    refresh_label_crc(&mut disk);
    disk
}

fn write_metadata(disk: &mut [u8], text: &str) {
    let bytes = text.as_bytes();
    let end = 1536 + bytes.len();
    assert!(end <= disk.len());
    disk[1536..end].copy_from_slice(bytes);
    disk[end..].fill(0);
    let mda_size = (bytes.len() as u64 + 1024).next_power_of_two();
    let mda = &mut disk[1024..1536];
    mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
    mda[20..24].copy_from_slice(&1u32.to_le_bytes());
    mda[24..32].copy_from_slice(&1024u64.to_le_bytes());
    mda[32..40].copy_from_slice(&mda_size.to_le_bytes());
    mda[40..48].copy_from_slice(&512u64.to_le_bytes());
    mda[48..56].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
    mda[56..60].copy_from_slice(&crc::lvm_crc32(bytes).to_le_bytes());
    let mda_crc = crc::lvm_crc32(&mda[4..512]);
    mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
    disk[624..632].copy_from_slice(&mda_size.to_le_bytes());
    refresh_label_crc(disk);
}

fn refresh_label_crc(disk: &mut [u8]) {
    let sector = &mut disk[512..1024];
    let checksum = crc::lvm_crc32(&sector[20..512]);
    sector[16..20].copy_from_slice(&checksum.to_le_bytes());
}

fn metadata_with(pv_uuid: &str, seqno: u64, logical_volumes: &str) -> String {
    format!(
        r#"test_vg {{
id="vg-test"
seqno={seqno}
extent_size=1
physical_volumes {{ pv0 {{ id="{pv_uuid}" pe_start=5 pe_count=4096 }} }}
logical_volumes {{ {logical_volumes} }}
}}
"#
    )
}

fn advanced_logical_volumes() -> &'static str {
    r#"
pool_tmeta { id="lv-pool-tmeta" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",0] } }
pool_tdata { id="lv-pool-tdata" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=4 type="linear" stripe_count=1 stripes=["pv0",1] } }
thin_pool { id="lv-thin-pool" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=4 type="thin-pool" metadata="pool_tmeta" pool="pool_tdata" transaction_id=1 chunk_size=128 } }
thin_root { id="lv-thin-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="thin" thin_pool="thin_pool" transaction_id=1 device_id=7 } }
origin { id="lv-origin" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",10] } }
cache_cmeta { id="lv-cache-cmeta" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",12] } }
cache_cdata { id="lv-cache-cdata" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",13] } }
cache_pool { id="lv-cache-pool" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="cache-pool" metadata="cache_cmeta" data="cache_cdata" chunk_size=64 } }
cached_root { id="lv-cached-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="cache" cache_pool="cache_pool" origin="origin" } }
snap_cow { id="lv-snap-cow" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",15] } }
origin_snap { id="lv-origin-snap" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="snapshot" origin="origin" cow_store="snap_cow" chunk_size=8 } }
root_rmeta_0 { id="lv-root-rmeta-0" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",17] } }
root_rimage_0 { id="lv-root-rimage-0" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",18] } }
root_rmeta_1 { id="lv-root-rmeta-1" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=1 type="linear" stripe_count=1 stripes=["pv0",20] } }
root_rimage_1 { id="lv-root-rimage-1" status=["READ","WRITE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="linear" stripe_count=1 stripes=["pv0",21] } }
mirrored_root { id="lv-mirrored-root" status=["READ","WRITE","VISIBLE"] segment_count=1 segment1 { start_extent=0 extent_count=2 type="raid1" device_count=2 raids=["root_rmeta_0","root_rimage_0","root_rmeta_1","root_rimage_1"] } }
"#
}

fn multi_pv_metadata(seqno: u64) -> String {
    format!(
        r#"test_vg {{
id="vg-multi"
seqno={seqno}
extent_size=1
physical_volumes {{
pv0 {{ id="{PV0_UUID}" pe_start=5 pe_count=16 }}
pv1 {{ id="{PV1_UUID}" pe_start=5 pe_count=16 }}
}}
logical_volumes {{
root {{ id="lv-root" status=["READ","WRITE","VISIBLE"] segment_count=1
segment1 {{ start_extent=0 extent_count=2 type="striped" stripe_count=2 stripe_size=1 stripes=["pv0",0,"pv1",0] }} }}
}}
}}
"#
    )
}

fn multi_pv_disks() -> (Vec<u8>, Vec<u8>) {
    let metadata = multi_pv_metadata(2);
    let mut pv0 = empty_pv(PV0_UUID);
    let mut pv1 = empty_pv(PV1_UUID);
    write_metadata(&mut pv0, &metadata);
    write_metadata(&mut pv1, &metadata);
    (pv0, pv1)
}

fn discover(disk: Vec<u8>) -> Result<LvmPool> {
    LvmPool::discover(vec![boxed_reader(disk)], vec![0])
}

fn expect_discover_error(disk: Vec<u8>) -> LvmError {
    match discover(disk) {
        Ok(_) => panic!("LVM discovery should have failed"),
        Err(error) => error,
    }
}

fn boxed_reader(disk: Vec<u8>) -> Box<dyn EvidenceReader> {
    Box::new(FakeDiskReader::new(disk))
}

fn volume_names(volumes: Vec<(usize, LvInfo)>) -> Vec<String> {
    volumes.into_iter().map(|(_, volume)| volume.name).collect()
}

fn find_volume<'a>(volumes: &'a [LvInfo], name: &str) -> &'a LvInfo {
    volumes.iter().find(|volume| volume.name == name).unwrap()
}

fn assert_reason(volumes: &[LvInfo], name: &str, fragments: &[&str]) {
    let reason = find_volume(volumes, name)
        .unsupported_reason
        .as_deref()
        .unwrap();
    for fragment in fragments {
        assert!(reason.contains(fragment), "{reason:?} lacks {fragment:?}");
    }
}

struct FakeDiskReader {
    data: Vec<u8>,
    position: u64,
    info: ReaderInfo,
}

impl FakeDiskReader {
    fn new(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            position: 0,
            info: ReaderInfo {
                path: PathBuf::from("<memory>"),
                size,
                kind: "memory".to_string(),
            },
        }
    }
}

impl Read for FakeDiskReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let start = self.position as usize;
        let end = (start + buffer.len()).min(self.data.len());
        let length = end.saturating_sub(start);
        buffer[..length].copy_from_slice(&self.data[start..end]);
        self.position += length as u64;
        Ok(length)
    }
}

impl Seek for FakeDiskReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.data.len() as i64 + offset).max(0) as u64,
            SeekFrom::Current(offset) => (self.position as i64 + offset).max(0) as u64,
        };
        Ok(self.position)
    }
}

impl EvidenceReader for FakeDiskReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
