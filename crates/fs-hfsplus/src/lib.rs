//! HFS+ (Mac OS Extended) filesystem reader.
//!
//! Implements the `FileSystemReader` trait for HFS Plus volumes.  Parses the
//! volume header at offset 1024 (magic `H+` or `HX`), the catalog B-tree for
//! directory traversal, and extent descriptors for file content.
//!
//! Supported features:
//! - Volume header parsing (block size, total/free blocks, timestamps).
//! - Catalog B-tree traversal: header → index → leaf nodes.
//! - Folder records (folder listing via `parentCNID` lookups).
//! - File records with data-fork extent descriptors.
//! - Hard-link detection via the indirect-node file and BSD `special` field.
//! - Symlink detection via BSD file-mode `S_IFLNK` or Finder type `slnk`.
//! - HFS+ timestamps (seconds since 1904-01-01 UTC).
//!
//! Many on-disk constants and optional fields are declared for completeness
//! even when not yet exercised by the current reader code path.

mod constants;
mod parser;
mod reader;

pub use reader::HfsPlusReader;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;
    use evidence_core::filesystem::FileSystemReader;
    use evidence_core::EvidenceReader;
    use evidence_core::ReaderInfo;
    use std::io::{self, Read, Seek, SeekFrom};

    // -------------------------------------------------------------------
    // Fake evidence reader
    // -------------------------------------------------------------------

    struct FakeReader {
        data: Vec<u8>,
        pos: u64,
    }

    impl FakeReader {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }
    }

    impl Read for FakeReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let start = (self.pos as usize).min(self.data.len());
            let end = (start + buf.len()).min(self.data.len());
            let n = end - start;
            buf[..n].copy_from_slice(&self.data[start..end]);
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for FakeReader {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.pos = match pos {
                SeekFrom::Start(p) => p,
                SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
                SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
            };
            Ok(self.pos)
        }
    }

    impl EvidenceReader for FakeReader {
        fn info(&self) -> &ReaderInfo {
            unimplemented!()
        }
    }

    fn build_hfsplus_fixture_v2() -> Vec<u8> {
        use crate::constants::*;

        let block_size: usize = 4096;
        let total_blocks: usize = 10;
        let total_size = total_blocks * block_size;
        let mut img = vec![0u8; total_size];

        let block = |n: usize| -> usize { n * block_size };

        // HFS+ timestamps (seconds since 1904-01-01).
        let ts_create: u32 = 3660681600u32; // 2020-01-01
        let ts_modify: u32 = ts_create + 86400;
        let ts_access: u32 = ts_create + 172800;

        // ===================================================================
        // Block 0: Volume header at byte offset 1024 (within block 0).
        // ===================================================================
        let vh_off = VOLUME_HEADER_OFFSET as usize;
        let vh = &mut img[vh_off..vh_off + VOLUME_HEADER_SIZE];

        vh[VH_SIGNATURE..VH_SIGNATURE + 2].copy_from_slice(&HFSPLUS_SIGNATURE.to_be_bytes());
        vh[VH_VERSION..VH_VERSION + 2].copy_from_slice(&4u16.to_be_bytes());
        vh[VH_BLOCK_SIZE..VH_BLOCK_SIZE + 4].copy_from_slice(&(block_size as u32).to_be_bytes());
        vh[VH_TOTAL_BLOCKS..VH_TOTAL_BLOCKS + 4]
            .copy_from_slice(&(total_blocks as u32).to_be_bytes());
        vh[VH_FREE_BLOCKS..VH_FREE_BLOCKS + 4].copy_from_slice(&1u32.to_be_bytes());
        vh[VH_NEXT_CATALOG_ID..VH_NEXT_CATALOG_ID + 4].copy_from_slice(&100u32.to_be_bytes());

        // Catalog file fork: logicalSize = 5*4096 = 20480, totalBlocks=5,
        // extent[0]=(1,4), extent[1]=(7,1)
        let cf = VH_CATALOG_FILE;
        vh[cf + FORK_LOGICAL_SIZE..cf + FORK_LOGICAL_SIZE + 8]
            .copy_from_slice(&(5u64 * block_size as u64).to_be_bytes());
        vh[cf + FORK_TOTAL_BLOCKS..cf + FORK_TOTAL_BLOCKS + 4].copy_from_slice(&5u32.to_be_bytes());
        let ext0 = cf + FORK_EXTENTS;
        vh[ext0..ext0 + 4].copy_from_slice(&1u32.to_be_bytes()); // startBlock
        vh[ext0 + 4..ext0 + 8].copy_from_slice(&4u32.to_be_bytes()); // blockCount
        let ext1 = ext0 + EXTENT_DESC_SIZE;
        vh[ext1..ext1 + 4].copy_from_slice(&7u32.to_be_bytes()); // startBlock
        vh[ext1 + 4..ext1 + 8].copy_from_slice(&1u32.to_be_bytes()); // blockCount

        // ===================================================================
        // Block 1: Catalog B-tree header node (node 0, kind=0x02)
        // ===================================================================
        let hn = &mut img[block(1)..block(2)];

        hn[BT_F_LINK..BT_F_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        hn[BT_B_LINK..BT_B_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        hn[BT_KIND] = BT_HEADER_NODE;
        hn[BT_HEIGHT] = 0;
        hn[BT_NUM_RECORDS..BT_NUM_RECORDS + 2].copy_from_slice(&3u16.to_be_bytes());
        hn[BT_RESERVED..BT_RESERVED + 2].copy_from_slice(&0u16.to_be_bytes());

        // Record offsets: 3 records.
        let rec_off_start = BT_NODE_DESC_SIZE;
        let hdr_rec_off: u16 = 0x0040; // header record
        let user_rec_off: u16 = 0x0120; // user data (dummy, 128 bytes)
        let map_rec_off: u16 = 0x01A0; // map record (dummy, 256 bytes)
        hn[rec_off_start..rec_off_start + 2].copy_from_slice(&hdr_rec_off.to_be_bytes());
        hn[rec_off_start + 2..rec_off_start + 4].copy_from_slice(&user_rec_off.to_be_bytes());
        hn[rec_off_start + 4..rec_off_start + 6].copy_from_slice(&map_rec_off.to_be_bytes());

        // Header record at offset 0x40.
        let hdr = &mut hn[hdr_rec_off as usize..];
        // Key: keyLength(2)=8 + parentCNID(4)=0 + nameLen(2)=0
        hdr[0..2].copy_from_slice(&0x0008u16.to_be_bytes());
        hdr[2..6].copy_from_slice(&0u32.to_be_bytes());
        hdr[6..8].copy_from_slice(&0u16.to_be_bytes());
        // B-Tree header data at offset 8:
        let hd = &mut hdr[8..];
        hd[BT_HEADER_TREE_DEPTH..BT_HEADER_TREE_DEPTH + 2].copy_from_slice(&2u16.to_be_bytes()); // depth=2 (header→index→leaf)
        hd[BT_HEADER_ROOT_NODE..BT_HEADER_ROOT_NODE + 4].copy_from_slice(&2u32.to_be_bytes()); // root = node 2 (the index node)
        hd[BT_HEADER_LEAF_RECORDS..BT_HEADER_LEAF_RECORDS + 4].copy_from_slice(&8u32.to_be_bytes());
        hd[BT_HEADER_FIRST_LEAF..BT_HEADER_FIRST_LEAF + 4].copy_from_slice(&3u32.to_be_bytes()); // first leaf = node 3
        hd[BT_HEADER_LAST_LEAF..BT_HEADER_LAST_LEAF + 4].copy_from_slice(&5u32.to_be_bytes()); // last leaf = node 5 (subdir)
        hd[BT_HEADER_NODE_SIZE..BT_HEADER_NODE_SIZE + 2]
            .copy_from_slice(&(block_size as u16).to_be_bytes());
        hd[BT_HEADER_MAX_KEY_LEN..BT_HEADER_MAX_KEY_LEN + 2].copy_from_slice(&512u16.to_be_bytes());
        hd[BT_HEADER_TOTAL_NODES..BT_HEADER_TOTAL_NODES + 4].copy_from_slice(&5u32.to_be_bytes()); // nodes 0,1,2,3 → 4 total
        hd[BT_HEADER_FREE_LIST..BT_HEADER_FREE_LIST + 4].copy_from_slice(&0u32.to_be_bytes());

        // Fill in dummy user data record and map record (minimal).
        for off in [user_rec_off as usize, map_rec_off as usize] {
            hn[off..off + 2].copy_from_slice(&0u16.to_be_bytes());
            hn[off + 2..off + 6].copy_from_slice(&0u32.to_be_bytes());
            hn[off + 6..off + 8].copy_from_slice(&0u16.to_be_bytes());
        }

        // ===================================================================
        // Block 2: Catalog B-tree index node (node 1, kind=0x01)
        // ===================================================================
        // Node numbers:
        //   0: header node (block 1)
        //   1: index node (block 2) → root node
        //   2: leaf node for parentCNID=2 (block 3)
        //   3: leaf node for parentCNID=32 (block 4)
        //
        // Wait, I should be consistent. In the header I said:
        //   rootNode = 2 (which means block index 2? or node number 2?)
        //
        // The catalog B-tree occupies blocks 1-4. Node numbers start at 0.
        // Block 1 = node 0 (header), Block 2 = node 1 (index/root),
        // Block 3 = node 2 (leaf), Block 4 = node 3 (leaf).
        //
        // But the node_size = 4096, and each node takes one block. So node N
        // is at offset: extent_start_block * block_size + N * node_size.
        //
        // For our case: node 0 at block 1, node 1 at block 2, node 2 at block 3,
        // node 3 at block 4. So rootNode = 1.
        //
        // Let me correct the header node: rootNode should be 1.

        // Fix rootNode in the header node:
        let hdr_off_fix = hdr_rec_off as usize + 8;
        hn[hdr_off_fix + BT_HEADER_ROOT_NODE..hdr_off_fix + BT_HEADER_ROOT_NODE + 4]
            .copy_from_slice(&1u32.to_be_bytes()); // rootNode = 1 (the index node)
        hn[hdr_off_fix + BT_HEADER_FIRST_LEAF..hdr_off_fix + BT_HEADER_FIRST_LEAF + 4]
            .copy_from_slice(&2u32.to_be_bytes()); // firstLeaf = 2
        hn[hdr_off_fix + BT_HEADER_LAST_LEAF..hdr_off_fix + BT_HEADER_LAST_LEAF + 4]
            .copy_from_slice(&4u32.to_be_bytes()); // lastLeaf = 4

        // ===================================================================
        // Block 2: Index node (node 1)
        // ===================================================================
        let idx = &mut img[block(2)..block(3)];

        idx[BT_F_LINK..BT_F_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        idx[BT_B_LINK..BT_B_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        idx[BT_KIND] = BT_INDEX_NODE;
        idx[BT_HEIGHT] = 1; // height above leaf level
        idx[BT_NUM_RECORDS..BT_NUM_RECORDS + 2].copy_from_slice(&3u16.to_be_bytes());
        idx[BT_RESERVED..BT_RESERVED + 2].copy_from_slice(&0u16.to_be_bytes());

        // Index record 0: parentCNID=2 (key only, no name for first separator).
        // Key = keyLength(2)+parentCNID(4)+nameLen(2)=8 bytes.
        // Value = childNode (u32, 4 bytes).
        let idx_rec0_off: u16 = 0x0100;
        let idx_rec0 = &mut idx[idx_rec0_off as usize..];
        idx_rec0[0..2].copy_from_slice(&0x0008u16.to_be_bytes()); // keyLength=8
        idx_rec0[2..6].copy_from_slice(&2u32.to_be_bytes()); // parentCNID=2
        idx_rec0[6..8].copy_from_slice(&0u16.to_be_bytes()); // nameLen=0
        idx_rec0[8..12].copy_from_slice(&2u32.to_be_bytes()); // childNode=2

        // Index record 1: parentCNID=32.
        let idx_rec1_off: u16 = 0x0110;
        let idx_rec1 = &mut idx[idx_rec1_off as usize..];
        idx_rec1[0..2].copy_from_slice(&0x0008u16.to_be_bytes()); // keyLength=8
        idx_rec1[2..6].copy_from_slice(&32u32.to_be_bytes()); // parentCNID=32
        idx_rec1[6..8].copy_from_slice(&0u16.to_be_bytes()); // nameLen=0
        idx_rec1[8..12].copy_from_slice(&3u32.to_be_bytes()); // childNode=3

        // Index record 2: parentCNID=64.
        let idx_rec2_off: u16 = 0x0120;
        let idx_rec2 = &mut idx[idx_rec2_off as usize..];
        idx_rec2[0..2].copy_from_slice(&0x0008u16.to_be_bytes()); // keyLength=8
        idx_rec2[2..6].copy_from_slice(&64u32.to_be_bytes()); // parentCNID=64
        idx_rec2[6..8].copy_from_slice(&0u16.to_be_bytes()); // nameLen=0
        idx_rec2[8..12].copy_from_slice(&4u32.to_be_bytes()); // childNode=4

        // Record offset table.
        idx[rec_off_start..rec_off_start + 2].copy_from_slice(&idx_rec0_off.to_be_bytes());
        idx[rec_off_start + 2..rec_off_start + 4].copy_from_slice(&idx_rec1_off.to_be_bytes());
        idx[rec_off_start + 4..rec_off_start + 6].copy_from_slice(&idx_rec2_off.to_be_bytes());

        // ===================================================================
        // Block 3: Leaf node (node 2) — root directory entries (parentCNID=2)
        // ===================================================================
        let leaf = &mut img[block(3)..block(4)];

        leaf[BT_F_LINK..BT_F_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        leaf[BT_B_LINK..BT_B_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        leaf[BT_KIND] = BT_LEAF_NODE;
        leaf[BT_HEIGHT] = 0;
        leaf[BT_NUM_RECORDS..BT_NUM_RECORDS + 2].copy_from_slice(&3u16.to_be_bytes());
        leaf[BT_RESERVED..BT_RESERVED + 2].copy_from_slice(&0u16.to_be_bytes());

        // Helper: write a key (parentCNID + name) at a given cursor position.
        // Returns new cursor position.
        fn write_key(buf: &mut [u8], cursor: usize, parent_cnid: u32, name: &str) -> usize {
            let utf16: Vec<u16> = name.encode_utf16().collect();
            let char_count = utf16.len() as u16;
            let key_len = 2 + 4 + 2 + char_count as usize * 2;
            buf[cursor..cursor + 2].copy_from_slice(&(key_len as u16).to_be_bytes());
            buf[cursor + 2..cursor + 6].copy_from_slice(&parent_cnid.to_be_bytes());
            buf[cursor + 6..cursor + 8].copy_from_slice(&char_count.to_be_bytes());
            for (i, &cu) in utf16.iter().enumerate() {
                buf[cursor + 8 + i * 2..cursor + 10 + i * 2].copy_from_slice(&cu.to_be_bytes());
            }
            cursor + key_len
        }

        fn write_folder_body(
            buf: &mut [u8],
            cursor: usize,
            cnid: u32,
            create: u32,
            mod_: u32,
            access: u32,
        ) -> usize {
            let mut data = [0u8; FOLDER_RECORD_SIZE];
            data[FOLDER_RECORD_TYPE..FOLDER_RECORD_TYPE + 2]
                .copy_from_slice(&RECORD_TYPE_FOLDER.to_be_bytes());
            data[FOLDER_ID..FOLDER_ID + 4].copy_from_slice(&cnid.to_be_bytes());
            data[FOLDER_CREATE_DATE..FOLDER_CREATE_DATE + 4].copy_from_slice(&create.to_be_bytes());
            data[FOLDER_CONTENT_MOD_DATE..FOLDER_CONTENT_MOD_DATE + 4]
                .copy_from_slice(&mod_.to_be_bytes());
            data[FOLDER_ACCESS_DATE..FOLDER_ACCESS_DATE + 4].copy_from_slice(&access.to_be_bytes());
            data[FOLDER_PERMISSIONS + BSDINFO_FILE_MODE
                ..FOLDER_PERMISSIONS + BSDINFO_FILE_MODE + 2]
                .copy_from_slice(&0x41EDu16.to_be_bytes());
            buf[cursor..cursor + FOLDER_RECORD_SIZE].copy_from_slice(&data);
            cursor + FOLDER_RECORD_SIZE
        }

        #[allow(clippy::too_many_arguments)]
        fn write_file_body(
            buf: &mut [u8],
            cursor: usize,
            cnid: u32,
            create: u32,
            mod_: u32,
            access: u32,
            logical_size: u64,
            ext_start: u32,
            ext_count: u32,
        ) -> usize {
            let total_size = FILE_DATA_FORK + 80;
            let mut data = vec![0u8; total_size];
            data[FILE_RECORD_TYPE..FILE_RECORD_TYPE + 2]
                .copy_from_slice(&RECORD_TYPE_FILE.to_be_bytes());
            data[FILE_ID..FILE_ID + 4].copy_from_slice(&cnid.to_be_bytes());
            data[FILE_CREATE_DATE..FILE_CREATE_DATE + 4].copy_from_slice(&create.to_be_bytes());
            data[FILE_CONTENT_MOD_DATE..FILE_CONTENT_MOD_DATE + 4]
                .copy_from_slice(&mod_.to_be_bytes());
            data[FILE_ACCESS_DATE..FILE_ACCESS_DATE + 4].copy_from_slice(&access.to_be_bytes());
            data[FILE_PERMISSIONS + BSDINFO_FILE_MODE..FILE_PERMISSIONS + BSDINFO_FILE_MODE + 2]
                .copy_from_slice(&0x81A4u16.to_be_bytes()); // S_IFREG | 0644
            data[FILE_DATA_FORK + FORK_LOGICAL_SIZE..FILE_DATA_FORK + FORK_LOGICAL_SIZE + 8]
                .copy_from_slice(&logical_size.to_be_bytes());
            data[FILE_DATA_FORK + FORK_TOTAL_BLOCKS..FILE_DATA_FORK + FORK_TOTAL_BLOCKS + 4]
                .copy_from_slice(&ext_count.to_be_bytes());
            let ext_off = FILE_DATA_FORK + FORK_EXTENTS;
            data[ext_off..ext_off + 4].copy_from_slice(&ext_start.to_be_bytes());
            data[ext_off + 4..ext_off + 8].copy_from_slice(&ext_count.to_be_bytes());
            buf[cursor..cursor + total_size].copy_from_slice(&data);
            cursor + total_size
        }

        // Records in leaf node 2 (parentCNID=2):
        //   Record 0: Folder thread for CNID=2 (parentID=1, name="root")
        //   Record 1: "file.txt" file, CNID=16, data at block 5
        //   Record 2: "subdir" folder, CNID=32, children in node 3

        let mut cursor = 0x0100;
        let mut offsets: [u16; 3] = [0; 3];

        // Record 0: folder thread
        offsets[0] = cursor as u16;
        cursor = write_key(leaf, cursor, 2, ""); // parentCNID=2, empty name = thread
                                                 // Thread body: recordType(2)=0x0003, reserved(2), parentID(4)=1, nameLen(2)=4, name="root"
        let root_utf16: Vec<u16> = "root".encode_utf16().collect();
        let root_cu = root_utf16.len() as u16;
        let thread_size = 8 + 2 + root_cu as usize * 2;
        leaf[cursor..cursor + 2].copy_from_slice(&RECORD_TYPE_FOLDER_THREAD.to_be_bytes());
        leaf[cursor + 4..cursor + 8].copy_from_slice(&1u32.to_be_bytes()); // parentID=1
        leaf[cursor + 8..cursor + 10].copy_from_slice(&root_cu.to_be_bytes());
        for (i, &cu) in root_utf16.iter().enumerate() {
            leaf[cursor + 10 + i * 2..cursor + 12 + i * 2].copy_from_slice(&cu.to_be_bytes());
        }
        cursor += thread_size;

        // Record 1: "file.txt" file
        offsets[1] = cursor as u16;
        cursor = write_key(leaf, cursor, 2, "file.txt");
        let file_content = b"Hello from HFS+!";
        cursor = write_file_body(
            leaf,
            cursor,
            16,
            ts_create,
            ts_modify,
            ts_access,
            file_content.len() as u64,
            5, // extent at block 5
            1,
        );

        // Record 2: "subdir" folder
        offsets[2] = cursor as u16;
        cursor = write_key(leaf, cursor, 2, "subdir");
        let _cursor = write_folder_body(leaf, cursor, 32, ts_create, ts_modify, ts_access);

        // Write record offset table.
        for (i, &off) in offsets.iter().enumerate() {
            let pos = rec_off_start + i * 2;
            leaf[pos..pos + 2].copy_from_slice(&off.to_be_bytes());
        }

        // ===================================================================
        // Block 4: Leaf node (node 3) — subdir entries (parentCNID=32)
        // ===================================================================
        let sleaf = &mut img[block(4)..block(5)];

        sleaf[BT_F_LINK..BT_F_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        sleaf[BT_B_LINK..BT_B_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        sleaf[BT_KIND] = BT_LEAF_NODE;
        sleaf[BT_HEIGHT] = 0;
        sleaf[BT_NUM_RECORDS..BT_NUM_RECORDS + 2].copy_from_slice(&3u16.to_be_bytes());
        sleaf[BT_RESERVED..BT_RESERVED + 2].copy_from_slice(&0u16.to_be_bytes());

        let mut scursor = 0x0100;
        let mut soffsets: [u16; 3] = [0; 3];

        // Record 0: Folder thread for CNID=32
        soffsets[0] = scursor as u16;
        scursor = write_key(sleaf, scursor, 32, "");
        let sub_utf16: Vec<u16> = "subdir".encode_utf16().collect();
        let sub_cu = sub_utf16.len() as u16;
        let sub_thread_size = 8 + 2 + sub_cu as usize * 2;
        sleaf[scursor..scursor + 2].copy_from_slice(&RECORD_TYPE_FOLDER_THREAD.to_be_bytes());
        sleaf[scursor + 4..scursor + 8].copy_from_slice(&2u32.to_be_bytes()); // parentID=2
        sleaf[scursor + 8..scursor + 10].copy_from_slice(&sub_cu.to_be_bytes());
        for (i, &cu) in sub_utf16.iter().enumerate() {
            sleaf[scursor + 10 + i * 2..scursor + 12 + i * 2].copy_from_slice(&cu.to_be_bytes());
        }
        scursor += sub_thread_size;

        // Record 1: "nested.dat" file
        soffsets[1] = scursor as u16;
        scursor = write_key(sleaf, scursor, 32, "nested.dat");
        let nested_content = b"Nested HFS+ content";
        scursor = write_file_body(
            sleaf,
            scursor,
            48,
            ts_create,
            ts_modify,
            ts_access,
            nested_content.len() as u64,
            6, // extent at block 6
            1,
        );

        // Record 2: "deeper" folder (CNID=64, children in node 4)
        soffsets[2] = scursor as u16;
        scursor = write_key(sleaf, scursor, 32, "deeper");
        let _scursor = write_folder_body(sleaf, scursor, 64, ts_create, ts_modify, ts_access);

        for (i, &off) in soffsets.iter().enumerate() {
            let pos = rec_off_start + i * 2;
            sleaf[pos..pos + 2].copy_from_slice(&off.to_be_bytes());
        }

        // ===================================================================
        // Block 5: File data for "file.txt"
        // ===================================================================
        img[block(5)..block(5) + file_content.len()].copy_from_slice(file_content);

        // ===================================================================
        // Block 6: File data for "nested.dat"
        // ===================================================================
        img[block(6)..block(6) + nested_content.len()].copy_from_slice(nested_content);

        // ===================================================================
        // Block 7: Leaf node (node 4) -- deeper subdirectory entries
        //          (parentCNID=64, child of subdir/CNID=32)
        // ===================================================================
        let dleaf = &mut img[block(7)..block(8)];

        dleaf[BT_F_LINK..BT_F_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        dleaf[BT_B_LINK..BT_B_LINK + 4].copy_from_slice(&0u32.to_be_bytes());
        dleaf[BT_KIND] = BT_LEAF_NODE;
        dleaf[BT_HEIGHT] = 0;
        dleaf[BT_NUM_RECORDS..BT_NUM_RECORDS + 2].copy_from_slice(&2u16.to_be_bytes());
        dleaf[BT_RESERVED..BT_RESERVED + 2].copy_from_slice(&0u16.to_be_bytes());

        let mut dcursor = 0x0100;
        let mut doffsets: [u16; 2] = [0; 2];

        // Record 0: Folder thread for CNID=64
        doffsets[0] = dcursor as u16;
        dcursor = write_key(dleaf, dcursor, 64, "");
        let deeper_utf16: Vec<u16> = "deeper".encode_utf16().collect();
        let deeper_cu = deeper_utf16.len() as u16;
        let deeper_thread_size = 8 + 2 + deeper_cu as usize * 2;
        dleaf[dcursor..dcursor + 2].copy_from_slice(&RECORD_TYPE_FOLDER_THREAD.to_be_bytes());
        dleaf[dcursor + 4..dcursor + 8].copy_from_slice(&32u32.to_be_bytes()); // parentID=32 (subdir)
        dleaf[dcursor + 8..dcursor + 10].copy_from_slice(&deeper_cu.to_be_bytes());
        for (i, &cu) in deeper_utf16.iter().enumerate() {
            dleaf[dcursor + 10 + i * 2..dcursor + 12 + i * 2].copy_from_slice(&cu.to_be_bytes());
        }
        dcursor += deeper_thread_size;

        // Record 1: "deeper_file.txt" file, CNID=80, data at block 8
        doffsets[1] = dcursor as u16;
        dcursor = write_key(dleaf, dcursor, 64, "deeper_file.txt");
        let deeper_content = b"Deeper HFS+ content";
        let _dcursor = write_file_body(
            dleaf,
            dcursor,
            80,
            ts_create,
            ts_modify,
            ts_access,
            deeper_content.len() as u64,
            8, // extent at block 8
            1,
        );

        for (i, &off) in doffsets.iter().enumerate() {
            let pos = rec_off_start + i * 2;
            dleaf[pos..pos + 2].copy_from_slice(&off.to_be_bytes());
        }

        // ===================================================================
        // Block 8: File data for "deeper_file.txt"
        // ===================================================================
        img[block(8)..block(8) + deeper_content.len()].copy_from_slice(deeper_content);

        img
    }

    // -------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------

    #[test]
    fn test_volume_header() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        assert_eq!(hfs.data_source_name(), "hfsplus");
        assert_eq!(hfs.block_size(), 4096);
        assert_eq!(hfs.total_blocks(), 10);
        assert_eq!(hfs.free_blocks(), 1);
    }

    #[test]
    fn test_root_catalog_listing() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        let root = hfs.root().unwrap();
        assert_eq!(root.name, "\\");
        assert!(root.is_dir);

        // Root listing (parentCNID=2)
        let children = hfs.list_children("").unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(
            names.contains(&"file.txt"),
            "expected file.txt in root listing, got {names:?}"
        );
        assert!(
            names.contains(&"subdir"),
            "expected subdir in root listing, got {names:?}"
        );
    }

    #[test]
    fn test_file_inode_and_extents() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        // Open file.txt and read content.
        let mut f = hfs.open_file("file.txt").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Hello from HFS+!");
    }

    #[test]
    fn test_btree_key_existence_via_subdirectory() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        // List subdir contents.
        let sub_children = hfs.list_children("subdir").unwrap();
        let sub_names: Vec<&str> = sub_children.iter().map(|n| n.name.as_str()).collect();
        assert!(
            sub_names.contains(&"nested.dat"),
            "expected nested.dat in subdir listing, got {sub_names:?}"
        );

        // Open nested file.
        let mut f = hfs.open_file("subdir/nested.dat").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Nested HFS+ content");
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let mut img = build_hfsplus_fixture_v2();
        // Corrupt the volume header magic.
        let vh_off = VOLUME_HEADER_OFFSET as usize;
        img[vh_off + VH_SIGNATURE..vh_off + VH_SIGNATURE + 2]
            .copy_from_slice(&0x0000u16.to_be_bytes());

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        match HfsPlusReader::open(reader, 0) {
            Ok(_) => panic!("expected error for invalid magic"),
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::InvalidData);
                assert!(e.to_string().contains("magic"));
            }
        }
    }

    #[test]
    fn test_nonexistent_path() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        let e = hfs.list_children("nonexistent").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);

        match hfs.open_file("no_such.txt") {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
            Ok(_) => panic!("expected error for nonexistent file"),
        }
    }

    #[test]
    fn test_deeply_nested_path() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        // Verify we can list the intermediate directory.
        let deeper_children = hfs.list_children("subdir/deeper").unwrap();
        let deeper_names: Vec<&str> = deeper_children.iter().map(|n| n.name.as_str()).collect();
        assert!(
            deeper_names.contains(&"deeper_file.txt"),
            "expected deeper_file.txt in subdir/deeper listing, got {deeper_names:?}"
        );

        // Open deeply nested file.
        let mut f = hfs.open_file("subdir/deeper/deeper_file.txt").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Deeper HFS+ content");
    }

    #[test]
    fn test_missing_subdirectory() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        // Intermediate directory does not exist.
        let e = hfs.list_children("subdir/nonexistentdir").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);

        // File under nonexistent intermediate dir.
        match hfs.open_file("subdir/nonexistentdir/file.txt") {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
            Ok(_) => panic!("expected NotFound for path with missing intermediate dir"),
        }
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        // HFS+ is case-insensitive by default. "FILE.TXT" should find "file.txt".
        let mut f = hfs.open_file("FILE.TXT").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Hello from HFS+!");

        // Also verify case-insensitive lookup in a subdirectory.
        let mut f2 = hfs.open_file("SUBDIR/NESTED.DAT").unwrap();
        let mut s2 = String::new();
        f2.read_to_string(&mut s2).unwrap();
        assert_eq!(s2, "Nested HFS+ content");
    }

    #[test]
    fn test_case_sensitive_exact() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        // Exact-case matches work as well.
        let mut f = hfs.open_file("file.txt").unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();
        assert_eq!(s, "Hello from HFS+!");
    }

    #[test]
    fn test_timestamp_format() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        let children = hfs.list_children("").unwrap();
        let file_entry = children
            .iter()
            .find(|n| n.name == "file.txt")
            .expect("file.txt should be in root listing");

        // Timestamps should be present (non-None) when the fixture provides them.
        assert!(
            file_entry.created_at.is_some(),
            "created timestamp should be present for file.txt"
        );
        assert!(
            file_entry.modified_at.is_some(),
            "modified timestamp should be present for file.txt"
        );

        // Subdirectory should also have timestamps.
        let sub_children = hfs.list_children("subdir").unwrap();
        let sub_file = sub_children
            .iter()
            .find(|n| n.name == "nested.dat")
            .expect("nested.dat should be in subdir listing");
        assert!(
            sub_file.created_at.is_some(),
            "created timestamp should be present for nested.dat"
        );
    }

    #[test]
    fn test_root_node() {
        let img = build_hfsplus_fixture_v2();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let hfs = HfsPlusReader::open(reader, 0).unwrap();

        let root = hfs.root().unwrap();
        assert!(root.is_dir, "root node should be a directory");
        assert_eq!(root.name, "\\");
    }
}
