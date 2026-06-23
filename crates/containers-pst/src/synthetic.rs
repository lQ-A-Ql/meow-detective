//! Synthetic PST fixture builder.

use crate::header::{
    bid_to_bytes, BTREE_BB, BTREE_NB, NID_MESSAGE_STORE, NID_ROOT_FOLDER, PST_MAGIC,
};
use crate::props::{
    prop_type, PROP_TAG_MESSAGE_CLASS, PROP_TAG_SENDER_EMAIL, PROP_TAG_SENDER_NAME,
    PROP_TAG_SUBJECT,
};

/// Build a minimal synthetic Unicode PST for testing.
///
/// Produces a valid 512-byte-aligned in-memory PST with header, NBT, BBT,
/// and property context pages. Used by both PST and OST test suites.
#[doc(hidden)]
pub fn build_synthetic_pst() -> Vec<u8> {
    build_synthetic_unicode_pst()
}

/// Build the full synthetic Unicode PST fixture.
pub(crate) fn build_synthetic_unicode_pst() -> Vec<u8> {
    let mut pst = vec![0u8; 512 * 8]; // 8 pages

    // ═══ PAGE 0: Header (512 bytes) ═══
    let header = &mut pst[0..512];

    // Magic: "!BDN"
    header[0..4].copy_from_slice(&PST_MAGIC);
    // dwCRCPartial (4-7): zero
    // wMagicClient (8-9): "SM"
    header[8] = 0x53; // 'S'
    header[9] = 0x4D; // 'M'
                      // wVer (10-11): 23 = Unicode (64-bit)
    header[10] = 23u8;
    header[11] = 0u8;
    // wVerClient (12-13): 19
    header[12] = 19u8;
    header[13] = 0u8;
    // bPlatformCreate (14): 1
    header[14] = 1u8;
    // bPlatformAccess (15): 1
    header[15] = 1u8;

    // ROOT at offset 188 (Unicode):
    let file_size = (512 * 8) as u64;
    header[188 + 4..188 + 12].copy_from_slice(&file_size.to_le_bytes());
    // brefNBT (bid=4, ib=2048)
    let nbt_bid: u64 = 4;
    let nbt_ib: u64 = 512 * 4;
    header[188 + 36..188 + 44].copy_from_slice(&nbt_bid.to_le_bytes());
    header[188 + 44..188 + 52].copy_from_slice(&nbt_ib.to_le_bytes());
    // brefBBT (bid=2, ib=1024)
    let bbt_bid: u64 = 2;
    let bbt_ib: u64 = 512 * 2;
    header[188 + 56..188 + 64].copy_from_slice(&bbt_bid.to_le_bytes());
    header[188 + 64..188 + 72].copy_from_slice(&bbt_ib.to_le_bytes());

    // ═══ PAGE 2: BBT leaf page (at offset 1024) ═══
    let bbt_page = &mut pst[1024..1536];
    bbt_page[0] = BTREE_BB;
    bbt_page[1] = 0x00;
    bbt_page[2] = 0xEC;
    bbt_page[3] = 1;
    let _ = bid_to_bytes(2, &mut bbt_page[8..16]);
    bbt_page[22] = 0u8; // leaf level
    bbt_page[23] = 6u8; // 6 entries

    let cb_ent: u16 = 24; // Unicode BBT entry: bref(16) + cb(2) + cRef(2) + alignment(4)
    bbt_page[24] = cb_ent as u8;
    bbt_page[25] = (cb_ent >> 8) as u8;

    let bbt_entries: [(u64, u64, u16); 6] = [
        (1, 0, 1),
        (2, 1024, 1),
        (3, 1536, 1),
        (4, 2048, 1),
        (5, 2560, 1),
        (6, 3072, 1),
    ];
    for (i, (bid_val, ib_val, c_ref)) in bbt_entries.iter().enumerate() {
        let offset = 40 + i * cb_ent as usize;
        bbt_page[offset..offset + 8].copy_from_slice(&bid_val.to_le_bytes());
        bbt_page[offset + 8..offset + 16].copy_from_slice(&ib_val.to_le_bytes());
        bbt_page[offset + 16..offset + 18].copy_from_slice(&0u16.to_le_bytes()); // cb = 0
        bbt_page[offset + 18..offset + 20].copy_from_slice(&c_ref.to_le_bytes());
    }

    // ═══ PAGE 4: NBT root page (leaf) at offset 2048 ═══
    let nbt_page = &mut pst[2048..2560];
    nbt_page[0] = BTREE_NB;
    nbt_page[1] = 0x00;
    nbt_page[2] = 0xEC;
    nbt_page[3] = 1;
    let _ = bid_to_bytes(4, &mut nbt_page[8..16]);
    nbt_page[22] = 0u8; // leaf level
    nbt_page[23] = 4u8; // 4 entries

    let nbt_ent_size: u16 = 24;
    nbt_page[24] = nbt_ent_size as u8;
    nbt_page[25] = (nbt_ent_size >> 8) as u8;

    let nbt_entries: [(u32, u64, u64); 4] = [
        (NID_MESSAGE_STORE, 5, 0),
        (NID_ROOT_FOLDER, 6, 0),
        (0x8001, 7, 0),
        (0x8021, 8, 0),
    ];
    for (i, (nid, bid_data, bid_sub)) in nbt_entries.iter().enumerate() {
        let offset = 40 + i * nbt_ent_size as usize;
        nbt_page[offset..offset + 4].copy_from_slice(&nid.to_le_bytes());
        nbt_page[offset + 8..offset + 16].copy_from_slice(&bid_data.to_le_bytes());
        nbt_page[offset + 16..offset + 24].copy_from_slice(&bid_sub.to_le_bytes());
    }

    // ═══ PAGE 5: Property context for message store (offset 2560) ═══
    let pc_page = &mut pst[2560..3072];
    let bth_offset: usize = 40;
    let data_offset = bth_offset + 8;
    let hid_root: u32 = data_offset as u32;
    pc_page[bth_offset] = 0xB5;
    pc_page[bth_offset + 1] = 2u8;
    pc_page[bth_offset + 2] = 12u8;
    pc_page[bth_offset + 3] = 0u8;
    pc_page[bth_offset + 4..bth_offset + 8].copy_from_slice(&hid_root.to_le_bytes());

    let tag_display_name: u32 = 0x3001_001F;
    pc_page[data_offset..data_offset + 4].copy_from_slice(&tag_display_name.to_le_bytes());
    let str_offset = data_offset + 12;
    let name = "MessageStore\0";
    let utf16_bytes: Vec<u8> = name.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let str_len = utf16_bytes.len() as u32;
    pc_page[data_offset + 4..data_offset + 8].copy_from_slice(&str_len.to_le_bytes());
    pc_page[str_offset..str_offset + utf16_bytes.len()].copy_from_slice(&utf16_bytes);

    // ═══ PAGE 6: Property context for root folder (offset 3072) ═══
    let pc_page6 = &mut pst[3072..3584];
    let bth_offset6: usize = 40;
    let data_offset6 = bth_offset6 + 8;
    let hid_root6: u32 = data_offset6 as u32;
    pc_page6[bth_offset6] = 0xB5;
    pc_page6[bth_offset6 + 1] = 2u8;
    pc_page6[bth_offset6 + 2] = 12u8;
    pc_page6[bth_offset6 + 3] = 0u8;
    pc_page6[bth_offset6 + 4..bth_offset6 + 8].copy_from_slice(&hid_root6.to_le_bytes());

    let tag_display: u32 = 0x3001_001F;
    pc_page6[data_offset6..data_offset6 + 4].copy_from_slice(&tag_display.to_le_bytes());
    let str_offset6 = data_offset6 + 12;
    let name6 = "RootFolder\0";
    let utf16_bytes6: Vec<u8> = name6.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let str_len6 = utf16_bytes6.len() as u32;
    pc_page6[data_offset6 + 4..data_offset6 + 8].copy_from_slice(&str_len6.to_le_bytes());
    pc_page6[str_offset6..str_offset6 + utf16_bytes6.len()].copy_from_slice(&utf16_bytes6);

    // ═══ PAGE 7: Property context for a synthetic message (offset 3584) ═══
    let pc_page7 = &mut pst[3584..4096];
    let bth_offset7: usize = 40;
    let data_offset7 = bth_offset7 + 8;
    let hid_root7: u32 = data_offset7 as u32;
    pc_page7[bth_offset7] = 0xB5;
    pc_page7[bth_offset7 + 1] = 2u8;
    pc_page7[bth_offset7 + 2] = 12u8;
    pc_page7[bth_offset7 + 3] = 0u8;
    pc_page7[bth_offset7 + 4..bth_offset7 + 8].copy_from_slice(&hid_root7.to_le_bytes());

    let props: &[(u32, &str)] = &[
        (
            (PROP_TAG_SUBJECT as u32) << 16 | prop_type::PtypString as u32,
            "Test Subject",
        ),
        (
            (PROP_TAG_MESSAGE_CLASS as u32) << 16 | prop_type::PtypString as u32,
            "IPM.Note",
        ),
        (
            (PROP_TAG_SENDER_NAME as u32) << 16 | prop_type::PtypString as u32,
            "Test Sender",
        ),
        (
            (PROP_TAG_SENDER_EMAIL as u32) << 16 | prop_type::PtypString as u32,
            "sender@test.com",
        ),
    ];

    let mut entry_pos = data_offset7;
    let mut string_storage = entry_pos + props.len() * 12;

    for (tag, value) in props {
        pc_page7[entry_pos..entry_pos + 4].copy_from_slice(&tag.to_le_bytes());
        let utf16: Vec<u8> = format!("{}\0", value)
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let slen = utf16.len() as u32;
        pc_page7[entry_pos + 4..entry_pos + 8].copy_from_slice(&slen.to_le_bytes());
        if string_storage + utf16.len() < 512 {
            pc_page7[string_storage..string_storage + utf16.len()].copy_from_slice(&utf16);
        }
        string_storage += utf16.len();
        entry_pos += 12;
    }

    pst
}
