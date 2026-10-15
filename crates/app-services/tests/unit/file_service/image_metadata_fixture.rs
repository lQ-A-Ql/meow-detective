//! Synthetic EXIF in both byte orders, independent of image decoders and disk fixtures.

pub(super) fn tiff(little: bool) -> Vec<u8> {
    let mut bytes = vec![0; 512];
    bytes[..4].copy_from_slice(if little { b"II*\0" } else { b"MM\0*" });
    write_u32(&mut bytes, 4, 8, little);
    write_u16(&mut bytes, 8, 3, little);
    entry(&mut bytes, 10, (0x0112, 3, 1, 0), little);
    write_u16(&mut bytes, 18, 6, little);
    entry(&mut bytes, 22, (0x8769, 4, 1, 64), little);
    entry(&mut bytes, 34, (0x8825, 4, 1, 128), little);
    write_u16(&mut bytes, 64, 3, little);
    entry(&mut bytes, 66, (0x9003, 2, 20, 320), little);
    entry(&mut bytes, 78, (0x9004, 2, 20, 340), little);
    entry(&mut bytes, 90, (0xa434, 2, 8, 360), little);
    bytes[320..340].copy_from_slice(b"2026:10:14 12:34:56\0");
    bytes[340..360].copy_from_slice(b"2026:10:14 12:34:57\0");
    bytes[360..368].copy_from_slice(b"Lens 50\0");
    write_u16(&mut bytes, 128, 8, little);
    for (index, values) in [
        (1, 2, 2, 0),
        (2, 5, 3, 384),
        (3, 2, 2, 0),
        (4, 5, 3, 408),
        (5, 1, 1, 0),
        (6, 5, 1, 432),
        (7, 5, 3, 440),
        (29, 2, 11, 464),
    ]
    .into_iter()
    .enumerate()
    {
        entry(&mut bytes, 130 + index * 12, values, little);
    }
    bytes[138] = b'S';
    bytes[162] = b'W';
    bytes[186] = 1;
    for (offset, num, denom) in [
        (384, 1, 1),
        (392, 30, 1),
        (400, 0, 1),
        (408, 2, 1),
        (416, 0, 1),
        (424, 0, 1),
        (432, 10, 1),
        (440, 12, 1),
        (448, 34, 1),
        (456, 113, 2),
    ] {
        write_u32(&mut bytes, offset, num, little);
        write_u32(&mut bytes, offset + 4, denom, little);
    }
    bytes[464..475].copy_from_slice(b"2026:10:14\0");
    bytes
}

fn entry(bytes: &mut [u8], offset: usize, value: (u16, u16, u32, u32), little: bool) {
    write_u16(bytes, offset, value.0, little);
    write_u16(bytes, offset + 2, value.1, little);
    write_u32(bytes, offset + 4, value.2, little);
    write_u32(bytes, offset + 8, value.3, little);
}

pub(super) fn write_u16(bytes: &mut [u8], offset: usize, value: u16, little: bool) {
    bytes[offset..offset + 2].copy_from_slice(&if little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}

pub(super) fn write_u32(bytes: &mut [u8], offset: usize, value: u32, little: bool) {
    bytes[offset..offset + 4].copy_from_slice(&if little {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    });
}

pub(super) fn jpeg(exif: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0xe1];
    bytes.extend_from_slice(&((exif.len() + 8) as u16).to_be_bytes());
    bytes.extend_from_slice(b"Exif\0\0");
    bytes.extend_from_slice(exif);
    bytes.extend_from_slice(&[0xff, 0xd9]);
    bytes
}

pub(super) fn png(exif: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = [0; 13];
    ihdr[3] = 2;
    ihdr[7] = 3;
    ihdr[8] = 8;
    ihdr[9] = 2;
    png_chunk(&mut bytes, b"IHDR", &ihdr);
    if let Some(exif) = exif {
        png_chunk(&mut bytes, b"eXIf", exif);
    }
    png_chunk(&mut bytes, b"IEND", &[]);
    bytes
}

pub(super) fn png_chunk(bytes: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(payload);
    let mut hash = crc32fast::Hasher::new();
    hash.update(kind);
    hash.update(payload);
    bytes.extend_from_slice(&hash.finalize().to_be_bytes());
}

pub(super) fn webp(exif: &[u8]) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
    bytes.extend_from_slice(b"EXIF");
    bytes.extend_from_slice(&(exif.len() as u32).to_le_bytes());
    bytes.extend_from_slice(exif);
    if !exif.len().is_multiple_of(2) {
        bytes.push(0);
    }
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}
