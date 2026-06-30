//! Synthetic PST fixture builder.

use crate::header::{bid_to_bytes, NID_MESSAGE_STORE, NID_ROOT_FOLDER, PAGE_SIZE, PST_MAGIC};
use crate::props::{
    prop_type, PROP_TAG_BODY, PROP_TAG_MESSAGE_CLASS, PROP_TAG_SENDER_EMAIL, PROP_TAG_SENDER_NAME,
    PROP_TAG_SUBJECT,
};

/// Number of pages in the base synthetic PST.
const BASE_PAGE_COUNT: usize = 10;

/// Build a minimal synthetic Unicode PST for testing.
///
/// Produces a valid 512-byte-aligned in-memory PST with header, NBT, BBT,
/// and property context pages. Used by both PST and OST test suites.
#[doc(hidden)]
pub fn build_synthetic_pst() -> Vec<u8> {
    build_synthetic_unicode_pst()
}

/// Build a synthetic Unicode PST containing multiple messages.
///
/// Messages are assigned sequential NIDs starting at `0x8001` and live on
/// their own property context pages. The NBT and BBT are updated to reflect
/// the additional pages. This is intended for medium-scale fixture data.
#[doc(hidden)]
pub fn build_synthetic_pst_with_messages(message_count: usize) -> Vec<u8> {
    build_synthetic_unicode_pst_with_messages(message_count)
}

/// Build the full synthetic Unicode PST fixture.
pub(crate) fn build_synthetic_unicode_pst() -> Vec<u8> {
    build_synthetic_unicode_pst_with_messages(1)
}

/// Build the full synthetic Unicode PST fixture with an arbitrary message count.
pub(crate) fn build_synthetic_unicode_pst_with_messages(message_count: usize) -> Vec<u8> {
    // Static allocations: header(0), BBT(2), NBT(4), message store(5), root folder(6),
    // plus one page per message. Reserve 10 + messages.
    let page_count = BASE_PAGE_COUNT + message_count;
    assert!(
        message_count <= 12,
        "single-page NBT/BBT can hold at most 12 extra message entries"
    );

    let mut pst = vec![0u8; PAGE_SIZE * page_count];

    // ═══ PAGE 0: Header (512 bytes) ═══
    let header = &mut pst[0..PAGE_SIZE];

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
    let file_size = (PAGE_SIZE * page_count) as u64;
    header[188 + 4..188 + 12].copy_from_slice(&file_size.to_le_bytes());
    // brefNBT (bid=4, ib=2048)
    let nbt_bid: u64 = 4;
    let nbt_ib: u64 = PAGE_SIZE as u64 * 4;
    header[188 + 36..188 + 44].copy_from_slice(&nbt_bid.to_le_bytes());
    header[188 + 44..188 + 52].copy_from_slice(&nbt_ib.to_le_bytes());
    // brefBBT (bid=2, ib=1024)
    let bbt_bid: u64 = 2;
    let bbt_ib: u64 = PAGE_SIZE as u64 * 2;
    header[188 + 56..188 + 64].copy_from_slice(&bbt_bid.to_le_bytes());
    header[188 + 64..188 + 72].copy_from_slice(&bbt_ib.to_le_bytes());

    // ═══ PAGE 2: BBT leaf page (at offset 1024) ═══
    let bbt_page = &mut pst[PAGE_SIZE * 2..PAGE_SIZE * 3];
    bbt_page[0] = 0x80; // wSig low byte
    bbt_page[1] = 0x00;
    bbt_page[2] = 0xEC;
    bbt_page[3] = 1;
    let _ = bid_to_bytes(2, &mut bbt_page[8..16]);
    bbt_page[22] = 0u8; // leaf level
    let cb_ent: u16 = 24;
    bbt_page[24] = cb_ent as u8;
    bbt_page[25] = (cb_ent >> 8) as u8;

    // BBT entries: header(1), BBT page itself(2), two spare static pages(3,8), NBT(4),
    // message store(5), root folder(6), message pages(7..7+message_count).
    let mut bbt_entries: Vec<(u64, u64, u16)> = vec![
        (1, 0, 1),
        (2, 1024, 1),
        (3, 1536, 1),
        (4, 2048, 1),
        (5, 2560, 1),
        (6, 3072, 1),
    ];
    for i in 0..message_count {
        let bid = 7 + i as u64;
        let ib = PAGE_SIZE as u64 * (7 + i as u64);
        bbt_entries.push((bid, ib, 1));
    }
    // Fill remaining slot with a placeholder page 8 if not already used.
    if message_count < 2 && !bbt_entries.iter().any(|(b, _, _)| *b == 8) {
        bbt_entries.push((8, 4096, 1));
    }

    bbt_page[23] = bbt_entries.len() as u8;
    for (i, (bid_val, ib_val, c_ref)) in bbt_entries.iter().enumerate() {
        let offset = 40 + i * cb_ent as usize;
        bbt_page[offset..offset + 8].copy_from_slice(&bid_val.to_le_bytes());
        bbt_page[offset + 8..offset + 16].copy_from_slice(&ib_val.to_le_bytes());
        bbt_page[offset + 16..offset + 18].copy_from_slice(&0u16.to_le_bytes()); // cb = 0
        bbt_page[offset + 18..offset + 20].copy_from_slice(&c_ref.to_le_bytes());
    }

    // ═══ PAGE 4: NBT root page (leaf) at offset 2048 ═══
    let nbt_page = &mut pst[PAGE_SIZE * 4..PAGE_SIZE * 5];
    nbt_page[0] = 0x81; // wSig low byte
    nbt_page[1] = 0x00;
    nbt_page[2] = 0xEC;
    nbt_page[3] = 1;
    let _ = bid_to_bytes(4, &mut nbt_page[8..16]);
    nbt_page[22] = 0u8; // leaf level

    let nbt_ent_size: u16 = 24;
    nbt_page[24] = nbt_ent_size as u8;
    nbt_page[25] = (nbt_ent_size >> 8) as u8;

    // NBT entries: message store, root folder, one entry per message.
    let mut nbt_entries: Vec<(u32, u64, u64)> =
        vec![(NID_MESSAGE_STORE, 5, 0), (NID_ROOT_FOLDER, 6, 0)];
    for i in 0..message_count {
        let nid = 0x8001 + i as u32;
        let bid_data = 7 + i as u64;
        nbt_entries.push((nid, 0, bid_data));
    }

    nbt_page[23] = nbt_entries.len() as u8;
    for (i, (nid, bid_data, bid_sub)) in nbt_entries.iter().enumerate() {
        let offset = 40 + i * nbt_ent_size as usize;
        nbt_page[offset..offset + 4].copy_from_slice(&nid.to_le_bytes());
        nbt_page[offset + 8..offset + 16].copy_from_slice(&bid_data.to_le_bytes());
        nbt_page[offset + 16..offset + 24].copy_from_slice(&bid_sub.to_le_bytes());
    }

    // ═══ PAGE 5: Property context for message store (offset 2560) ═══
    write_simple_property_context(
        &mut pst[PAGE_SIZE * 5..PAGE_SIZE * 6],
        &[(0x3001, "MessageStore")],
    );

    // ═══ PAGE 6: Property context for root folder (offset 3072) ═══
    write_simple_property_context(
        &mut pst[PAGE_SIZE * 6..PAGE_SIZE * 7],
        &[(0x3001, "RootFolder")],
    );

    // ═══ PAGE 7..: Property context for each synthetic message ═══
    for i in 0..message_count {
        let idx = i + 1;
        let subject = format!("Synthetic Subject {idx}");
        let sender_name = format!("Sender {idx}");
        let sender_email = format!("sender{idx}@example.com");
        let body = format!("Body text for synthetic message number {idx}.");
        write_simple_property_context(
            &mut pst[PAGE_SIZE * (7 + i)..PAGE_SIZE * (8 + i)],
            &[
                (PROP_TAG_SUBJECT, subject.as_str()),
                (PROP_TAG_MESSAGE_CLASS, "IPM.Note"),
                (PROP_TAG_SENDER_NAME, sender_name.as_str()),
                (PROP_TAG_SENDER_EMAIL, sender_email.as_str()),
                (PROP_TAG_BODY, body.as_str()),
            ],
        );
    }

    pst
}

/// Write a minimal property context (Heap-on-Node + BTree-on-Heap) into `page`.
///
/// The BTH uses 4-byte keys (full property tags), an entry size of 48 bytes,
/// and stores Unicode string values inline so that `parse_property_context`
/// can read them directly.
fn write_simple_property_context(page: &mut [u8], props: &[(u16, &str)]) {
    let bth_offset: usize = 40;
    let data_offset = bth_offset + 8;
    let hid_root: u32 = data_offset as u32;

    // BTH header.
    page[bth_offset] = 0xB5;
    page[bth_offset + 1] = 4u8; // cbKey: full 4-byte property tag
    page[bth_offset + 2] = 48u8; // cbEnt (large enough for inline strings)
    page[bth_offset + 3] = 0u8;
    page[bth_offset + 4..bth_offset + 8].copy_from_slice(&hid_root.to_le_bytes());

    let cb_ent = 48usize;
    for (i, (prop_id, value)) in props.iter().enumerate() {
        let entry_offset = data_offset + i * cb_ent;
        let tag = ((*prop_id as u32) << 16) | prop_type::PtypString as u32;
        page[entry_offset..entry_offset + 4].copy_from_slice(&tag.to_le_bytes());

        let utf16: Vec<u8> = format!("{}\0", value)
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let slen = utf16.len() as u32;
        page[entry_offset + 4..entry_offset + 8].copy_from_slice(&slen.to_le_bytes());
        let value_start = entry_offset + 8;
        page[value_start..value_start + utf16.len()].copy_from_slice(&utf16);
    }
}
