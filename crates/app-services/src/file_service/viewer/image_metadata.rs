use transport::dto::{ImageMetadataDto, ImageMetadataStatusDto};

use crate::file_service::FileServiceError;

use super::{
    open_file_handle_real, preview_bytes::read_preview_bytes_for_file, PreviewReadContext,
};

const MAX_METADATA_READ: u32 = 2 * 1024 * 1024;

pub fn image_metadata_for_file<C>(
    mut context: C,
    file_id: &str,
) -> Result<ImageMetadataDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;
    let mime = handle.mime.as_deref().unwrap_or("");
    let format = format_from_mime(mime);
    if format.is_none() {
        return Ok(empty(ImageMetadataStatusDto::Unsupported, None));
    }
    let length = handle.size.min(MAX_METADATA_READ as u64) as u32;
    let bytes = read_preview_bytes_for_file(&mut context, file_id, 0, length)?;
    if bytes.is_empty() {
        return Ok(empty(ImageMetadataStatusDto::Corrupt, format));
    }
    let dimensions = dimensions(&bytes, mime);
    let mut result = empty(ImageMetadataStatusDto::Absent, format);
    result.width = dimensions.map(|value| value.0);
    result.height = dimensions.map(|value| value.1);
    if let Some(exif) = find_exif(&bytes) {
        result.status = if parse_tiff(exif, &mut result) {
            ImageMetadataStatusDto::Present
        } else {
            ImageMetadataStatusDto::Corrupt
        };
    }
    Ok(result)
}

fn empty(status: ImageMetadataStatusDto, format: Option<String>) -> ImageMetadataDto {
    ImageMetadataDto {
        status,
        format,
        width: None,
        height: None,
        orientation: None,
        make: None,
        model: None,
        software: None,
        date_time_original: None,
        create_date: None,
        modify_date: None,
        lens_model: None,
        latitude: None,
        longitude: None,
        altitude: None,
        gps_date_time: None,
    }
}

fn format_from_mime(mime: &str) -> Option<String> {
    Some(
        match mime {
            "image/jpeg" => "jpeg",
            "image/png" => "png",
            "image/tiff" => "tiff",
            "image/webp" => "webp",
            _ => return None,
        }
        .to_string(),
    )
}

fn dimensions(bytes: &[u8], mime: &str) -> Option<(u32, u32)> {
    match mime {
        "image/png" if bytes.len() >= 24 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" => Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        )),
        "image/jpeg" => jpeg_dimensions(bytes),
        _ => None,
    }
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut p = 2;
    while p + 9 < bytes.len() {
        if bytes[p] != 0xff {
            p += 1;
            continue;
        }
        let marker = bytes[p + 1];
        p += 2;
        if marker == 0xd8 || marker == 0xd9 {
            continue;
        }
        let len = u16::from_be_bytes(bytes.get(p..p + 2)?.try_into().ok()?) as usize;
        if len < 2 || p + len > bytes.len() {
            return None;
        }
        if (0xc0..=0xc3).contains(&marker)
            || (0xc5..=0xc7).contains(&marker)
            || (0xc9..=0xcb).contains(&marker)
            || (0xcd..=0xcf).contains(&marker)
        {
            return Some((
                u16::from_be_bytes(bytes[p + 5..p + 7].try_into().ok()?) as u32,
                u16::from_be_bytes(bytes[p + 3..p + 5].try_into().ok()?) as u32,
            ));
        }
        p += len;
    }
    None
}

fn find_exif(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return Some(bytes);
    }
    if bytes.get(..2) != Some(&[0xff, 0xd8]) {
        return None;
    }
    let mut p = 2;
    while p + 4 <= bytes.len() {
        if bytes[p] != 0xff {
            p += 1;
            continue;
        }
        let marker = bytes[p + 1];
        p += 2;
        if marker == 0xda {
            break;
        }
        let len = u16::from_be_bytes(bytes.get(p..p + 2)?.try_into().ok()?) as usize;
        if len < 2 || p + len > bytes.len() {
            return None;
        }
        if marker == 0xe1 && len >= 8 && &bytes[p + 2..p + 8] == b"Exif\0\0" {
            return Some(&bytes[p + 8..p + len]);
        }
        p += len;
    }
    None
}

fn parse_tiff(bytes: &[u8], out: &mut ImageMetadataDto) -> bool {
    let Some(little) = bytes.get(..2).map(|v| v == b"II") else {
        return false;
    };
    if bytes.get(2..4) != Some(if little { b"*\0" } else { b"\0*" }) {
        return false;
    }
    let Some(ifd) = read_u32(bytes, 4, little).map(|value| value as usize) else {
        return false;
    };
    parse_ifd(bytes, ifd, little, out)
}

fn parse_ifd(bytes: &[u8], offset: usize, little: bool, out: &mut ImageMetadataDto) -> bool {
    let Some(count) = read_u16(bytes, offset, little).map(|v| v as usize) else {
        return false;
    };
    for i in 0..count.min(256) {
        let Some(p) = offset.checked_add(2 + i * 12) else {
            break;
        };
        if p.checked_add(12).is_none() || p + 12 > bytes.len() {
            break;
        }
        let Some(tag) = read_u16(bytes, p, little) else {
            break;
        };
        let typ = read_u16(bytes, p + 2, little).unwrap_or(0);
        let n = read_u32(bytes, p + 4, little).unwrap_or(0);
        let value = value_bytes(bytes, p + 8, typ, n, little);
        match tag {
            0x0112 => out.orientation = value.and_then(|v| read_u16(v, 0, little)),
            0x010f => out.make = text(value),
            0x0110 => out.model = text(value),
            0x0131 => out.software = text(value),
            0x9003 => out.date_time_original = text(value),
            0x9004 => out.create_date = text(value),
            0x0132 => out.modify_date = text(value),
            0xa434 => out.lens_model = text(value),
            0x8825 => {
                if let Some(gps_offset) = value.and_then(|v| read_u32(v, 0, little)) {
                    parse_gps_ifd(bytes, gps_offset as usize, little, out);
                }
            }
            _ => {}
        }
    }
    true
}

fn parse_gps_ifd(bytes: &[u8], offset: usize, little: bool, out: &mut ImageMetadataDto) {
    let Some(count) = read_u16(bytes, offset, little).map(|value| value as usize) else {
        return;
    };
    let mut latitude_ref = None;
    let mut longitude_ref = None;
    let mut altitude_ref = None;
    let mut latitude = None;
    let mut longitude = None;
    let mut altitude = None;
    let mut gps_time = None;
    let mut gps_date = None;
    for i in 0..count.min(128) {
        let Some(p) = offset.checked_add(2 + i * 12) else {
            break;
        };
        if p.checked_add(12).is_none() || p + 12 > bytes.len() {
            break;
        }
        let Some(tag) = read_u16(bytes, p, little) else {
            continue;
        };
        let typ = read_u16(bytes, p + 2, little).unwrap_or(0);
        let count = read_u32(bytes, p + 4, little).unwrap_or(0);
        let value = value_bytes(bytes, p + 8, typ, count, little);
        match tag {
            1 => latitude_ref = text(value),
            2 => latitude = rational_triplet(value, little),
            3 => longitude_ref = text(value),
            4 => longitude = rational_triplet(value, little),
            5 => altitude_ref = value.and_then(|v| v.first().copied()),
            6 => altitude = value.and_then(|bytes| rational(bytes, little)),
            7 => gps_time = rational_triplet(value, little),
            29 => gps_date = text(value),
            _ => {}
        }
    }
    out.latitude =
        latitude.and_then(|value| apply_coordinate_ref(value, latitude_ref.as_deref(), b'S', b'N'));
    out.longitude = longitude
        .and_then(|value| apply_coordinate_ref(value, longitude_ref.as_deref(), b'W', b'E'));
    out.altitude = altitude.map(|value| {
        if altitude_ref == Some(1) {
            -value
        } else {
            value
        }
    });
    if let (Some(date), Some(time)) = (gps_date, gps_time) {
        out.gps_date_time = Some(format!("{date} {time:.3}"));
    }
}

fn apply_coordinate_ref(
    value: f64,
    reference: Option<&str>,
    negative: u8,
    positive: u8,
) -> Option<f64> {
    let reference = reference?.as_bytes().first().copied()?;
    if reference == negative {
        Some(-value)
    } else if reference == positive {
        Some(value)
    } else {
        None
    }
}

fn rational_triplet(value: Option<&[u8]>, little: bool) -> Option<f64> {
    let value = value?;
    let degrees = rational(value, little)?;
    let minutes = rational(value.get(8..)?, little)?;
    let seconds = rational(value.get(16..)?, little)?;
    Some(degrees + minutes / 60.0 + seconds / 3600.0)
}

fn rational(value: &[u8], little: bool) -> Option<f64> {
    let numerator = read_u32(value, 0, little)? as f64;
    let denominator = read_u32(value, 4, little)? as f64;
    (denominator != 0.0).then_some(numerator / denominator)
}

fn value_bytes(bytes: &[u8], inline: usize, typ: u16, count: u32, little: bool) -> Option<&[u8]> {
    let width: usize = match typ {
        1 | 2 | 7 => 1,
        3 => 2,
        4 | 9 => 4,
        5 | 10 => 8,
        _ => return None,
    };
    let size = width.checked_mul(count as usize)?;
    let start = if size <= 4 {
        inline
    } else {
        read_u32(bytes, inline, little)? as usize
    };
    bytes.get(start..start + size.min(4096))
}

fn text(value: Option<&[u8]>) -> Option<String> {
    let value = value?;
    let end = value.iter().position(|b| *b == 0).unwrap_or(value.len());
    let text = String::from_utf8_lossy(&value[..end]).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn read_u16(bytes: &[u8], offset: usize, little: bool) -> Option<u16> {
    let v = bytes.get(offset..offset + 2)?.try_into().ok()?;
    Some(if little {
        u16::from_le_bytes(v)
    } else {
        u16::from_be_bytes(v)
    })
}
fn read_u32(bytes: &[u8], offset: usize, little: bool) -> Option<u32> {
    let v = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(if little {
        u32::from_le_bytes(v)
    } else {
        u32::from_be_bytes(v)
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/image_metadata.rs"]
mod tests;
