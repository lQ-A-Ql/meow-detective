use std::io::Cursor;

use super::{Channels, DecodeError, DecodedHeader, RowSink, RowSource};
use crate::limits::StegLimits;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const MAX_CHUNKS: usize = 4096;
const MAX_APNG_SCAN: usize = 4 * 1024 * 1024;

pub(crate) struct PngSource<'a> {
    reader: png::Reader<Cursor<&'a [u8]>>,
    header: DecodedHeader,
    y: u32,
    plane_bufs: [Vec<u8>; 3],
}

pub(crate) fn open<'a>(bytes: &'a [u8], limits: &StegLimits) -> Result<PngSource<'a>, DecodeError> {
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err(DecodeError::corrupt("image.decode"));
    }
    if has_actl(bytes) {
        return Err(DecodeError::unsupported("png.apng"));
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    decoder.set_limits(png::Limits {
        bytes: 96 * 1024 * 1024,
    });
    let reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(error) => return Err(map_png_error(error)),
    };
    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let dims = (width, height);
    if width == 0 || height == 0 {
        return Err(DecodeError::corrupt("image.decode"));
    }
    if info.interlaced {
        return Err(DecodeError::unsupported_at("png.interlaced", width, height));
    }
    if info.bit_depth != png::BitDepth::Eight {
        return Err(DecodeError::unsupported_at("png.bit-depth", width, height));
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => Channels::Gray,
        png::ColorType::GrayscaleAlpha => Channels::GrayAlpha,
        png::ColorType::Rgb => Channels::Rgb,
        png::ColorType::Rgba => Channels::Rgba,
        png::ColorType::Indexed => Channels::Indexed,
    };
    if channels.plane_count() > limits.max_planes {
        return Err(DecodeError::unsupported_at(
            "png.color-type",
            dims.0,
            dims.1,
        ));
    }
    if let Err(reason) = limits.check_dimensions(width, height) {
        return Err(DecodeError::oversized_at(reason, width, height));
    }
    Ok(PngSource {
        reader,
        header: DecodedHeader {
            width,
            height,
            channels,
        },
        y: 0,
        plane_bufs: std::array::from_fn(|_| Vec::new()),
    })
}

impl RowSource for PngSource<'_> {
    fn header(&self) -> DecodedHeader {
        self.header
    }

    fn next_row(&mut self, sink: &mut dyn RowSink) -> Result<bool, DecodeError> {
        if self.y >= self.header.height {
            return Ok(false);
        }
        let row = match self.reader.next_row() {
            Ok(Some(row)) => row,
            Ok(None) => return Err(DecodeError::corrupt("image.truncated")),
            Err(error) => return Err(map_png_error(error)),
        };
        let width = self.header.width as usize;
        fill_planes(
            row.data(),
            width,
            self.header.channels,
            &mut self.plane_bufs,
            self.header.width,
            self.header.height,
        )?;
        let y = self.y;
        self.y += 1;
        let planes = plane_refs(&self.plane_bufs, self.header.plane_count());
        sink.on_row(y, &planes[..self.header.plane_count()], width);
        Ok(true)
    }
}

fn fill_planes(
    data: &[u8],
    width: usize,
    channels: Channels,
    out: &mut [Vec<u8>; 3],
    image_width: u32,
    image_height: u32,
) -> Result<(), DecodeError> {
    match channels {
        Channels::Gray | Channels::Indexed => {
            let Some(source) = data.get(..width) else {
                return Err(DecodeError::corrupt_at(
                    "image.truncated",
                    image_width,
                    image_height,
                ));
            };
            out[0].clear();
            out[0].extend_from_slice(source);
        }
        Channels::GrayAlpha => {
            let Some(source) = data.get(..width.saturating_mul(2)) else {
                return Err(DecodeError::corrupt_at(
                    "image.truncated",
                    image_width,
                    image_height,
                ));
            };
            let plane = &mut out[0];
            plane.clear();
            plane.extend(source.as_chunks::<2>().0.iter().map(|pair| pair[0]));
        }
        Channels::Rgb => deinterleave(data, width, 3, out, image_width, image_height)?,
        Channels::Rgba => deinterleave(data, width, 4, out, image_width, image_height)?,
    }
    Ok(())
}

fn deinterleave(
    data: &[u8],
    width: usize,
    step: usize,
    out: &mut [Vec<u8>; 3],
    image_width: u32,
    image_height: u32,
) -> Result<(), DecodeError> {
    let needed = width.saturating_mul(step);
    let Some(source) = data.get(..needed) else {
        return Err(DecodeError::corrupt_at(
            "image.truncated",
            image_width,
            image_height,
        ));
    };
    for plane in out.iter_mut().take(3) {
        plane.clear();
    }
    for pixel in source.chunks_exact(step) {
        out[0].push(pixel[0]);
        out[1].push(pixel[1]);
        out[2].push(pixel[2]);
    }
    Ok(())
}

fn plane_refs(bufs: &[Vec<u8>; 3], count: usize) -> [&[u8]; 3] {
    let empty: &[u8] = &[];
    [
        bufs[0].as_slice(),
        if count > 1 { bufs[1].as_slice() } else { empty },
        if count > 2 { bufs[2].as_slice() } else { empty },
    ]
}

/// APNG is recognised by the mandatory `acTL` chunk before the first `IDAT`.
fn has_actl(bytes: &[u8]) -> bool {
    let mut offset = PNG_SIGNATURE.len();
    let limit = bytes.len().min(MAX_APNG_SCAN);
    for _ in 0..MAX_CHUNKS {
        if offset + 8 > limit {
            return false;
        }
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        if kind == b"acTL" {
            return true;
        }
        if kind == b"IDAT" || kind == b"IEND" {
            return false;
        }
        let next = match offset.checked_add(12).and_then(|v| v.checked_add(length)) {
            Some(next) => next,
            None => return false,
        };
        if next <= offset || next > bytes.len() {
            return false;
        }
        offset = next;
        if offset > limit {
            return false;
        }
    }
    false
}

fn map_png_error(error: png::DecodingError) -> DecodeError {
    match error {
        png::DecodingError::LimitsExceeded => DecodeError {
            status: crate::types::StegStatus::Oversized,
            reason: "limit.bytes",
            width: None,
            height: None,
        },
        _ => DecodeError::corrupt("image.decode"),
    }
}
