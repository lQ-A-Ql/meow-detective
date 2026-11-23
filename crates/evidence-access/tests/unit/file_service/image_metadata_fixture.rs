//! EXIF and PNG fixtures encoded with the native libraries.

use exif::{experimental::Writer, Field, In, Tag, Value};

pub(super) fn tiff(little: bool) -> Vec<u8> {
    let fields = [
        (Tag::Orientation, Value::Short(vec![6])),
        (Tag::DateTimeOriginal, ascii(b"2026:10:14 12:34:56")),
        (Tag::DateTimeDigitized, ascii(b"2026:10:14 12:34:57")),
        (Tag::LensModel, ascii(b"Lens 50")),
        (Tag::GPSLatitudeRef, ascii(b"S")),
        (Tag::GPSLatitude, rational(&[(1, 1), (30, 1), (0, 1)])),
        (Tag::GPSLongitudeRef, ascii(b"W")),
        (Tag::GPSLongitude, rational(&[(2, 1), (0, 1), (0, 1)])),
        (Tag::GPSAltitudeRef, Value::Byte(vec![1])),
        (Tag::GPSAltitude, rational(&[(10, 1)])),
        (Tag::GPSTimeStamp, rational(&[(12, 1), (34, 1), (113, 2)])),
        (Tag::GPSDateStamp, ascii(b"2026:10:14")),
    ]
    .into_iter()
    .map(|(tag, value)| Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    })
    .collect::<Vec<_>>();
    let mut writer = Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut output = std::io::Cursor::new(Vec::new());
    writer.write(&mut output, little).unwrap();
    let mut bytes = output.into_inner();
    bytes.extend_from_slice(&[0; 37]);
    bytes
}

fn ascii(value: &[u8]) -> Value {
    Value::Ascii(vec![value.to_vec()])
}

fn rational(values: &[(u32, u32)]) -> Value {
    Value::Rational(values.iter().copied().map(Into::into).collect())
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
    use image_inspection::png_codec::{chunk, BitDepth, ColorType, Encoder};
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, 2, 3);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        if let Some(exif) = exif {
            writer.write_chunk(chunk::eXIf, exif).unwrap();
        }
        writer.write_image_data(&[0u8; 18]).unwrap();
        writer.finish().unwrap();
    }
    bytes
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
