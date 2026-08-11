use super::*;
use std::io::ErrorKind;
use std::sync::{Arc, Mutex};

const BPS: usize = 512;
const RESERVED: usize = 32;
const FAT_SECTORS: usize = 16;
const FAT_COUNT: usize = 2;
const FIRST_DATA: usize = RESERVED + FAT_COUNT * FAT_SECTORS;
const TOTAL_SECTORS: usize = FIRST_DATA + 200;
const ROOT_CLUSTER: u32 = 2;

fn cluster_pos(cluster: u32) -> usize {
    (FIRST_DATA + (cluster as usize - ROOT_CLUSTER as usize)) * BPS
}

fn fat_pos(fat_index: usize, cluster: u32) -> usize {
    (RESERVED + fat_index * FAT_SECTORS) * BPS + cluster as usize * 4
}

fn put_dir_entry(
    data: &mut [u8],
    slot: usize,
    name: &str,
    ext: &str,
    attr: u8,
    cluster: u32,
    size: u32,
) {
    let entry = &mut data[slot * 32..(slot + 1) * 32];
    for byte in entry.iter_mut() {
        *byte = 0;
    }
    entry[..8].fill(b' ');
    entry[..name.len()].copy_from_slice(name.as_bytes());
    entry[8..11].fill(b' ');
    entry[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    entry[11] = attr;
    entry[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
}

/// A small FAT32 volume with `\EFI\KALI\GRUBX64.EFI` populated:
/// cluster 2 = root, 3 = EFI, 4 = KALI, 5 = the grub payload.
fn build_esp_fixture() -> Vec<u8> {
    let mut data = vec![0u8; TOTAL_SECTORS * BPS];
    data[11..13].copy_from_slice(&(BPS as u16).to_le_bytes());
    data[13] = 1;
    data[14..16].copy_from_slice(&(RESERVED as u16).to_le_bytes());
    data[16] = FAT_COUNT as u8;
    data[32..36].copy_from_slice(&(TOTAL_SECTORS as u32).to_le_bytes());
    data[36..40].copy_from_slice(&(FAT_SECTORS as u32).to_le_bytes());
    data[44..48].copy_from_slice(&ROOT_CLUSTER.to_le_bytes());
    data[66] = 0x29; // FAT32 extended boot signature forces the type check
                     // FSInfo sector (spec default sector 1).
    data[BPS..BPS + 4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    data[BPS + 484..BPS + 488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    data[BPS + 488..BPS + 492].copy_from_slice(&190u32.to_le_bytes());
    data[BPS + 492..BPS + 496].copy_from_slice(&2u32.to_le_bytes());
    for fat in 0..FAT_COUNT {
        for (cluster, value) in [
            (0, 0x0FFF_FFF8u32),
            (1, 0x0FFF_FFFF),
            (2, 0x0FFF_FFFF),
            (3, 0x0FFF_FFFF),
            (4, 0x0FFF_FFFF),
            (5, 0x0FFF_FFFF),
        ] {
            data[fat_pos(fat, cluster)..fat_pos(fat, cluster) + 4]
                .copy_from_slice(&value.to_le_bytes());
        }
    }
    let root = cluster_pos(2);
    put_dir_entry(&mut data[root..root + BPS], 0, "EFI", "", 0x10, 3, 0);
    let efi = cluster_pos(3);
    put_dir_entry(&mut data[efi..efi + BPS], 0, ".", "", 0x10, 3, 0);
    put_dir_entry(&mut data[efi..efi + BPS], 1, "..", "", 0x10, 0, 0);
    put_dir_entry(&mut data[efi..efi + BPS], 2, "KALI", "", 0x10, 4, 0);
    let kali = cluster_pos(4);
    put_dir_entry(&mut data[kali..kali + BPS], 0, ".", "", 0x10, 4, 0);
    put_dir_entry(&mut data[kali..kali + BPS], 1, "..", "", 0x10, 3, 0);
    put_dir_entry(&mut data[kali..kali + BPS], 2, "GRUBX64", "EFI", 0x20, 5, 4);
    let grub = cluster_pos(5);
    data[grub..grub + 4].copy_from_slice(b"GRUB");
    data
}

struct MemIo(Arc<Mutex<Vec<u8>>>);

impl crate::FatBlockIo for MemIo {
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
        let data = self.0.lock().unwrap();
        buffer.copy_from_slice(&data[offset as usize..offset as usize + buffer.len()]);
        Ok(())
    }

    fn write_at(&self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let mut data = self.0.lock().unwrap();
        data[offset as usize..offset as usize + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

/// Live view over the shared buffer: reads observe prior MemIo writes, which
/// `install_efi_fallback` relies on when revisiting a directory it just
/// extended.
struct LiveReader {
    data: Arc<Mutex<Vec<u8>>>,
    pos: u64,
    info: evidence_core::ReaderInfo,
}

impl LiveReader {
    fn new(data: Arc<Mutex<Vec<u8>>>) -> Self {
        Self {
            data,
            pos: 0,
            info: evidence_core::ReaderInfo {
                path: std::path::PathBuf::from("mem-esp"),
                size: 0,
                kind: "mem-esp".to_string(),
            },
        }
    }
}

impl Read for LiveReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let data = self.data.lock().unwrap();
        let start = self.pos.min(data.len() as u64) as usize;
        let end = (start + buf.len()).min(data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for LiveReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let len = self.data.lock().unwrap().len() as u64;
        self.pos = match pos {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(delta) => (len as i64 + delta).max(0) as u64,
            SeekFrom::Current(delta) => (self.pos as i64 + delta).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl evidence_core::EvidenceReader for LiveReader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}

fn shared_reader(data: Arc<Mutex<Vec<u8>>>) -> Box<dyn evidence_core::EvidenceReader> {
    Box::new(LiveReader::new(data))
}

fn open_installed(data: &Arc<Mutex<Vec<u8>>>) -> crate::FatReader {
    crate::FatReader::open(shared_reader(Arc::clone(data)), 0).unwrap()
}

fn install(data: &Arc<Mutex<Vec<u8>>>) -> crate::EspFallbackInstall {
    let io = MemIo(Arc::clone(data));
    crate::install_efi_fallback(
        shared_reader(Arc::clone(data)),
        0,
        &io,
        &[
            ("BOOTX64.EFI".to_string(), b"shim-bytes".to_vec()),
            ("GRUBX64.EFI".to_string(), b"grub-copy".to_vec()),
        ],
    )
    .unwrap()
}

#[test]
fn install_creates_boot_directory_and_files() {
    let data = Arc::new(Mutex::new(build_esp_fixture()));
    let result = install(&data);
    assert!(result.created_boot_directory);
    assert_eq!(result.files_written, ["BOOTX64.EFI", "GRUBX64.EFI"]);
    assert!(result.files_skipped.is_empty());

    let fs = open_installed(&data);
    let content = fs.read_file_range("EFI/BOOT/BOOTX64.EFI", 0, 64).unwrap();
    assert_eq!(content, b"shim-bytes");
    let content = fs.read_file_range("EFI/BOOT/GRUBX64.EFI", 0, 64).unwrap();
    assert_eq!(content, b"grub-copy");
    let boot_children = fs.list_children("EFI/BOOT").unwrap();
    assert!(boot_children.iter().any(|node| node.name == "BOOTX64.EFI"));

    // Both FAT copies carry the new chains; FSInfo free count dropped by 3
    // (BOOT dir + one cluster per file).
    let raw = data.lock().unwrap();
    for fat in 0..FAT_COUNT {
        assert_eq!(
            u32::from_le_bytes(
                raw[fat_pos(fat, 6)..fat_pos(fat, 6) + 4]
                    .try_into()
                    .unwrap()
            ),
            0x0FFF_FFFF
        );
    }
    assert_eq!(
        u32::from_le_bytes(raw[BPS + 488..BPS + 492].try_into().unwrap()),
        187
    );
}

#[test]
fn install_skips_existing_files() {
    let data = Arc::new(Mutex::new(build_esp_fixture()));
    install(&data);
    let second = install(&data);
    assert!(!second.created_boot_directory);
    assert!(second.files_written.is_empty());
    assert_eq!(second.files_skipped, ["BOOTX64.EFI", "GRUBX64.EFI"]);
    // The directory still holds exactly one entry per file.
    let fs = open_installed(&data);
    let names: Vec<String> = fs
        .list_children("EFI/BOOT")
        .unwrap()
        .into_iter()
        .map(|node| node.name)
        .collect();
    assert_eq!(
        names.iter().filter(|name| *name == "BOOTX64.EFI").count(),
        1
    );
}

#[test]
fn rejects_non_fat32_volumes() {
    let mut fixture = build_esp_fixture();
    fixture[66] = 0; // small cluster count without the FAT32 signature
    let data = Arc::new(Mutex::new(fixture));
    let io = MemIo(Arc::clone(&data));
    let error = crate::install_efi_fallback(
        shared_reader(Arc::clone(&data)),
        0,
        &io,
        &[("BOOTX64.EFI".to_string(), b"shim".to_vec())],
    )
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
}

#[test]
fn rejects_lowercase_names() {
    let data = Arc::new(Mutex::new(build_esp_fixture()));
    let io = MemIo(Arc::clone(&data));
    let error = crate::install_efi_fallback(
        shared_reader(Arc::clone(&data)),
        0,
        &io,
        &[("bootx64.efi".to_string(), b"shim".to_vec())],
    )
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn extends_a_full_directory_chain() {
    let mut fixture = build_esp_fixture();
    // Fill the EFI directory cluster (16 slots) so BOOT needs a new cluster.
    let efi = cluster_pos(3);
    for slot in 3..16 {
        let name = format!("FILL{slot:02}");
        put_dir_entry(&mut fixture[efi..efi + BPS], slot, &name, "", 0x20, 5, 1);
    }
    let data = Arc::new(Mutex::new(fixture));
    let result = install(&data);
    assert!(result.created_boot_directory);
    let fs = open_installed(&data);
    assert_eq!(
        fs.read_file_range("EFI/BOOT/BOOTX64.EFI", 0, 64).unwrap(),
        b"shim-bytes"
    );
    let names: Vec<String> = fs
        .list_children("EFI")
        .unwrap()
        .into_iter()
        .map(|node| node.name)
        .collect();
    assert!(names.iter().any(|name| name == "BOOT"));
    assert!(names.iter().any(|name| name == "FILL15"));
}
