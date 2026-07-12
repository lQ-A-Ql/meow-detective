use std::io;

pub const JBD2_MAGIC: u32 = 0xC03B_399B;
pub const JBD2_DESCRIPTOR_MAGIC: u32 = 0xC03B_3998;
pub const JBD2_COMMIT_MAGIC: u32 = 0xC03B_3999;
pub const JBD2_REVOKE_MAGIC: u32 = 0xC03B_399A;
pub const JOURNAL_INODE: u32 = 8;
pub const JOURNAL_HEADER_SIZE: usize = 12;
pub const JOURNAL_SB_OFFSET: u64 = 4096;
pub const JBD2_TAG_SIZE_V2: usize = 16;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecoveredFile {
    pub original_path: String,
    pub inode: u32,
    pub blocks: Vec<Vec<u8>>,
    pub declared_size: u64,
    pub recovery_method: String,
    pub confidence: f64,
    pub block_count: u64,
}

#[derive(Debug, Clone)]
pub struct JournalSuperblock {
    pub magic: u32,
    pub block_type: u32,
    pub sequence: u32,
    pub blocksize: u32,
    pub maxlen: u32,
    pub first: u32,
    pub sequence_num: u32,
    pub start: u32,
}

#[derive(Debug, Clone)]
pub struct JournalHeader {
    pub magic: u32,
    pub block_type: u32,
    pub sequence: u32,
}

impl JournalHeader {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < JOURNAL_HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "journal header too short",
            ));
        }
        Ok(Self {
            magic: u32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            block_type: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            sequence: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
        })
    }

    pub fn is_descriptor(&self) -> bool {
        self.magic == JBD2_DESCRIPTOR_MAGIC
    }

    pub fn is_commit(&self) -> bool {
        self.magic == JBD2_COMMIT_MAGIC
    }

    pub fn is_revoke(&self) -> bool {
        self.magic == JBD2_REVOKE_MAGIC
    }
}

impl JournalSuperblock {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "journal superblock too short",
            ));
        }
        let header = JournalHeader::parse(&data[..JOURNAL_HEADER_SIZE])?;
        if header.magic != JBD2_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid journal superblock magic 0x{:08X}, expected 0x{:08X}",
                    header.magic, JBD2_MAGIC
                ),
            ));
        }
        Ok(Self {
            magic: header.magic,
            block_type: header.block_type,
            sequence: header.sequence,
            blocksize: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            maxlen: u32::from_be_bytes([data[20], data[21], data[22], data[23]]),
            first: u32::from_be_bytes([data[24], data[25], data[26], data[27]]),
            sequence_num: u32::from_be_bytes([data[28], data[29], data[30], data[31]]),
            start: u32::from_be_bytes([data[32], data[33], data[34], data[35]]),
        })
    }
}

#[derive(Debug, Clone)]
pub struct BlockTag {
    pub block_number: u32,
    pub flags: u32,
}

#[derive(Debug, Clone)]
pub struct DescriptorBlock {
    pub header: JournalHeader,
    pub tags: Vec<BlockTag>,
    pub block_data: Vec<Vec<u8>>,
}
