pub struct Hive {
    pub bytes: Vec<u8>,
    next: usize,
}
impl Hive {
    pub fn new() -> Self {
        let mut bytes = vec![0; 4096 + 262144];
        bytes[..4].copy_from_slice(b"regf");
        for (at, value) in [
            (4, 1u32),
            (8, 1),
            (0x14, 1),
            (0x18, 5),
            (0x20, 1),
            (0x28, 262144),
        ] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[4096..4100].copy_from_slice(b"hbin");
        bytes[4104..4108].copy_from_slice(&262144u32.to_le_bytes());
        Self { bytes, next: 4128 }
    }
    pub fn cell(&mut self, data: &[u8]) -> u32 {
        let at = self.next;
        let length = (data.len() + 4).div_ceil(8) * 8;
        self.bytes[at..at + 4].copy_from_slice(&(-(length as i32)).to_le_bytes());
        self.bytes[at + 4..at + 4 + data.len()].copy_from_slice(data);
        self.next += length;
        (at - 4096) as u32
    }
    pub fn key(&mut self, name: &str, children: &[u32], values: &[u32]) -> u32 {
        let mut child_list = b"lh".to_vec();
        child_list.extend_from_slice(&(children.len() as u16).to_le_bytes());
        for child in children {
            child_list.extend_from_slice(&child.to_le_bytes());
            child_list.extend_from_slice(&0xfffffff0u32.to_le_bytes());
        }
        let child_offset = self.cell(&child_list);
        let value_list: Vec<_> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let value_offset = self.cell(&value_list);
        let mut data = vec![0; 76];
        data[..2].copy_from_slice(b"nk");
        data[2..4].copy_from_slice(&0x20u16.to_le_bytes());
        data[4..12].copy_from_slice(&133000000000000001u64.to_le_bytes());
        for (at, val) in [
            (20, children.len() as u32),
            (28, child_offset),
            (36, values.len() as u32),
            (40, value_offset),
        ] {
            data[at..at + 4].copy_from_slice(&val.to_le_bytes());
        }
        data[72..74].copy_from_slice(&(name.len() as u16).to_le_bytes());
        data.extend_from_slice(name.as_bytes());
        let parent = self.cell(&data);
        for child in children {
            let at = 4096 + *child as usize + 4 + 16;
            if let Some(field) = self.bytes.get_mut(at..at + 4) {
                field.copy_from_slice(&parent.to_le_bytes());
            }
        }
        parent
    }
    pub fn value(&mut self, name: &str, kind: u32, raw: &[u8], inline: bool) -> u32 {
        let data_offset = if inline {
            let mut b = [0; 4];
            b[..raw.len()].copy_from_slice(raw);
            u32::from_le_bytes(b)
        } else if raw.len() > 16344 {
            let offsets: Vec<_> = raw.chunks(16344).map(|part| self.cell(part)).collect();
            let list: Vec<_> = offsets.iter().flat_map(|v| v.to_le_bytes()).collect();
            let offset = self.cell(&list);
            let mut db = b"db".to_vec();
            db.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
            db.extend_from_slice(&offset.to_le_bytes());
            self.cell(&db)
        } else {
            self.cell(raw)
        };
        let mut data = vec![0; 20];
        data[..2].copy_from_slice(b"vk");
        data[2..4].copy_from_slice(&(name.len() as u16).to_le_bytes());
        let length = raw.len() as u32 | if inline { 0x80000000 } else { 0 };
        for (at, val) in [(4, length), (8, data_offset), (12, kind)] {
            data[at..at + 4].copy_from_slice(&val.to_le_bytes());
        }
        data[16..18].copy_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(name.as_bytes());
        self.cell(&data)
    }
    pub fn finish(mut self, root: u32) -> Vec<u8> {
        self.bytes[0x24..0x28].copy_from_slice(&root.to_le_bytes());
        let length = self.bytes.len() - self.next;
        self.bytes[self.next..self.next + 4].copy_from_slice(&(length as i32).to_le_bytes());
        self.bytes
    }
}
