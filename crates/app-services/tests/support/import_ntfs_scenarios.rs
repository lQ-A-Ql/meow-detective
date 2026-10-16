//! A distinct deterministic workload; never relabel the original empty-index fixture.
use super::fixture::{self, MFT_OFFSET, RECORD_BYTES};

pub(super) fn image(count: usize, scenario: &str) -> Vec<u8> {
    let mut bytes = fixture::image(count);
    match scenario {
        "resident" => return bytes,
        "indexed-fragmented" => {}
        _ => panic!("unknown synthetic scenario: {scenario}"),
    }
    let lcn = count + 8;
    bytes.resize((lcn + 8) * 1024, 0);
    let sectors = (bytes.len() / 512) as u64;
    bytes[40..48].copy_from_slice(&sectors.to_le_bytes());
    for inode in std::iter::once(5).chain(16..count) {
        let name = if inode == 5 {
            ".".into()
        } else {
            format!("file-{inode}.txt")
        };
        let mut r = fixture::record(inode, &name, count);
        undo_fixups(&mut r);
        let mut p = 56;
        while u32::from_le_bytes(r[p..p + 4].try_into().unwrap()) != u32::MAX {
            let kind = u32::from_le_bytes(r[p..p + 4].try_into().unwrap());
            if kind == 0x10 || kind == 0x30 {
                let start = p + 24 + if kind == 0x30 { 8 } else { 0 };
                for (i, offset) in [0, 8, 16, 24].into_iter().enumerate() {
                    let stamp = 125_911_584_000_000_000u64
                        + ((inode * 7919 + i * 104729) % 31_536_000) as u64 * 10_000_000;
                    r[start + offset..start + offset + 8].copy_from_slice(&stamp.to_le_bytes());
                }
                if kind == 0x30 && inode > 19 {
                    r[p + 24..p + 32].copy_from_slice(&(16u64 | (7u64 << 48)).to_le_bytes());
                }
            }
            if kind == 0x80 || kind == 0x90 {
                break;
            }
            p += u32::from_le_bytes(r[p + 4..p + 8].try_into().unwrap()) as usize;
        }
        if inode == 5 {
            r[22..24].copy_from_slice(&3u16.to_le_bytes());
            // Retain the root index and append a named nonresident $I30 allocation.
            p += u32::from_le_bytes(r[p + 4..p + 8].try_into().unwrap()) as usize;
            r[p..].fill(0);
            p = nonresident(&mut r, p, 0xa0, lcn, false, true);
            let bitmap = [1u8];
            p = named_resident(&mut r, p, 0xb0, &bitmap);
        } else if inode == 16 {
            r[22..24].copy_from_slice(&3u16.to_le_bytes());
            r[p..].fill(0);
            let mut root = [0u8; 48];
            root[..4].copy_from_slice(&0x30u32.to_le_bytes());
            root[8..12].copy_from_slice(&1024u32.to_le_bytes());
            root[16..20].copy_from_slice(&16u32.to_le_bytes());
            root[20..24].copy_from_slice(&32u32.to_le_bytes());
            root[24..28].copy_from_slice(&32u32.to_le_bytes());
            root[40..42].copy_from_slice(&16u16.to_le_bytes());
            root[44..46].copy_from_slice(&2u16.to_le_bytes());
            p = fixture::resident(&mut r, p, 0x90, &root);
        } else if inode == 17 {
            r[p..].fill(0);
            p = nonresident(&mut r, p, 0x80, lcn + 2, true, false);
        } else if inode == 18 {
            r[p..].fill(0);
            let mut list = [0u8; 32];
            list[..4].copy_from_slice(&0x80u32.to_le_bytes());
            list[4..6].copy_from_slice(&32u16.to_le_bytes());
            list[16..24].copy_from_slice(&(19u64 | (7u64 << 48)).to_le_bytes());
            p = fixture::resident(&mut r, p, 0x20, &list);
        } else if inode == 19 {
            r[32..40].copy_from_slice(&(18u64 | (7u64 << 48)).to_le_bytes());
            p += u32::from_le_bytes(r[p + 4..p + 8].try_into().unwrap()) as usize;
        } else {
            p += u32::from_le_bytes(r[p + 4..p + 8].try_into().unwrap()) as usize;
        }
        if inode % 97 == 0 {
            r[22..24].copy_from_slice(&0u16.to_le_bytes());
        }
        finish(&mut r, p);
        bytes[MFT_OFFSET + inode * RECORD_BYTES..MFT_OFFSET + (inode + 1) * RECORD_BYTES]
            .copy_from_slice(&r);
    }
    bytes[lcn * 1024..(lcn + 1) * 1024].copy_from_slice(&index_record());
    bytes[(lcn + 2) * 1024..(lcn + 3) * 1024].fill(b'A');
    bytes[(lcn + 6) * 1024..(lcn + 7) * 1024].fill(b'B');
    bytes
}

fn undo_fixups(r: &mut [u8]) {
    for (tail, replacement) in [(510, 50), (1022, 52)] {
        let original = [r[replacement], r[replacement + 1]];
        r[tail..tail + 2].copy_from_slice(&original);
    }
}
fn finish(r: &mut [u8], p: usize) {
    r[p..p + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    r[24..28].copy_from_slice(&((p + 4) as u32).to_le_bytes());
    for (tail, replacement) in [(510, 50), (1022, 52)] {
        let original = [r[tail], r[tail + 1]];
        r[replacement..replacement + 2].copy_from_slice(&original);
        r[tail..tail + 2].copy_from_slice(&[0x55, 0xaa]);
    }
}
fn named_resident(r: &mut [u8], p: usize, kind: u32, data: &[u8]) -> usize {
    let end = fixture::resident(r, p, kind, &[vec![0u8; 8], data.to_vec()].concat());
    r[p + 9] = 4;
    r[p + 10..p + 12].copy_from_slice(&24u16.to_le_bytes());
    r[p + 16..p + 20].copy_from_slice(&(data.len() as u32).to_le_bytes());
    r[p + 20..p + 22].copy_from_slice(&32u16.to_le_bytes());
    for (i, c) in "$I30".encode_utf16().enumerate() {
        r[p + 24 + i * 2..p + 26 + i * 2].copy_from_slice(&c.to_le_bytes());
    }
    end
}
fn nonresident(r: &mut [u8], p: usize, kind: u32, lcn: usize, split: bool, named: bool) -> usize {
    let run = if named { 72 } else { 64 };
    r[p..p + 4].copy_from_slice(&kind.to_le_bytes());
    r[p + 4..p + 8].copy_from_slice(&88u32.to_le_bytes());
    r[p + 8] = 1;
    r[p + 24..p + 32].copy_from_slice(&(u64::from(split)).to_le_bytes());
    r[p + 32..p + 34].copy_from_slice(&(run as u16).to_le_bytes());
    for (offset, size) in [
        (40, if split { 2048u64 } else { 1024 }),
        (48, if split { 1800 } else { 1024 }),
        (56, if split { 1300 } else { 1024 }),
    ] {
        r[p + offset..p + offset + 8].copy_from_slice(&size.to_le_bytes());
    }
    if named {
        r[p + 9] = 4;
        r[p + 10..p + 12].copy_from_slice(&64u16.to_le_bytes());
        for (i, c) in "$I30".encode_utf16().enumerate() {
            r[p + 64 + i * 2..p + 66 + i * 2].copy_from_slice(&c.to_le_bytes());
        }
    }
    r[p + run] = 0x31;
    r[p + run + 1] = 1;
    r[p + run + 2..p + run + 5].copy_from_slice(&(lcn as u32).to_le_bytes()[..3]);
    if split {
        r[p + run + 5..p + run + 8].copy_from_slice(&[0x11, 1, 4]);
    }
    p + 88
}
fn index_record() -> [u8; 1024] {
    let mut r = [0u8; 1024];
    r[..4].copy_from_slice(b"INDX");
    r[4..6].copy_from_slice(&0x28u16.to_le_bytes());
    r[6..8].copy_from_slice(&3u16.to_le_bytes());
    let name: Vec<_> = "indexed-17.txt".encode_utf16().collect();
    let len = (0x52 + name.len() * 2).div_ceil(8) * 8;
    r[0x18..0x1c].copy_from_slice(&0x18u32.to_le_bytes());
    for offset in [0x1c, 0x20] {
        r[offset..offset + 4].copy_from_slice(&((0x18 + len + 16) as u32).to_le_bytes());
    }
    let p = 0x30;
    r[p..p + 8].copy_from_slice(&(17u64 | (7u64 << 48)).to_le_bytes());
    r[p + 8..p + 10].copy_from_slice(&(len as u16).to_le_bytes());
    r[p + 10..p + 12].copy_from_slice(&((66 + name.len() * 2) as u16).to_le_bytes());
    r[p + 0x50] = name.len() as u8;
    r[p + 0x51] = 1;
    for (i, c) in name.into_iter().enumerate() {
        r[p + 0x52 + i * 2..p + 0x54 + i * 2].copy_from_slice(&c.to_le_bytes());
    }
    r[p + len + 8..p + len + 10].copy_from_slice(&16u16.to_le_bytes());
    r[p + len + 12..p + len + 14].copy_from_slice(&2u16.to_le_bytes());
    r[0x28..0x2a].copy_from_slice(&[0x55, 0xaa]);
    for (tail, replacement) in [(510, 0x2a), (1022, 0x2c)] {
        let orig = [r[tail], r[tail + 1]];
        r[replacement..replacement + 2].copy_from_slice(&orig);
        r[tail..tail + 2].copy_from_slice(&[0x55, 0xaa]);
    }
    r
}
