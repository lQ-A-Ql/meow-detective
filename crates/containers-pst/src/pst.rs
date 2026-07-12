//! PST (Personal Storage Table) format support.
//!
//! PST is the file format used by Microsoft Outlook to store email messages,
//! calendar entries, contacts, and other items locally.
//!
//! ## Format overview
//!
//! The PST format consists of:
//! - **NDB (Node Database)**: Fixed-size pages (512 bytes). The first page is
//!   the header, which contains pointers to the Node BTree (NBT) and Block
//!   BTree (BBT) roots.
//! - **NBT (Node BTree)**: Maps Node IDs (NIDs) to data block locations.
//!   Internal nodes contain `(nid, bidData, bidSub)` entries; leaf nodes
//!   omit `bidSub`.
//! - **BBT (Block BTree)**: Maps Block IDs (BIDs) to file byte offsets.
//! - **LTP (List, Table, Properties)**: Heap-on-Node and BTree-on-Heap
//!   structures for property storage.
//!
//! ## Supported formats
//!
//! - Unicode PST (64-bit, wVer=23)
//! - ANSI PST (32-bit, wVer=14/15)
//!
//! ## Known property IDs
//!
//! | ID     | Name                     |
//! |--------|--------------------------|
//! | 0x0037 | PidTagSubject            |
//! | 0x1000 | PidTagBody               |
//! | 0x0C1A | PidTagSenderName         |
//! | 0x0E1F | PidTagSenderEmailAddress |
//! | 0x0039 | PidTagClientSubmitTime   |
//! | 0x0E06 | PidTagMessageDeliveryTime|
//! | 0x0E04 | PidTagDisplayTo          |
//! | 0x0E03 | PidTagDisplayCc          |
//! | 0x0E17 | PidTagMessageClass       |
//! | 0x3701 | PidTagAttachmentData     |

pub use crate::reader::PstReader;

#[cfg(test)]
#[path = "../tests/unit/pst.rs"]
mod tests;
