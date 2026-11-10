//! Decoding compressed extents.
//!
//! Btrfs may store an extent's data compressed, recording which
//! algorithm in the file-extent item. Three are defined, and a reader
//! that means to be useful on real volumes needs all three: zstd is the
//! usual choice on modern installs, zlib the long-standing default, and
//! LZO the one picked when decode speed matters most.
//!
//! # What is compressed, and what the offsets then mean
//!
//! The unit of compression is the whole extent, not the part of it a
//! given reference covers. So for a compressed extent:
//!
//! - `disk_bytenr` addresses the start of the compressed run, and
//!   `disk_num_bytes` is its length **on disk**;
//! - `ram_bytes` is what that run decodes to;
//! - `offset` and `num_bytes` then index into the **decoded** bytes.
//!
//! This is the trap. For an uncompressed extent the driver reads from
//! `disk_bytenr + offset`, because there the two coordinate systems
//! coincide. Doing that to a compressed extent seeks into the middle of
//! a compressed stream, and the whole run has to be decoded first and
//! sliced afterwards instead.
//!
//! # zlib and zstd carry their own framing; LZO does not
//!
//! A zlib or zstd extent is one ordinary stream of that format, so the
//! respective decoder handles it whole.
//!
//! LZO1X has no container — it is a bare instruction stream with no
//! length, no checksum and no end marker that a decoder can find without
//! being told how many bytes it holds. Btrfs therefore supplies its own
//! framing, and that framing is this module's real work. See
//! `decompress_lzo`.
//!
//! # A short decode is padded only on the file's last extent
//!
//! A decoder may stop early on a file's last extent, where the tail of
//! the final sector holds nothing, so [`decompress`] pads a short result
//! to `ram_bytes` there. Anywhere else it refuses: a stream that stops
//! early in an interior extent is truncated or damaged, and zeros are
//! legitimate file content, so padding one would hand a caller plausible
//! bytes with no way to tell. The caller says which case it has through
//! [`ShortDecode`] (#189).

use crate::error::{Error, Result};

/// Compression type as stored in `btrfs_file_extent_item.compression`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Stored as written.
    None,
    /// zlib, i.e. DEFLATE inside a zlib wrapper.
    Zlib,
    /// LZO1X, in the segmented framing described in `decompress_lzo`.
    Lzo,
    /// zstd.
    Zstd,
}

impl Compression {
    /// Interpret the byte in a file-extent item.
    ///
    /// An unrecognised value is refused rather than treated as "none":
    /// reading a compressed extent as though it were plain produces
    /// bytes that look like a successful read of corrupt data, which no
    /// caller can distinguish from the real thing.
    pub fn from_byte(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Compression::None),
            1 => Ok(Compression::Zlib),
            2 => Ok(Compression::Lzo),
            3 => Ok(Compression::Zstd),
            other => Err(Error::UnsupportedFeature(format!(
                "extent uses compression type {other}, which is not one of \
                 zlib (1), LZO (2) or zstd (3)"
            ))),
        }
    }

    /// Whether data stored under this type needs decoding.
    pub fn is_compressed(self) -> bool {
        self != Compression::None
    }
}

/// The most a compressed extent decodes to.
///
/// Btrfs compresses in units of 128 KiB (`BTRFS_MAX_UNCOMPRESSED`) and
/// never writes a larger one. Measured against btrfs-progs 6.x: every
/// compressed extent an 8 MiB file produced has `ram_bytes` of exactly
/// 131072, and none is larger.
pub const MAX_UNCOMPRESSED: usize = 128 * 1024;

/// The most a compressed extent occupies on disk.
///
/// `BTRFS_MAX_COMPRESSED`, the same number for the same reason.
/// Measured: a barely-compressible 128 KiB extent came out at 102400
/// bytes, and the encoder stores the extent uncompressed rather than
/// let it grow past the unit it started from.
pub const MAX_COMPRESSED: u64 = 128 * 1024;

/// Decode `input` into exactly `ram_bytes` bytes.
///
/// `sectorsize` is the filesystem's sector size, which the LZO framing
/// is laid out against and the other two ignore.
///
/// The decoded length is checked rather than trusted. Every caller then
/// slices this by an offset the item supplied, and a short result would
/// otherwise turn a corrupt extent into a panic or a silent hole.
/// What a decode that comes up short of `ram_bytes` means.
///
/// A decoder may legitimately stop early on the file's LAST extent, where
/// the tail of the final sector holds nothing. Anywhere else a short decode
/// is a truncated or damaged stream, and padding it with zeros hands back
/// plausible file content with no error — which is the one decode failure
/// this module used not to report (#189).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortDecode {
    /// The file's last extent: pad to `ram_bytes`.
    Pad,
    /// Any other extent: refuse.
    Refuse,
}

pub fn decompress(
    algo: Compression,
    input: &[u8],
    ram_bytes: usize,
    sectorsize: usize,
    short: ShortDecode,
) -> Result<Vec<u8>> {
    // `ram_bytes` IS THE ALLOCATION, and it comes off the disk.
    //
    // Every branch below sizes its output buffer from it before any
    // input is decoded, so an inline extent of about thirty bytes
    // claiming 2^55 asks for 36 petabytes. That does not fail politely:
    // `handle_alloc_error` aborts, and an abort is not something
    // `capi::guard`'s `catch_unwind` can turn into an EIO -- the host
    // process dies.
    //
    // Btrfs compresses in units of 128 KiB and never writes a larger
    // one. Measured rather than assumed: every compressed extent an
    // 8 MiB file produced under `compress-force=zstd` has `ram_bytes`
    // of exactly 131072, and none is larger.
    if ram_bytes > MAX_UNCOMPRESSED {
        return Err(Error::BadSuperblock(format!(
            "{algo:?} extent says it decodes to {ram_bytes} bytes, more than the \
             {MAX_UNCOMPRESSED} a compressed extent can hold"
        )));
    }
    let out = match algo {
        Compression::None => input.to_vec(),
        Compression::Zlib => decompress_zlib(input, ram_bytes)?,
        Compression::Lzo => decompress_lzo(input, ram_bytes, sectorsize)?,
        Compression::Zstd => decompress_zstd(input, ram_bytes)?,
    };

    // A decoder may legitimately stop early on the last extent of a
    // file, where the tail of the final sector holds nothing. Padding is
    // correct there; coming up short anywhere else is not, and the
    // difference is not visible from here — so pad and let the caller's
    // slice bound the result.
    if out.len() > ram_bytes {
        return Err(Error::BadSuperblock(format!(
            "{algo:?} extent decoded to {} bytes, more than the {ram_bytes} it records",
            out.len()
        )));
    }
    if out.len() < ram_bytes && short == ShortDecode::Refuse {
        return Err(Error::BadSuperblock(format!(
            "{algo:?} extent decoded to {} bytes where it records {ram_bytes}, and it is not \
             the file's last extent, so the rest is missing rather than absent",
            out.len()
        )));
    }
    let mut out = out;
    out.resize(ram_bytes, 0);
    Ok(out)
}

fn decompress_zlib(input: &[u8], ram_bytes: usize) -> Result<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(input, ram_bytes).map_err(|e| {
        Error::BadSuperblock(format!(
            "zlib extent failed to decode ({:?}) after {} bytes",
            e.status,
            e.output.len()
        ))
    })
}

fn decompress_zstd(input: &[u8], ram_bytes: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let decoder = ruzstd::decoding::StreamingDecoder::new(input)
        .map_err(|e| Error::BadSuperblock(format!("zstd extent has no readable frame: {e}")))?;
    // Bounded by what the item says it decodes to, so a corrupt frame
    // claiming an enormous size cannot be used to exhaust memory.
    let mut out = Vec::with_capacity(ram_bytes);
    decoder
        .take(ram_bytes as u64)
        .read_to_end(&mut out)
        .map_err(|e| Error::BadSuperblock(format!("zstd extent failed to decode: {e}")))?;
    Ok(out)
}

/// Length prefix width in the LZO framing.
const LZO_LEN: usize = 4;

/// Decode btrfs's segmented LZO framing.
///
/// LZO1X is a bare instruction stream: it carries no length, no checksum
/// and no terminator a decoder could find on its own. Btrfs supplies the
/// framing itself, and it is arranged so that a reader can decode any
/// one sector without walking the ones before it:
///
/// ```text
/// [ u32 total ][ u32 len ][ segment ][ u32 len ][ segment ] ... padding ...
/// |<--------------- sector ------------------>|<----- next sector ----->|
/// ```
///
/// - The extent opens with a little-endian `u32` giving the total
///   compressed length, counting those four bytes.
/// - Each segment is a little-endian `u32` of its own compressed length,
///   then that many bytes, decoding to at most one sector.
/// - **A segment's header never straddles a sector boundary.** When what
///   remains of the current sector cannot hold another four-byte header,
///   the rest of the sector (at most three bytes) is skipped and the next
///   header begins at the following boundary. The segment's DATA may run
///   across a boundary freely; the kernel's `lzo.c` says the same, and a
///   check that also required the data to fit would misread every extent
///   whose payload spans a sector.
///
/// That last rule is the whole reason this function exists rather than a
/// single call to the LZO decoder, and it is invisible in any extent
/// small enough to fit in one sector — which is most of them, and every
/// one that a hand-written test is likely to build. It is checked here
/// against extents the kernel itself compressed.
fn decompress_lzo(input: &[u8], ram_bytes: usize, sectorsize: usize) -> Result<Vec<u8>> {
    if sectorsize < LZO_LEN {
        return Err(Error::BadSuperblock(format!(
            "sector size {sectorsize} is too small to hold an LZO segment header"
        )));
    }
    if input.len() < LZO_LEN {
        return Err(Error::BadSuperblock(format!(
            "LZO extent is {} bytes, too short to hold its length header",
            input.len()
        )));
    }

    let total = read_u32(input, 0)? as usize;
    if total > input.len() {
        return Err(Error::BadSuperblock(format!(
            "LZO extent declares {total} compressed bytes but only {} are present",
            input.len()
        )));
    }

    let mut out = Vec::with_capacity(ram_bytes);
    let mut at = LZO_LEN;

    while at < total && out.len() < ram_bytes {
        // A header that will not fit in what is left of this sector means
        // the encoder moved on to the next one.
        let room = sectorsize - (at % sectorsize);
        if room < LZO_LEN {
            at += room;
            continue;
        }

        let seg_len = read_u32(input, at)? as usize;
        at += LZO_LEN;

        if seg_len == 0 {
            return Err(Error::BadSuperblock(
                "LZO extent contains a zero-length segment".into(),
            ));
        }
        let end = at.checked_add(seg_len).ok_or_else(|| {
            Error::BadSuperblock("LZO segment length overflows the extent".into())
        })?;
        if end > input.len() {
            return Err(Error::BadSuperblock(format!(
                "LZO segment at {at} runs {seg_len} bytes past the end of a {}-byte extent",
                input.len()
            )));
        }

        // Each segment decodes to at most one sector, and never to more
        // than what is left to produce.
        let want = sectorsize.min(ram_bytes - out.len());
        let chunk = lzo1x::decompress(&input[at..end], want).map_err(|e| {
            Error::BadSuperblock(format!("LZO segment at {at} failed to decode: {e}"))
        })?;
        out.extend_from_slice(&chunk);
        at = end;
    }

    Ok(out)
}

fn read_u32(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
        .ok_or_else(|| {
            Error::BadSuperblock(format!(
                "LZO extent ended at {} with a length prefix expected at {at}",
                b.len()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ram_bytes` is the allocation, and it comes off the disk. Every
    /// branch sizes its output buffer from it before decoding any
    /// input, so an inline extent of about thirty bytes claiming 2^55
    /// asked for 36 petabytes -- and `handle_alloc_error` answers that
    /// by aborting, which `capi::guard`'s `catch_unwind` cannot turn
    /// into an EIO.
    ///
    /// Btrfs compresses in units of 128 KiB. Measured against
    /// btrfs-progs 6.x: every compressed extent an 8 MiB file produced
    /// under `compress-force=zstd` has `ram_bytes` of exactly 131072.
    #[test]
    fn an_extent_decoding_to_more_than_a_compression_unit_is_refused() {
        for algo in [Compression::Zlib, Compression::Lzo, Compression::Zstd] {
            let outcome = decompress(
                algo,
                &[0u8; 4],
                MAX_UNCOMPRESSED + 1,
                4096,
                ShortDecode::Pad,
            );
            let why = format!("{outcome:?}");
            assert!(
                why.contains("more than the"),
                "{algo:?} accepted a declared size of {} and was refused as {why}",
                MAX_UNCOMPRESSED + 1
            );
            // The shape that reached the abort: an inline extent
            // claiming a size no allocator will satisfy.
            assert!(decompress(algo, &[0u8; 4], 1 << 55, 4096, ShortDecode::Pad).is_err());
        }
    }

    #[test]
    fn compression_bytes_map_to_the_defined_algorithms() {
        assert_eq!(Compression::from_byte(0).unwrap(), Compression::None);
        assert_eq!(Compression::from_byte(1).unwrap(), Compression::Zlib);
        assert_eq!(Compression::from_byte(2).unwrap(), Compression::Lzo);
        assert_eq!(Compression::from_byte(3).unwrap(), Compression::Zstd);
    }

    /// An unknown type must not fall back to "stored as written": that
    /// would hand the caller compressed bytes labelled as file contents.
    #[test]
    fn an_unknown_compression_type_is_refused() {
        let err = Compression::from_byte(4).unwrap_err();
        assert!(format!("{err}").contains("compression type 4"), "got {err}");
    }

    #[test]
    fn none_passes_bytes_through() {
        let got = decompress(Compression::None, b"hello", 5, 4096, ShortDecode::Pad).unwrap();
        assert_eq!(got, b"hello");
    }

    #[test]
    fn zlib_round_trips() {
        let plain: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&plain, 6);
        let got = decompress(
            Compression::Zlib,
            &packed,
            plain.len(),
            4096,
            ShortDecode::Pad,
        )
        .unwrap();
        assert_eq!(got, plain);
    }

    /// The decoded length is checked, not taken on trust: a stream that
    /// expands past what the item recorded means the two disagree, and
    /// the caller is about to slice by an offset derived from the item.
    #[test]
    fn a_stream_longer_than_the_item_records_is_refused() {
        let plain = vec![7u8; 5000];
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&plain, 6);
        // miniz stops at the limit, so this surfaces as a decode failure
        // rather than an over-long buffer; either way it must not pass.
        assert!(decompress(Compression::Zlib, &packed, 100, 4096, ShortDecode::Pad).is_err());
    }

    /// A short decode is padded rather than refused — the tail of a
    /// file's last sector legitimately holds nothing.
    #[test]
    fn a_short_decode_is_zero_padded_to_the_recorded_length() {
        let plain = b"abc".to_vec();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&plain, 6);
        let got = decompress(Compression::Zlib, &packed, 16, 4096, ShortDecode::Pad).unwrap();
        assert_eq!(&got[..3], b"abc");
        assert_eq!(&got[3..], &[0u8; 13]);
    }

    /// Build btrfs's LZO framing around already-compressed segments, so
    /// the padding rule can be exercised without an LZO compressor.
    fn lzo_frame(segments: &[Vec<u8>], sectorsize: usize) -> Vec<u8> {
        let mut out = vec![0u8; LZO_LEN];
        for seg in segments {
            if sectorsize - (out.len() % sectorsize) < LZO_LEN {
                let pad = sectorsize - (out.len() % sectorsize);
                out.resize(out.len() + pad, 0);
            }
            out.extend_from_slice(&(seg.len() as u32).to_le_bytes());
            out.extend_from_slice(seg);
        }
        let total = out.len() as u32;
        out[0..LZO_LEN].copy_from_slice(&total.to_le_bytes());
        out
    }

    #[test]
    fn lzo_declaring_more_than_it_holds_is_refused() {
        let mut framed = vec![0u8; 8];
        framed[0..4].copy_from_slice(&999u32.to_le_bytes());
        let err = decompress(Compression::Lzo, &framed, 4096, 4096, ShortDecode::Pad).unwrap_err();
        assert!(format!("{err}").contains("only 8 are present"), "got {err}");
    }

    #[test]
    fn lzo_shorter_than_its_header_is_refused() {
        let err = decompress(Compression::Lzo, &[1, 2], 4096, 4096, ShortDecode::Pad).unwrap_err();
        assert!(format!("{err}").contains("too short"), "got {err}");
    }

    #[test]
    fn lzo_with_a_zero_length_segment_is_refused() {
        let framed = lzo_frame(&[vec![]], 4096);
        let err = decompress(Compression::Lzo, &framed, 4096, 4096, ShortDecode::Pad).unwrap_err();
        assert!(format!("{err}").contains("zero-length"), "got {err}");
    }

    /// A segment claiming to run past the buffer must be caught before
    /// it is sliced.
    #[test]
    fn lzo_with_a_segment_past_the_end_is_refused() {
        let mut framed = vec![0u8; 12];
        framed[0..4].copy_from_slice(&12u32.to_le_bytes());
        framed[4..8].copy_from_slice(&9999u32.to_le_bytes());
        let err = decompress(Compression::Lzo, &framed, 4096, 4096, ShortDecode::Pad).unwrap_err();
        assert!(format!("{err}").contains("past the end"), "got {err}");
    }

    /// An LZO1X stream of `n` literals (4..=238) and nothing else: the
    /// first byte `17 + n` announces the run, and `0x11 0x00 0x00` is the
    /// end-of-stream marker. Enough to exercise the framing through the
    /// real decoder without an LZO compressor.
    fn lzo_literals(bytes: &[u8]) -> Vec<u8> {
        assert!((4..=238).contains(&bytes.len()));
        let mut out = vec![17 + bytes.len() as u8];
        out.extend_from_slice(bytes);
        out.extend_from_slice(&[0x11, 0, 0]);
        out
    }

    /// A segment's HEADER never straddles a sector; its PAYLOAD may, and is
    /// read straight across the boundary (#91).
    ///
    /// The kernel's `lzo.c`: the header does not cross a sector, "thus it's
    /// possible to have at most 3 padding zeros at the end of the sector",
    /// and nothing more. The doc once said header AND data, which reads as
    /// if this decoder's header-only check were too weak.
    ///
    /// With 64-byte sectors: the extent header and the first segment's
    /// header take 8 bytes, a 48-byte first segment ends at 56, the second
    /// header fits in 56..60, and the second segment's 14 bytes run from 60
    /// across the boundary at 64.
    #[test]
    fn an_lzo_payload_crosses_a_sector_boundary_and_is_read_through() {
        let sectorsize = 64;
        let a: Vec<u8> = (0..44u8).collect();
        let b: Vec<u8> = (100..110u8).collect();
        let first = lzo_literals(&a);
        let second = lzo_literals(&b);
        assert_eq!(first.len(), 48);
        let framed = lzo_frame(&[first, second], sectorsize);
        assert_eq!(read_u32(&framed, 56).unwrap(), 14, "fixture: header at 56");

        let got = decompress(
            Compression::Lzo,
            &framed,
            a.len() + b.len(),
            sectorsize,
            ShortDecode::Pad,
        )
        .expect("a payload crossing a sector is legal");
        assert_eq!(&got[..a.len()], &a[..]);
        assert_eq!(&got[a.len()..], &b[..]);
    }

    /// The layout a straddling header produces, written down.
    ///
    /// # What this does NOT do
    ///
    /// It does not exercise the decoder. `lzo_frame` is the test
    /// module's own implementation of the padding rule, so this builds a
    /// frame with that rule and then asserts the rule was applied —
    /// `decompress` is never called, and the production branch that
    /// skips a straddling header can be deleted without this going red.
    ///
    /// It is kept because the layout is worth having written down
    /// somewhere executable, and removed nothing: what actually covers
    /// the decoder here is `tests/compression_oracle.rs`, against LZO
    /// streams a real encoder produced. That suite needs fixtures, and
    /// until now it was not named in the CI gate that builds them — so
    /// it had never run an assertion on a pull request. It is named
    /// there now.
    #[test]
    fn lzo_segment_headers_do_not_straddle_a_sector() {
        let sectorsize = 64;
        // Sized so the first segment ends 2 bytes short of the sector —
        // too few for the next header, which must therefore move on.
        let first = vec![0xAAu8; sectorsize - LZO_LEN - LZO_LEN - 2];
        let second = vec![0xBBu8; 8];
        let framed = lzo_frame(&[first.clone(), second.clone()], sectorsize);

        // The second segment must begin at the sector boundary.
        assert_eq!(
            read_u32(&framed, sectorsize).unwrap() as usize,
            second.len()
        );
        assert_eq!(
            &framed[sectorsize + LZO_LEN..sectorsize + LZO_LEN + 8],
            &second[..]
        );
    }
}
