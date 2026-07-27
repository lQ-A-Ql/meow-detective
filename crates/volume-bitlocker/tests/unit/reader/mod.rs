#[path = "harness.rs"]
mod harness;

use std::io::{Read, Seek, SeekFrom};

use harness::{
    plaintext_pattern, CountingCursor, Harness, HEADER_SIZE, IMAGE_LEN, META_OFFSET, META_SIZE,
    RELOCATED_OFFSET,
};

use crate::reader::BitLockerReader;

#[test]
fn a_normal_sector_decrypts_to_its_plaintext() {
    let mut reader = Harness::standard().reader();
    let mut got = [0u8; 512];
    reader.read_at(0x5000, &mut got).expect("read succeeds");
    assert_eq!(got, plaintext_pattern(0x5000));
}

#[test]
fn the_relocated_header_decrypts_at_its_physical_offset() {
    // The bytes for logical 0 are stored at RELOCATED_OFFSET and were encrypted
    // there, so the reader has to use the physical offset as the cipher address.
    // Using the logical one yields 512 plausible bytes and no error.
    let mut reader = Harness::standard().reader();
    let mut got = [0u8; 512];
    reader.read_at(0, &mut got).expect("read succeeds");
    assert_eq!(got, plaintext_pattern(RELOCATED_OFFSET));
}

#[test]
fn the_whole_relocated_region_maps_sector_by_sector() {
    let mut reader = Harness::standard().reader();
    for sector in 0..(HEADER_SIZE / 512) {
        let logical = sector * 512;
        let mut got = [0u8; 512];
        reader.read_at(logical, &mut got).expect("read succeeds");
        assert_eq!(
            got,
            plaintext_pattern(RELOCATED_OFFSET + logical),
            "logical sector {sector}"
        );
    }
}

#[test]
fn metadata_regions_read_back_as_zeros() {
    let mut reader = Harness::standard().reader();
    let mut got = [0xFFu8; 512];
    reader
        .read_at(META_OFFSET, &mut got)
        .expect("read succeeds");
    assert_eq!(got, [0u8; 512], "the FVE block must not surface as content");
}

#[test]
fn a_read_spanning_a_metadata_block_zeros_only_that_part() {
    // A caller reading across the boundary must get real content either side and
    // zeros in the middle, not a short read or an error.
    let mut reader = Harness::standard().reader();
    let start = META_OFFSET - 512;
    let mut got = vec![0u8; (META_SIZE + 1024) as usize];
    reader.read_at(start, &mut got).expect("read succeeds");

    assert_eq!(&got[..512], &plaintext_pattern(start)[..]);
    assert!(
        got[512..512 + META_SIZE as usize].iter().all(|b| *b == 0),
        "the metadata span must be zeroed"
    );
    let after = META_OFFSET + META_SIZE;
    assert_eq!(
        &got[512 + META_SIZE as usize..],
        &plaintext_pattern(after)[..]
    );
}

#[test]
fn a_partially_encrypted_tail_is_returned_verbatim() {
    // Past encrypted_volume_size the bytes are already plaintext on disk. Running
    // them through the cipher would corrupt readable data.
    let mut reader = Harness::partially_encrypted().reader();
    let mut got = [0u8; 512];
    reader.read_at(0x8000, &mut got).expect("read succeeds");
    assert_eq!(
        got,
        plaintext_pattern(0x8000),
        "the plaintext tail must not be decrypted"
    );
}

#[test]
fn reads_can_start_and_end_mid_sector() {
    let mut reader = Harness::standard().reader();
    let expected = plaintext_pattern(0x5000);
    let mut got = [0u8; 100];
    reader
        .read_at(0x5000 + 40, &mut got)
        .expect("read succeeds");
    assert_eq!(got, expected[40..140]);
}

#[test]
fn a_read_spanning_many_sectors_is_contiguous() {
    let mut reader = Harness::standard().reader();
    let mut got = vec![0u8; 4096];
    reader.read_at(0x5000, &mut got).expect("read succeeds");
    for sector in 0..8usize {
        let span = sector * 512..(sector + 1) * 512;
        assert_eq!(
            &got[span],
            &plaintext_pattern(0x5000 + (sector as u64) * 512)[..],
            "sector {sector} of the span"
        );
    }
}

#[test]
fn reading_past_the_end_yields_zeros() {
    let mut reader = Harness::standard().reader();
    let mut got = [0xFFu8; 512];
    reader.read_at(IMAGE_LEN, &mut got).expect("read succeeds");
    assert_eq!(got, [0u8; 512]);
}

#[test]
fn an_absent_sector_is_not_run_through_the_cipher() {
    // Decrypting a zero-filled buffer for a sector that is not on the image would
    // return 512 bytes of plausible-looking garbage. On an evidence path that is
    // worse than an error, because nothing reports it as absent.
    let mut reader = Harness::standard().reader();
    for past_end in [IMAGE_LEN, IMAGE_LEN + 512, IMAGE_LEN * 2] {
        let mut got = [0xFFu8; 512];
        reader.read_at(past_end, &mut got).expect("read succeeds");
        assert_eq!(got, [0u8; 512], "at {past_end:#x}");
    }
}

#[test]
fn a_read_straddling_the_end_returns_content_then_zeros() {
    let mut reader = Harness::standard().reader();
    let mut got = vec![0xFFu8; 1024];
    reader
        .read_at(IMAGE_LEN - 512, &mut got)
        .expect("read succeeds");
    assert_eq!(&got[..512], &plaintext_pattern(IMAGE_LEN - 512)[..]);
    assert!(
        got[512..].iter().all(|byte| *byte == 0),
        "the span past the image must be zeros, not decrypted padding"
    );
}

#[test]
fn an_offset_that_overflows_is_rejected() {
    let mut reader = Harness::standard().reader();
    let mut got = [0u8; 512];
    let error = match reader.read_at(u64::MAX - 8, &mut got) {
        Ok(()) => panic!("an unaddressable span must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "BITLOCKER_OUT_OF_BOUNDS");
}

#[test]
fn repeated_reads_of_the_same_range_hit_the_cache() {
    // The acceptance criterion is that a repeated range read does no fresh key
    // work. Key derivation already ran once at unlock, so what remains to prove is
    // that the second read does not touch the evidence handle at all.
    let harness = Harness::standard();
    let (mut reader, counters) = harness.counting_reader();
    let mut got = [0u8; 512];

    reader.read_at(0x5000, &mut got).expect("first read");
    let after_first = counters.reads();
    assert!(after_first > 0, "the first read must reach the image");

    for _ in 0..16 {
        reader.read_at(0x5000, &mut got).expect("repeat read");
    }
    assert_eq!(
        counters.reads(),
        after_first,
        "repeated reads of a cached sector must not touch the image"
    );
}

#[test]
fn a_sequential_read_coalesces_into_few_image_reads() {
    // Without coalescing a 64 KiB read costs 128 seek+read pairs. The run loader
    // should collapse a contiguous span into one.
    let harness = Harness::standard();
    let (mut reader, counters) = harness.counting_reader();
    let mut got = vec![0u8; 64 * 1024];
    reader.read_at(0x10000, &mut got).expect("read succeeds");

    let seeks = counters.seeks();
    assert!(
        seeks <= 4,
        "a contiguous 64 KiB read issued {seeks} seeks; coalescing is not working"
    );
    // Content still has to be right, not merely cheap.
    assert_eq!(&got[..512], &plaintext_pattern(0x10000)[..]);
    let last = got.len() - 512;
    assert_eq!(&got[last..], &plaintext_pattern(0x10000 + last as u64)[..]);
}

#[test]
fn coalescing_stops_at_a_metadata_block() {
    // A run must not fold a blanked region into a neighbouring read, or the
    // metadata bytes would be decrypted and returned as content.
    let mut reader = Harness::standard().reader();
    let start = META_OFFSET - 1024;
    let mut got = vec![0u8; 1024 + META_SIZE as usize];
    reader.read_at(start, &mut got).expect("read succeeds");
    assert_eq!(&got[..512], &plaintext_pattern(start)[..]);
    assert!(got[1024..].iter().all(|byte| *byte == 0));
}

#[test]
fn coalescing_stops_at_the_relocation_boundary() {
    // Logical 0x1E00 lives at RELOCATED_OFFSET + 0x1E00 while 0x2000 lives at
    // 0x2000, so the physical run is not contiguous across the boundary.
    let mut reader = Harness::standard().reader();
    let mut got = vec![0u8; 1024];
    reader
        .read_at(HEADER_SIZE - 512, &mut got)
        .expect("read succeeds");
    assert_eq!(
        &got[..512],
        &plaintext_pattern(RELOCATED_OFFSET + HEADER_SIZE - 512)[..]
    );
    assert_eq!(&got[512..], &plaintext_pattern(HEADER_SIZE)[..]);
}

#[test]
fn the_cache_is_bounded_and_evicts_by_slot() {
    // The cache is direct-mapped over 256 slots, so sweeping far more than that
    // must stay correct rather than returning a stale slot's bytes.
    let mut reader = Harness::standard().reader();
    for sector in 0..600u64 {
        let logical = 0x10000 + sector * 512;
        let mut got = [0u8; 512];
        reader.read_at(logical, &mut got).expect("read succeeds");
        assert_eq!(got, plaintext_pattern(logical), "sector {sector}");
    }
    // Re-read the first one, long since evicted, and it must still be right.
    let mut got = [0u8; 512];
    reader.read_at(0x10000, &mut got).expect("read succeeds");
    assert_eq!(got, plaintext_pattern(0x10000));
}

#[test]
fn read_advances_the_position_and_stops_at_the_end() {
    let mut reader = Harness::standard().reader();
    reader
        .seek(SeekFrom::Start(IMAGE_LEN - 512))
        .expect("seek succeeds");
    let mut buffer = [0u8; 1024];
    let count = reader.read(&mut buffer).expect("read succeeds");
    assert_eq!(count, 512, "a read must stop at the end of the volume");
    assert_eq!(reader.read(&mut buffer).expect("read at eof"), 0);
}

#[test]
fn read_matches_read_at() {
    let mut reader = Harness::standard().reader();
    reader.seek(SeekFrom::Start(0x5000)).expect("seek succeeds");
    let mut streamed = [0u8; 512];
    reader.read_exact(&mut streamed).expect("read succeeds");
    assert_eq!(streamed, plaintext_pattern(0x5000));
}

#[test]
fn seek_variants_agree() {
    let mut reader = Harness::standard().reader();
    assert_eq!(reader.seek(SeekFrom::Start(1024)).expect("start"), 1024);
    assert_eq!(reader.seek(SeekFrom::Current(512)).expect("current"), 1536);
    assert_eq!(reader.seek(SeekFrom::Current(-512)).expect("back"), 1024);
    assert_eq!(
        reader.seek(SeekFrom::End(0)).expect("end"),
        IMAGE_LEN,
        "seeking to the end must report the volume length"
    );
}

#[test]
fn seek_before_the_start_errors_rather_than_clamping() {
    // Clamping would hide a caller's offset arithmetic bug on an evidence path,
    // the same reasoning as evidence_core::RawImageReader.
    let mut reader = Harness::standard().reader();
    assert!(reader.seek(SeekFrom::Start(0)).is_ok());
    assert!(reader.seek(SeekFrom::Current(-1)).is_err());
    assert!(reader.seek(SeekFrom::End(-(IMAGE_LEN as i64) - 1)).is_err());
}

#[test]
fn seeking_past_the_end_is_legal_and_reads_nothing() {
    let mut reader = Harness::standard().reader();
    reader
        .seek(SeekFrom::Start(IMAGE_LEN * 4))
        .expect("seek past end is legal");
    let mut buffer = [0u8; 64];
    assert_eq!(reader.read(&mut buffer).expect("read past end"), 0);
}

#[test]
fn the_reported_length_matches_the_image() {
    let reader = Harness::standard().reader();
    assert_eq!(reader.len(), IMAGE_LEN);
    assert!(!reader.is_empty());
}

#[test]
fn an_empty_image_presents_no_bytes() {
    let harness = Harness::standard();
    let reader = BitLockerReader::new(harness.volume(), CountingCursor::new(Vec::new()))
        .expect("an empty image still opens");
    assert!(reader.is_empty());
}

#[test]
fn many_readers_share_one_unlocked_volume() {
    // Per-read readers are the design: each owns its handle, position, and cache
    // while the cipher and layout are shared, so a second reader costs no key work.
    let harness = Harness::standard();
    let volume = harness.volume();
    let mut first = BitLockerReader::new(volume.clone(), harness.cursor()).expect("first reader");
    let mut second = BitLockerReader::new(volume, harness.cursor()).expect("second reader");

    first.seek(SeekFrom::Start(0x5000)).expect("seek first");
    let mut from_first = [0u8; 512];
    first.read_exact(&mut from_first).expect("read first");

    let mut from_second = [0u8; 512];
    second
        .read_at(0x5000, &mut from_second)
        .expect("read second");

    assert_eq!(from_first, from_second);
    assert_eq!(
        second.stream_position().expect("position"),
        0,
        "read_at must not move the stream position"
    );
}

#[test]
fn an_unsupported_method_cannot_produce_a_reader() {
    let error = match Harness::unsupported_method().try_volume() {
        Ok(_) => panic!("0x8001 must not yield an unlocked volume"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "BITLOCKER_UNSUPPORTED_METHOD");
}

#[test]
fn the_layout_is_reachable_from_the_unlocked_volume() {
    let harness = Harness::standard();
    let volume = harness.volume();
    assert!(volume.layout().is_encrypted());
    assert_eq!(volume.layout().volume_header_size(), HEADER_SIZE);
}
