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
pub use crate::synthetic::{build_synthetic_pst, build_synthetic_pst_with_messages};

#[cfg(test)]
mod tests {
    use crate::header::{NID_MESSAGE_STORE, NID_ROOT_FOLDER, PST_MAGIC};
    use crate::props::read_u16_le;
    use crate::pst::PstReader;
    use crate::synthetic::build_synthetic_unicode_pst;

    #[test]
    fn synthetic_pst_header_magic() {
        let pst = build_synthetic_unicode_pst();
        assert_eq!(&pst[0..4], &PST_MAGIC);
        let ver = read_u16_le(&pst, 10).unwrap();
        assert_eq!(ver, 23);
    }

    #[test]
    fn open_synthetic_pst() {
        let pst = build_synthetic_unicode_pst();

        // Write to temp file, then open.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();
        assert!(reader.is_unicode());
        assert!(reader.file_size() > 0);
    }

    #[test]
    fn synthetic_pst_nbt_entries() {
        let pst = build_synthetic_unicode_pst();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();

        // The message store NID should be in the NBT cache.
        assert!(reader.nbt_cache.contains_key(&NID_MESSAGE_STORE));
        assert!(reader.nbt_cache.contains_key(&NID_ROOT_FOLDER));
    }

    #[test]
    fn fold_and_read_nbt_structure() {
        let pst = build_synthetic_unicode_pst();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();

        // Verify the NBT root bid from the header.
        let nbt_bid = reader.header.root_nbt.bid;
        assert_eq!(nbt_bid, 4);

        // Verify the BBT root bid.
        let bbt_bid = reader.header.root_bbt.bid;
        assert_eq!(bbt_bid, 2);
    }

    #[test]
    fn synthetic_pst_property_context() {
        let pst = build_synthetic_unicode_pst();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();

        // Read the property context for the message store.
        let block = reader.read_subnode_block(NID_MESSAGE_STORE);
        assert!(block.is_some());

        let props = reader.parse_property_context(block.unwrap());
        assert!(!props.is_empty(), "Property context should have entries");
    }

    #[test]
    fn synthetic_pst_read_messages() {
        let pst = build_synthetic_unicode_pst();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &pst).unwrap();

        let reader = PstReader::open(&path).unwrap();
        let messages = reader.read_messages().unwrap();
        assert_eq!(
            messages.len(),
            1,
            "expected one message, got {:?}",
            messages
        );
        let msg = &messages[0];
        assert_eq!(msg.subject, "Synthetic Subject 1");
        assert_eq!(msg.sender_name, "Sender 1");
        assert_eq!(msg.sender_email, "sender1@example.com");
        assert!(msg
            .body_plain
            .contains("Body text for synthetic message number 1."));
    }

    #[test]
    fn invalid_pst_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.pst");
        std::fs::write(&path, b"not a pst file").unwrap();

        let result = PstReader::open(&path);
        assert!(result.is_err());
    }

    #[test]
    fn empty_file_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.pst");
        std::fs::write(&path, []).unwrap();

        let result = PstReader::open(&path);
        assert!(result.is_err());
    }
}
