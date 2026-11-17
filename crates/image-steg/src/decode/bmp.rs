use super::{
    decode_u16_le, decode_u32_le, Channels, DecodeError, DecodedHeader, RowSink, RowSource,
};
use crate::limits::StegLimits;

const FILE_HEADER_BYTES: usize = 14;
const INFO_HEADER_MIN_BYTES: usize = 40;
const MIN_BMP_BYTES: usize = 54;
const MAX_PALETTE_COLORS: u32 = 256;

pub(crate) struct BmpSource<'a> {
    bytes: &'a [u8],
    header: DecodedHeader,
    data_offset: usize,
    row_stride: usize,
    top_down: bool,
    bytes_per_pixel: usize,
    y: u32,
    plane_bufs: [Vec<u8>; 3],
}

pub(crate) fn open<'a>(bytes: &'a [u8], limits: &StegLimits) -> Result<BmpSource<'a>, DecodeError> {
    if bytes.len() < MIN_BMP_BYTES || &bytes[..2] != b"BM" {
        return Err(DecodeError::corrupt("bmp.header"));
    }
    let data_offset = read_u32(bytes, 10)? as usize;
    let dib_size = read_u32(bytes, 14)? as usize;
    if dib_size < INFO_HEADER_MIN_BYTES {
        return Err(DecodeError::unsupported("bmp.header"));
    }
    if dib_size > bytes.len().saturating_sub(FILE_HEADER_BYTES) {
        return Err(DecodeError::corrupt("bmp.header"));
    }
    let width_i = read_u32(bytes, 18)? as i32;
    let height_i = read_u32(bytes, 22)? as i32;
    if width_i <= 0 || height_i == 0 {
        return Err(DecodeError::corrupt("bmp.header"));
    }
    let width = width_i as u32;
    let top_down = height_i < 0;
    let height = height_i.unsigned_abs();
    let planes = read_u16(bytes, 26)?;
    if planes != 1 {
        return Err(DecodeError::corrupt_at("bmp.header", width, height));
    }
    let bit_count = read_u16(bytes, 28)?;
    let compression = read_u32(bytes, 30)?;
    if compression != 0 {
        return Err(DecodeError::unsupported_at(
            "bmp.compression",
            width,
            height,
        ));
    }
    let (channels, bytes_per_pixel) = match bit_count {
        8 => (Channels::Indexed, 1usize),
        24 => (Channels::Rgb, 3usize),
        32 => (Channels::Rgba, 4usize),
        _ => return Err(DecodeError::unsupported_at("bmp.bit-depth", width, height)),
    };
    if let Err(reason) = limits.check_dimensions(width, height) {
        return Err(DecodeError::oversized_at(reason, width, height));
    }
    let row_bytes = u64::from(width) * bytes_per_pixel as u64;
    let row_stride = (row_bytes + 3) & !3;
    let palette_end = palette_end(bytes, dib_size, bit_count, width, height)?;
    if (data_offset as u64) < palette_end {
        return Err(DecodeError::corrupt_at("bmp.header", width, height));
    }
    let needed = (data_offset as u64)
        .checked_add(row_stride.saturating_mul(u64::from(height)))
        .ok_or_else(|| DecodeError::corrupt_at("bmp.header", width, height))?;
    if needed > bytes.len() as u64 {
        return Err(DecodeError::corrupt_at("bmp.header", width, height));
    }
    Ok(BmpSource {
        bytes,
        header: DecodedHeader {
            width,
            height,
            channels,
        },
        data_offset,
        row_stride: row_stride as usize,
        top_down,
        bytes_per_pixel,
        y: 0,
        plane_bufs: std::array::from_fn(|_| Vec::new()),
    })
}

impl RowSource for BmpSource<'_> {
    fn header(&self) -> DecodedHeader {
        self.header
    }

    fn next_row(&mut self, sink: &mut dyn RowSink) -> Result<bool, DecodeError> {
        if self.y >= self.header.height {
            return Ok(false);
        }
        let src_y = if self.top_down {
            self.y
        } else {
            self.header.height - 1 - self.y
        } as usize;
        let width = self.header.width as usize;
        let start = self.data_offset + src_y * self.row_stride;
        let end = start + width * self.bytes_per_pixel;
        let Some(row) = self.bytes.get(start..end) else {
            return Err(DecodeError::corrupt_at(
                "bmp.header",
                self.header.width,
                self.header.height,
            ));
        };
        match self.header.channels {
            Channels::Indexed => {
                self.plane_bufs[0].clear();
                self.plane_bufs[0].extend_from_slice(row);
            }
            Channels::Rgb | Channels::Rgba => {
                for plane in self.plane_bufs.iter_mut() {
                    plane.clear();
                }
                for pixel in row.chunks_exact(self.bytes_per_pixel) {
                    self.plane_bufs[2].push(pixel[0]);
                    self.plane_bufs[1].push(pixel[1]);
                    self.plane_bufs[0].push(pixel[2]);
                }
            }
            Channels::Gray | Channels::GrayAlpha => {
                return Err(DecodeError::corrupt("image.decode"));
            }
        }
        let y = self.y;
        self.y += 1;
        let planes = plane_refs(&self.plane_bufs, self.header.plane_count());
        sink.on_row(y, &planes[..self.header.plane_count()], width);
        Ok(true)
    }
}

fn palette_end(
    bytes: &[u8],
    dib_size: usize,
    bit_count: u16,
    width: u32,
    height: u32,
) -> Result<u64, DecodeError> {
    if bit_count != 8 {
        return Ok((FILE_HEADER_BYTES + dib_size) as u64);
    }
    let colors = read_u32(bytes, 46).unwrap_or(0);
    let colors = if colors == 0 {
        MAX_PALETTE_COLORS
    } else {
        colors
    };
    if colors > MAX_PALETTE_COLORS {
        return Err(DecodeError::corrupt_at("bmp.header", width, height));
    }
    Ok((FILE_HEADER_BYTES + dib_size) as u64 + u64::from(colors) * 4)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, DecodeError> {
    decode_u16_le(bytes, offset).ok_or_else(|| DecodeError::corrupt("bmp.header"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, DecodeError> {
    decode_u32_le(bytes, offset).ok_or_else(|| DecodeError::corrupt("bmp.header"))
}

fn plane_refs(bufs: &[Vec<u8>; 3], count: usize) -> [&[u8]; 3] {
    let empty: &[u8] = &[];
    [
        bufs[0].as_slice(),
        if count > 1 { bufs[1].as_slice() } else { empty },
        if count > 2 { bufs[2].as_slice() } else { empty },
    ]
}
