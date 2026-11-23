use super::{Channels, DecodeError, DecodedHeader, RowSink, RowSource};
use crate::limits::StegLimits;
use tinybmp::{Bpp, CompressionMethod, Header, ParseError, RawBmp};

pub(crate) struct BmpSource<'a> {
    bitmap: RawBmp<'a>,
    header: DecodedHeader,
    y: u32,
    plane_bufs: [Vec<u8>; 3],
}

pub(crate) fn open<'a>(bytes: &'a [u8], limits: &StegLimits) -> Result<BmpSource<'a>, DecodeError> {
    let metadata = Header::from_slice(bytes).map_err(map_error)?;
    let width = metadata.image_size.width;
    let height = metadata.image_size.height;
    if let Err(reason) = limits.check_dimensions(width, height) {
        return Err(DecodeError::oversized_at(reason, width, height));
    }
    if metadata.compression_method != CompressionMethod::Rgb {
        return Err(DecodeError::unsupported_at(
            "bmp.compression",
            width,
            height,
        ));
    }
    let channels = match metadata.bpp {
        Bpp::Bits8 => Channels::Indexed,
        Bpp::Bits24 => Channels::Rgb,
        Bpp::Bits32 => Channels::Rgba,
        _ => return Err(DecodeError::unsupported_at("bmp.bit-depth", width, height)),
    };
    let bitmap = RawBmp::from_slice(bytes).map_err(map_error)?;
    Ok(BmpSource {
        bitmap,
        header: DecodedHeader {
            width,
            height,
            channels,
        },
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
        for plane in &mut self.plane_bufs {
            plane.clear();
        }
        for x in 0..self.header.width {
            let pixel = self
                .bitmap
                .pixel((x as i32, self.y as i32).into())
                .ok_or_else(|| DecodeError::corrupt("bmp.header"))?;
            if self.header.channels == Channels::Indexed {
                self.plane_bufs[0].push(pixel as u8);
            } else {
                self.plane_bufs[0].push((pixel >> 16) as u8);
                self.plane_bufs[1].push((pixel >> 8) as u8);
                self.plane_bufs[2].push(pixel as u8);
            }
        }
        let planes = [
            self.plane_bufs[0].as_slice(),
            self.plane_bufs[1].as_slice(),
            self.plane_bufs[2].as_slice(),
        ];
        sink.on_row(
            self.y,
            &planes[..self.header.plane_count()],
            self.header.width as usize,
        );
        self.y += 1;
        Ok(true)
    }
}

fn map_error(error: ParseError) -> DecodeError {
    match error {
        ParseError::UnsupportedBpp(_) => DecodeError::unsupported("bmp.bit-depth"),
        ParseError::UnsupportedCompressionMethod(_) => DecodeError::unsupported("bmp.compression"),
        ParseError::UnsupportedHeaderLength(_) | ParseError::UnsupportedChannelMasks => {
            DecodeError::unsupported("bmp.header")
        }
        _ => DecodeError::corrupt("bmp.header"),
    }
}
