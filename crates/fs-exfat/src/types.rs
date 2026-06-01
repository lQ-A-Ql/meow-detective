//! exFAT constants and type definitions.
//!
//! Based on the exFAT specification from Microsoft:
//! https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification

/// Boot sector magic: "EXFAT   " (8 bytes with trailing spaces)
pub const EXFAT_MAGIC: &[u8; 8] = b"EXFAT   ";

/// Boot sector jump instruction: EBh 76h 90h
pub const JUMP_BOOT: [u8; 3] = [0xEB, 0x76, 0x90];

/// Boot sector signature: AA55h
pub const BOOT_SIGNATURE: u16 = 0xAA55;

/// Extended boot signature: AA550000h
pub const EXTENDED_BOOT_SIGNATURE: u32 = 0xAA55_0000;

// FAT entry values
/// End of chain marker (>= 0xFFFF_FFF8)
pub const END_OF_CHAIN: u32 = 0xFFFF_FFFF;
/// Bad cluster marker
pub const BAD_CLUSTER: u32 = 0xFFFF_FFF7;
/// Free cluster
pub const FREE_CLUSTER: u32 = 0x0000_0000;
/// Minimum valid cluster index
pub const MIN_CLUSTER: u32 = 2;

// Directory entry types (TypeCode field, bits 0-6)
/// File directory entry
pub const ENTRY_TYPE_FILE: u8 = 0x05;
/// Stream Extension directory entry
pub const ENTRY_TYPE_STREAM: u8 = 0x00;
/// File Name directory entry
pub const ENTRY_TYPE_FILENAME: u8 = 0x01;
/// Allocation Bitmap directory entry
pub const ENTRY_TYPE_BITMAP: u8 = 0x01;
/// Up-case Table directory entry
pub const ENTRY_TYPE_UPCASE: u8 = 0x02;
/// Volume Label directory entry
pub const ENTRY_TYPE_LABEL: u8 = 0x03;
/// Vendor Extension directory entry
pub const ENTRY_TYPE_VENDOR_EXT: u8 = 0x04;
/// Vendor Allocation directory entry
pub const ENTRY_TYPE_VENDOR_ALLOC: u8 = 0x05;

// Directory entry flags
/// In-use flag (bit 7 of EntryType)
pub const ENTRY_IN_USE: u8 = 0x80;
/// NoFatChain flag (bit 1 of GeneralSecondaryFlags)
pub const NO_FAT_CHAIN: u8 = 0x02;

// File attributes
pub const ATTR_READ_ONLY: u16 = 0x0001;
pub const ATTR_HIDDEN: u16 = 0x0002;
pub const ATTR_SYSTEM: u16 = 0x0004;
pub const ATTR_DIRECTORY: u16 = 0x0010;
pub const ATTR_ARCHIVE: u16 = 0x0020;

/// Size of a directory entry in bytes
pub const DIR_ENTRY_SIZE: usize = 32;

/// Maximum number of File Name entries per file (NameLength / 15, rounded up)
pub const MAX_FILENAME_ENTRIES: usize = 17; // 255 / 15 = 17

/// Characters per File Name entry
pub const CHARS_PER_FILENAME_ENTRY: usize = 15;
