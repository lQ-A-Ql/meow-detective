use super::*;

/// A minimal fake reader for testing LvReader.
struct FakeDevice {
    data: Vec<u8>,
    pos: u64,
    info: ReaderInfo,
    preferred_read_granularity: usize,
}

impl FakeDevice {
    fn new(data: Vec<u8>) -> Self {
        Self::with_granularity(data, 0)
    }

    fn with_granularity(data: Vec<u8>, preferred_read_granularity: usize) -> Self {
        Self {
            info: ReaderInfo {
                path: std::path::PathBuf::from("fake-lvm-device"),
                size: data.len() as u64,
                kind: "fake-lvm-device".to_string(),
            },
            data,
            pos: 0,
            preferred_read_granularity,
        }
    }
}

impl Read for FakeDevice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let start = self.pos as usize;
        let end = (start + buf.len()).min(self.data.len());
        let len = end.saturating_sub(start);
        buf[..len].copy_from_slice(&self.data[start..end]);
        self.pos += len as u64;
        Ok(len)
    }
}

impl Seek for FakeDevice {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(o) => o,
            SeekFrom::End(o) => (self.data.len() as i64 + o).max(0) as u64,
            SeekFrom::Current(o) => (self.pos as i64 + o).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl EvidenceReader for FakeDevice {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        self.preferred_read_granularity
    }
}

#[test]
fn read_within_single_extent() {
    // PV data: 4 KB of zeros, then "HELLO WORLD DATA" at offset 4096
    let mut pv_data = vec![0u8; 4096];
    pv_data.extend_from_slice(b"HELLO WORLD DATA");

    let device = Box::new(FakeDevice::new(pv_data));
    let extent_map = vec![LvExtent {
        logical_start: 0,
        physical_offset: 4096,
        length: 17,
        pv_index: 0,
    }];

    let lv = LvReader::new(device, "test_lv".into(), 17, extent_map);
    let reader = std::sync::Mutex::new(Box::new(lv) as Box<dyn EvidenceReader>);
    let mut lv_ref = reader.lock().unwrap();

    let mut buf = [0u8; 5];
    lv_ref.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"HELLO");
}

#[test]
fn read_across_extents_uses_extent_pv_index() {
    let mut pv0_data = vec![0u8; 32];
    pv0_data[8..12].copy_from_slice(b"PV00");
    let mut pv1_data = vec![0u8; 32];
    pv1_data[8..12].copy_from_slice(b"PV11");

    let pv0: Box<dyn EvidenceReader> = Box::new(FakeDevice::new(pv0_data));
    let pv1: Box<dyn EvidenceReader> = Box::new(FakeDevice::new(pv1_data));
    let device_readers = vec![
        std::sync::Arc::new(std::sync::Mutex::new(pv0)),
        std::sync::Arc::new(std::sync::Mutex::new(pv1)),
    ];
    let extent_map = vec![
        LvExtent {
            logical_start: 0,
            physical_offset: 8,
            length: 4,
            pv_index: 0,
        },
        LvExtent {
            logical_start: 4,
            physical_offset: 8,
            length: 4,
            pv_index: 1,
        },
    ];

    let mut lv = LvReader::new_shared(device_readers, "striped_lv".into(), 8, extent_map);

    let mut buf = [0u8; 8];
    lv.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"PV00PV11");
}

#[test]
fn plain_read_fills_across_extent_boundaries() {
    let mut pv0_data = vec![0u8; 32];
    pv0_data[8..12].copy_from_slice(b"PV00");
    let mut pv1_data = vec![0u8; 32];
    pv1_data[8..12].copy_from_slice(b"PV11");

    let device_readers = vec![
        std::sync::Arc::new(std::sync::Mutex::new(
            Box::new(FakeDevice::new(pv0_data)) as Box<dyn EvidenceReader>
        )),
        std::sync::Arc::new(std::sync::Mutex::new(
            Box::new(FakeDevice::new(pv1_data)) as Box<dyn EvidenceReader>
        )),
    ];
    let extent_map = vec![
        LvExtent {
            logical_start: 0,
            physical_offset: 8,
            length: 4,
            pv_index: 0,
        },
        LvExtent {
            logical_start: 4,
            physical_offset: 8,
            length: 4,
            pv_index: 1,
        },
    ];

    let mut lv = LvReader::new_shared(device_readers, "striped_lv".into(), 8, extent_map);
    let mut buf = [0u8; 8];
    let n = lv.read(&mut buf).unwrap();

    assert_eq!(n, 8);
    assert_eq!(&buf, b"PV00PV11");
}

#[test]
fn seek_and_read() {
    let mut pv_data = vec![0u8; 2048];
    pv_data.extend_from_slice(b"ABCDEFGHIJ");

    let device = Box::new(FakeDevice::new(pv_data));
    let extent_map = vec![LvExtent {
        logical_start: 0,
        physical_offset: 2048,
        length: 10,
        pv_index: 0,
    }];

    let lv = LvReader::new(device, "test_lv".into(), 10, extent_map);
    let reader = std::sync::Mutex::new(Box::new(lv) as Box<dyn EvidenceReader>);
    let mut lv_ref = reader.lock().unwrap();

    // Seek to offset 3, read 2 bytes
    lv_ref.seek(SeekFrom::Start(3)).unwrap();
    let mut buf = [0u8; 2];
    lv_ref.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"DE");
}

#[test]
fn read_past_end_returns_zero() {
    let mut pv_data = vec![0u8; 1024];
    pv_data.extend_from_slice(b"DATA");

    let device = Box::new(FakeDevice::new(pv_data));
    let extent_map = vec![LvExtent {
        logical_start: 0,
        physical_offset: 1024,
        length: 4,
        pv_index: 0,
    }];

    let lv = LvReader::new(device, "small_lv".into(), 4, extent_map);
    let reader = std::sync::Mutex::new(Box::new(lv) as Box<dyn EvidenceReader>);
    let mut lv_ref = reader.lock().unwrap();

    lv_ref.seek(SeekFrom::Start(4)).unwrap();
    let mut buf = [0u8; 10];
    let n = lv_ref.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn preferred_read_granularity_propagates_from_underlying_devices() {
    let first: Box<dyn EvidenceReader> =
        Box::new(FakeDevice::with_granularity(vec![0; 16], 4 * 1024));
    let second: Box<dyn EvidenceReader> =
        Box::new(FakeDevice::with_granularity(vec![0; 16], 64 * 1024));
    let readers = vec![
        std::sync::Arc::new(std::sync::Mutex::new(first)),
        std::sync::Arc::new(std::sync::Mutex::new(second)),
    ];

    let lv = LvReader::new_shared(readers, "granularity".to_string(), 0, Vec::new());

    assert_eq!(lv.preferred_read_granularity(), 64 * 1024);
}
