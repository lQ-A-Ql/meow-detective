use super::*;
use crate::metadata::SegmentArea;

fn build_minimal_metadata_text() -> String {
    let mut s = String::new();
    s.push_str("contents = \"Text Format Volume Group\"\n");
    s.push_str("version = 1\n");
    s.push('\n');
    s.push_str("test_vg {\n");
    s.push_str("    id = \"vg-uuid-1234-5678-90ab-cdef\"\n");
    s.push_str("    seqno = 42\n");
    s.push_str("    extent_size = 8192\n");
    s.push('\n');
    s.push_str("    physical_volumes {\n");
    s.push_str("        pv0 {\n");
    s.push_str("            id = \"pv-uuid-1234-5678-90ab-cdef\"\n");
    s.push_str("            device = \"/dev/sda1\"\n");
    s.push_str("            pe_start = 2048\n");
    s.push_str("            pe_count = 2559\n");
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push('\n');
    s.push_str("    logical_volumes {\n");
    s.push_str("        root {\n");
    s.push_str("            id = \"lv-root-uuid-1234-5678\"\n");
    s.push_str("            segment_count = 1\n");
    s.push_str("            segment1 {\n");
    s.push_str("                start_extent = 0\n");
    s.push_str("                extent_count = 1280\n");
    s.push_str("                type = \"striped\"\n");
    s.push_str("                stripe_count = 1\n");
    s.push_str("                stripes = [\"pv0\", 0]\n");
    s.push_str("            }\n");
    s.push_str("        }\n");
    s.push_str("        home {\n");
    s.push_str("            id = \"lv-home-uuid-1234-5678\"\n");
    s.push_str("            segment_count = 1\n");
    s.push_str("            segment1 {\n");
    s.push_str("                start_extent = 0\n");
    s.push_str("                extent_count = 512\n");
    s.push_str("                type = \"striped\"\n");
    s.push_str("                stripe_count = 1\n");
    s.push_str("                stripes = [\"pv0\", 1280]\n");
    s.push_str("            }\n");
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str("}\n");
    s
}

fn base_metadata_with_lv(lv_body: &str) -> String {
    format!(
        "test_vg {{\n\
         id = \"vg-test\"\n\
         seqno = 1\n\
         extent_size = 8\n\
         physical_volumes {{\n\
         pv0 {{ id = \"pv0\" pe_start = 0 pe_count = 100 }}\n\
         pv1 {{ id = \"pv1\" pe_start = 0 pe_count = 100 }}\n\
         }}\n\
         logical_volumes {{\n\
         {}\n\
         }}\n\
         }}\n",
        lv_body
    )
}

fn write_mda_header(
    disk: &mut [u8],
    mda_offset: usize,
    mda_size: u64,
    raw_offset: u64,
    text_size: u64,
    text_crc: u32,
) {
    let mda = &mut disk[mda_offset..mda_offset + 512];
    mda[4..20].copy_from_slice(b" LVM2 x[5A%r0N*>");
    mda[20..24].copy_from_slice(&1u32.to_le_bytes());
    mda[24..32].copy_from_slice(&(mda_offset as u64).to_le_bytes());
    mda[32..40].copy_from_slice(&mda_size.to_le_bytes());
    mda[40..48].copy_from_slice(&raw_offset.to_le_bytes());
    mda[48..56].copy_from_slice(&text_size.to_le_bytes());
    mda[56..60].copy_from_slice(&text_crc.to_le_bytes());
    let mda_crc = crc::lvm_crc32(&mda[4..512]);
    mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
}

fn write_raw_location(
    disk: &mut [u8],
    mda_offset: usize,
    slot_index: usize,
    raw_offset: u64,
    text_size: u64,
    text_crc: u32,
) {
    let mda = &mut disk[mda_offset..mda_offset + 512];
    let rl_base = 40 + slot_index * 24;
    mda[rl_base..rl_base + 8].copy_from_slice(&raw_offset.to_le_bytes());
    mda[rl_base + 8..rl_base + 16].copy_from_slice(&text_size.to_le_bytes());
    mda[rl_base + 16..rl_base + 20].copy_from_slice(&text_crc.to_le_bytes());
    mda[rl_base + 20..rl_base + 24].copy_from_slice(&0u32.to_le_bytes());
    let mda_crc = crc::lvm_crc32(&mda[4..512]);
    mda[0..4].copy_from_slice(&mda_crc.to_le_bytes());
}

fn write_circular_bytes(
    disk: &mut [u8],
    mda_offset: usize,
    mda_size: u64,
    raw_offset: u64,
    bytes: &[u8],
) {
    let mda_end = mda_offset + mda_size as usize;
    let first_len = bytes
        .len()
        .min(mda_end - (mda_offset + raw_offset as usize));
    disk[mda_offset + raw_offset as usize..mda_offset + raw_offset as usize + first_len]
        .copy_from_slice(&bytes[..first_len]);
    if first_len < bytes.len() {
        let wrapped_len = bytes.len() - first_len;
        disk[mda_offset + 512..mda_offset + 512 + wrapped_len].copy_from_slice(&bytes[first_len..]);
    }
}

fn metadata_text_with_seqno(seqno: u64) -> String {
    build_minimal_metadata_text().replace("seqno = 42", &format!("seqno = {}", seqno))
}

#[test]
fn parse_minimal_metadata() {
    let text = build_minimal_metadata_text();
    let vg = parse_metadata_text(&text).unwrap();

    assert_eq!(vg.name, "test_vg");
    assert_eq!(vg.extent_size, 8192);
    assert_eq!(vg.seqno, 42);
    assert_eq!(vg.physical_volumes.len(), 1);
    assert_eq!(vg.logical_volumes.len(), 2);

    let root = &vg.logical_volumes[0];
    assert_eq!(root.name, "root");
    assert_eq!(root.segments.len(), 1);
    assert_eq!(root.segments[0].extent_count, 1280);
    assert!(matches!(root.segments[0].seg_type, SegmentType::Linear));

    let home = &vg.logical_volumes[1];
    assert_eq!(home.name, "home");
    assert_eq!(home.segments.len(), 1);
    assert_eq!(home.segments[0].extent_count, 512);
}

#[test]
fn parse_identifiers_with_plus_sign() {
    let text = build_minimal_metadata_text()
        .replace("test_vg", "test+vg")
        .replace("pv0", "pv+0")
        .replace("root {", "root+lv {");
    let vg = parse_metadata_text(&text).unwrap();

    assert_eq!(vg.name, "test+vg");
    assert_eq!(vg.physical_volumes[0].name, "pv+0");
    assert_eq!(vg.logical_volumes[0].name, "root+lv");
    assert_eq!(vg.logical_volumes[0].segments[0].stripes[0].0, "pv+0");
}

#[test]
fn parse_identifiers_still_reject_other_punctuation() {
    let text = build_minimal_metadata_text().replace("test_vg {", "test@vg {");
    let err = parse_metadata_text(&text).unwrap_err();

    assert!(
        matches!(err, LvmError::MetadataParseError { message, .. } if message.contains("expected"))
    );
}

#[test]
fn metadata_text_lv_sizes() {
    let text = build_minimal_metadata_text();
    let vg = parse_metadata_text(&text).unwrap();

    let extent_bytes = vg.extent_size * 512;
    let root = &vg.logical_volumes[0];
    assert_eq!(root.size_bytes, 1280 * extent_bytes);
    let home = &vg.logical_volumes[1];
    assert_eq!(home.size_bytes, 512 * extent_bytes);
}

#[test]
fn raid0_lvs_and_raids_metadata_is_marked_unsupported() {
    let text = base_metadata_with_lv(
        "        raid_lv {\n\
                 id = \"lv-raid0\"\n\
                 segment_count = 1\n\
                 segment1 {\n\
                 start_extent = 0\n\
                 extent_count = 4\n\
                 type = \"raid0\"\n\
                 stripe_count = 2\n\
                 stripe_size = 2\n\
                 raid0_lvs = [\"raid_lv_rimage_0\", \"raid_lv_rimage_1\"]\n\
                 raids = [\"raid_lv_rimage_0\", \"raid_lv_rimage_1\"]\n\
                 }\n\
                 }\n\
                 linear_lv {\n\
                 id = \"lv-linear\"\n\
                 segment_count = 1\n\
                 segment1 { start_extent = 0 extent_count = 1 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 10] }\n\
                 }\n",
    );

    let vg = parse_metadata_text(&text).unwrap();

    assert_eq!(vg.logical_volumes.len(), 2);
    assert_eq!(vg.logical_volumes[0].size_bytes, 4 * vg.extent_size * 512);
    assert!(matches!(
        vg.logical_volumes[0].segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));
    assert!(matches!(
        vg.logical_volumes[1].segments[0].seg_type,
        SegmentType::Linear
    ));
}

#[test]
fn striped_without_stripe_size_is_marked_unsupported() {
    let text = base_metadata_with_lv(
        "        bad_stripe {\n\
                 id = \"lv-bad-stripe\"\n\
                 segment_count = 1\n\
                 segment1 {\n\
                 start_extent = 0\n\
                 extent_count = 4\n\
                 type = \"striped\"\n\
                 stripe_count = 2\n\
                 stripes = [\"pv0\", 0, \"pv1\", 0]\n\
                 }\n\
                 }\n\
                 linear_lv {\n\
                 id = \"lv-linear\"\n\
                 segment_count = 1\n\
                 segment1 { start_extent = 0 extent_count = 1 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 10] }\n\
                 }\n",
    );

    let vg = parse_metadata_text(&text).unwrap();

    assert_eq!(vg.logical_volumes.len(), 2);
    assert_eq!(vg.logical_volumes[0].size_bytes, 4 * vg.extent_size * 512);
    assert!(matches!(
        vg.logical_volumes[0].segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));
    assert!(matches!(
        vg.logical_volumes[1].segments[0].seg_type,
        SegmentType::Linear
    ));
}

#[test]
fn valid_striped_keeps_declared_stripe_size() {
    let text = base_metadata_with_lv(
        "        stripe_lv {\n\
                 id = \"lv-stripe\"\n\
                 segment_count = 1\n\
                 segment1 {\n\
                 start_extent = 0\n\
                 extent_count = 4\n\
                 type = \"striped\"\n\
                 stripe_count = 2\n\
                 stripe_size = 2\n\
                 stripes = [\"pv0\", 0, \"pv1\", 10]\n\
                 }\n\
                 }\n",
    );

    let vg = parse_metadata_text(&text).unwrap();

    assert!(matches!(
        vg.logical_volumes[0].segments[0].seg_type,
        SegmentType::Striped {
            stripe_count: 2,
            stripe_size: 2
        }
    ));
}

#[test]
fn metadata_text_gapped_lv_is_marked_unsupported() {
    let text = concat!(
        "test_vg {\n",
        "    id = \"vg-size\"\n",
        "    seqno = 1\n",
        "    extent_size = 8192\n",
        "    physical_volumes { pv0 { id = \"pv0\" pe_start = 2048 pe_count = 1000 } }\n",
        "    logical_volumes {\n",
        "        gapped {\n",
        "            id = \"lv-gapped\"\n",
        "            segment_count = 2\n",
        "            segment1 { start_extent = 0 extent_count = 2 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 0] }\n",
        "            segment2 { start_extent = 5 extent_count = 2 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 20] }\n",
        "        }\n",
        "    }\n",
        "}\n",
    );
    let vg = parse_metadata_text(text).unwrap();
    let extent_bytes = vg.extent_size * 512;

    assert_eq!(vg.logical_volumes[0].size_bytes, 7 * extent_bytes);
    assert!(matches!(
        vg.logical_volumes[0].segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));
}

#[test]
fn committed_slot0_preferred_over_higher_seqno_precommit_slot1() {
    let committed_text = metadata_text_with_seqno(7);
    let precommit_text = metadata_text_with_seqno(99);
    let mut disk = vec![0u8; 8192];
    let mda = super::super::label::DataRegion {
        offset: 1024,
        size: 4096,
    };

    disk[1536..1536 + committed_text.len()].copy_from_slice(committed_text.as_bytes());
    disk[3072..3072 + precommit_text.len()].copy_from_slice(precommit_text.as_bytes());
    write_mda_header(
        &mut disk,
        1024,
        4096,
        512,
        committed_text.len() as u64,
        crc::lvm_crc32(committed_text.as_bytes()),
    );
    write_raw_location(
        &mut disk,
        1024,
        1,
        2048,
        precommit_text.len() as u64,
        crc::lvm_crc32(precommit_text.as_bytes()),
    );

    let mut reader = std::io::Cursor::new(disk);
    let vg = parse_metadata(&mut reader, &mda, 0).unwrap();

    assert_eq!(vg.seqno, 7);
}

#[test]
fn wrapped_raw_location_is_accepted() {
    let text = metadata_text_with_seqno(77);
    let text_bytes = text.as_bytes();
    let mda_size = 2048u64;
    let raw_offset = 1900u64;
    assert!(raw_offset + text_bytes.len() as u64 > mda_size);

    let mut disk = vec![0u8; 4096];
    let mda = super::super::label::DataRegion {
        offset: 1024,
        size: mda_size,
    };
    write_circular_bytes(&mut disk, 1024, mda_size, raw_offset, text_bytes);
    write_mda_header(
        &mut disk,
        1024,
        mda_size,
        raw_offset,
        text_bytes.len() as u64,
        crc::lvm_crc32(text_bytes),
    );

    let mut reader = std::io::Cursor::new(disk);
    let vg = parse_metadata(&mut reader, &mda, 0).unwrap();

    assert_eq!(vg.seqno, 77);
}

#[test]
fn corrupt_first_mda_followed_by_valid_second_mda_is_accepted() {
    let text = metadata_text_with_seqno(123);
    let text_bytes = text.as_bytes();
    let mut disk = vec![0u8; 12_288];
    let regions = [
        super::super::label::DataRegion {
            offset: 1024,
            size: 2048,
        },
        super::super::label::DataRegion {
            offset: 4096,
            size: 2048,
        },
    ];

    disk[1024 + 4..1024 + 20].copy_from_slice(b"corrupt-MDA-copy");
    disk[4096 + 512..4096 + 512 + text_bytes.len()].copy_from_slice(text_bytes);
    write_mda_header(
        &mut disk,
        4096,
        2048,
        512,
        text_bytes.len() as u64,
        crc::lvm_crc32(text_bytes),
    );

    let mut reader = std::io::Cursor::new(disk);
    let vg = parse_metadata_from_regions(&mut reader, &regions, 0).unwrap();

    assert_eq!(vg.seqno, 123);
}

#[test]
fn missing_required_fields_fail_closed() {
    let text = build_minimal_metadata_text().replace("            pe_start = 2048\n", "");
    let err = parse_metadata_text(&text).unwrap_err();

    assert!(
        matches!(err, LvmError::MetadataParseError { message, .. } if message.contains("pe_start"))
    );
}

#[test]
fn malformed_stripe_count_fails_closed() {
    let text = build_minimal_metadata_text().replace("                stripe_count = 1\n", "");
    let err = parse_metadata_text(&text).unwrap_err();

    assert!(
        matches!(err, LvmError::MetadataParseError { message, .. } if message.contains("stripe_count"))
    );
}

#[test]
fn malformed_stripes_list_fails_closed() {
    let text =
        build_minimal_metadata_text().replace("                stripes = [\"pv0\", 0]\n", "");
    let err = parse_metadata_text(&text).unwrap_err();

    assert!(
        matches!(err, LvmError::MetadataParseError { message, .. } if message.contains("stripes"))
    );
}

#[test]
fn unsupported_non_linear_lvs_do_not_abort_vg_parse() {
    let text = concat!(
        "test_vg {\n",
        "    id = \"vg-raid\"\n",
        "    seqno = 1\n",
        "    extent_size = 4096\n",
        "\n",
        "    physical_volumes { pv0 { id = \"pv0\" device = \"/dev/sda\" pe_start = 0 pe_count = 100 }\n",
        "                       pv1 { id = \"pv1\" device = \"/dev/sdb\" pe_start = 0 pe_count = 100 }\n",
        "                       pv2 { id = \"pv2\" device = \"/dev/sdc\" pe_start = 0 pe_count = 100 } }\n",
        "\n",
        "    logical_volumes {\n",
        "        lv_linear {\n",
        "            id = \"lv-linear\"\n",
        "            segment_count = 1\n",
        "            segment1 {\n",
        "                start_extent = 0\n",
        "                extent_count = 10\n",
        "                type = \"linear\"\n",
        "                stripe_count = 1\n",
        "                stripes = [\"pv0\", 0]\n",
        "            }\n",
        "        }\n",
        "        lv_raid5 {\n",
        "            id = \"lv-raid5\"\n",
        "            segment_count = 1\n",
        "            segment1 {\n",
        "                start_extent = 0\n",
        "                extent_count = 30\n",
        "                type = \"raid5\"\n",
        "                stripe_count = 3\n",
        "                stripes = [\"pv0\", 0, \"pv1\", 0, \"pv2\", 0]\n",
        "            }\n",
        "        }\n",
        "        lv_mirror {\n",
        "            id = \"lv-mirror\"\n",
        "            segment_count = 1\n",
        "            segment1 {\n",
        "                start_extent = 0\n",
        "                extent_count = 10\n",
        "                type = \"raid1\"\n",
        "                stripe_count = 2\n",
        "                stripes = [\"pv0\", 0, \"pv1\", 0]\n",
        "            }\n",
        "        }\n",
        "        lv_thin {\n",
        "            id = \"lv-thin\"\n",
        "            segment_count = 1\n",
        "            segment1 {\n",
        "                start_extent = 0\n",
        "                extent_count = 5\n",
        "                type = \"thin\"\n",
        "                thin_pool = \"pool\"\n",
        "                transaction_id = 1\n",
        "                device_id = 2\n",
        "            }\n",
        "        }\n",
        "    }\n",
        "}\n",
    );
    let vg = parse_metadata_text(text).unwrap();
    assert_eq!(vg.name, "test_vg");
    assert_eq!(vg.logical_volumes.len(), 4);

    let linear = &vg.logical_volumes[0];
    assert_eq!(linear.role, LvRole::Public);
    assert!(linear.is_visible());
    assert!(linear.is_directly_mappable());
    assert!(matches!(linear.segments[0].seg_type, SegmentType::Linear));

    let raid5 = &vg.logical_volumes[1];
    assert!(!raid5.is_directly_mappable());
    assert!(matches!(
        raid5.segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));

    let mirror = &vg.logical_volumes[2];
    assert_eq!(mirror.role, LvRole::Public);
    assert!(mirror.is_visible());
    assert!(!mirror.is_directly_mappable());
    assert!(matches!(
        mirror.segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));

    let thin = &vg.logical_volumes[3];
    assert_eq!(thin.role, LvRole::ThinVolume);
    assert!(thin.is_visible());
    assert!(!thin.is_directly_mappable());
    assert!(matches!(
        thin.segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));
}

#[test]
fn thin_cache_and_pool_segments_keep_distinct_unsupported_labels() {
    let text = base_metadata_with_lv(
        "\
         thin_root {
             id = \"lv-thin-root\"
             status = [\"READ\", \"WRITE\", \"VISIBLE\"]
             segment_count = 1
             segment1 { start_extent = 0 extent_count = 2 type = \"thin\" thin_pool = \"pool\" transaction_id = 1 device_id = 2 }
         }
         thin_pool {
             id = \"lv-thin-pool\"
             status = [\"READ\", \"WRITE\"]
             segment_count = 1
             segment1 { start_extent = 0 extent_count = 4 type = \"thin-pool\" metadata = \"pool_tmeta\" pool = \"pool_tdata\" transaction_id = 1 }
         }
         cached_root {
             id = \"lv-cache-root\"
             status = [\"READ\", \"WRITE\", \"VISIBLE\"]
             segment_count = 1
             segment1 { start_extent = 0 extent_count = 2 type = \"cache\" cache_pool = \"cache_pool\" origin = \"origin\" }
         }
         cache_pool {
             id = \"lv-cache-pool\"
             status = [\"READ\", \"WRITE\"]
             segment_count = 1
             segment1 { start_extent = 0 extent_count = 4 type = \"cache-pool\" metadata = \"cache_cmeta\" pool = \"cache_cdata\" }
         }\n",
    );

    let vg = parse_metadata_text(&text).unwrap();

    assert_eq!(vg.logical_volumes[0].role, LvRole::ThinVolume);
    assert!(matches!(
        vg.logical_volumes[0].segments[0].seg_type,
        SegmentType::Unsupported { ref type_name } if type_name.contains("thin")
    ));
    assert_eq!(vg.logical_volumes[1].role, LvRole::ThinPool);
    assert!(matches!(
        vg.logical_volumes[1].segments[0].seg_type,
        SegmentType::Unsupported { ref type_name } if type_name.contains("thin-pool")
    ));
    assert_eq!(vg.logical_volumes[2].role, LvRole::CacheVolume);
    assert!(matches!(
        vg.logical_volumes[2].segments[0].seg_type,
        SegmentType::Unsupported { ref type_name } if type_name.contains("cache")
    ));
    assert_eq!(vg.logical_volumes[3].role, LvRole::CachePool);
    assert!(matches!(
        vg.logical_volumes[3].segments[0].seg_type,
        SegmentType::Unsupported { ref type_name } if type_name.contains("cache-pool")
    ));
}

#[test]
fn component_lv_areas_are_preserved_for_diagnostics() {
    let text = base_metadata_with_lv(
        "\
         root_rimage_0 {
             id = \"lv-rimage-0\"
             status = [\"READ\", \"WRITE\"]
             segment_count = 1
             segment1 { start_extent = 0 extent_count = 4 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 0] }
         }
         root_rmeta_0 {
             id = \"lv-rmeta-0\"
             status = [\"READ\", \"WRITE\"]
             segment_count = 1
             segment1 { start_extent = 0 extent_count = 1 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 4] }
         }
         mirrored_root {
             id = \"lv-mirrored-root\"
             status = [\"READ\", \"WRITE\", \"VISIBLE\"]
             segment_count = 1
             segment1 {
                 start_extent = 0
                 extent_count = 4
                 type = \"raid1\"
                 stripe_count = 2
                 areas = [\"lv\", \"root_rimage_0\", 0, \"lv\", \"root_rmeta_0\", 0]
             }
         }\n",
    );

    let vg = parse_metadata_text(&text).unwrap();
    let mirrored = &vg.logical_volumes[2];

    assert_eq!(mirrored.role, LvRole::Public);
    assert!(mirrored.is_visible());
    assert!(!mirrored.is_directly_mappable());
    assert_eq!(mirrored.segments[0].areas.len(), 2);
    assert!(matches!(
        &mirrored.segments[0].areas[0],
        SegmentArea::LogicalVolume { name, start_extent }
            if name == "root_rimage_0" && *start_extent == 0
    ));
}

#[test]
fn metadata_text_classifies_hidden_and_internal_lvs() {
    let text = base_metadata_with_lv(
        "\
         root {\n\
             id = \"lv-root\"\n\
             status = [\"READ\", \"WRITE\", \"VISIBLE\"]\n\
             segment_count = 1\n\
             segment1 { start_extent = 0 extent_count = 1 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 0] }\n\
         }\n\
         hidden_tmp {\n\
             id = \"lv-hidden\"\n\
             status = [\"READ\", \"WRITE\"]\n\
             segment_count = 1\n\
             segment1 { start_extent = 0 extent_count = 1 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 1] }\n\
         }\n\
         pool_tdata {\n\
             id = \"lv-tdata\"\n\
             status = [\"READ\", \"WRITE\"]\n\
             segment_count = 1\n\
             segment1 { start_extent = 0 extent_count = 1 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 2] }\n\
         }\n\
         pool_tmeta {\n\
             id = \"lv-tmeta\"\n\
             status = [\"READ\", \"WRITE\"]\n\
             segment_count = 1\n\
             segment1 { start_extent = 0 extent_count = 1 type = \"linear\" stripe_count = 1 stripes = [\"pv0\", 3] }\n\
         }\n",
    );

    let vg = parse_metadata_text(&text).unwrap();
    let root = &vg.logical_volumes[0];
    let hidden = &vg.logical_volumes[1];
    let tdata = &vg.logical_volumes[2];
    let tmeta = &vg.logical_volumes[3];

    assert_eq!(root.status, vec!["READ", "WRITE", "VISIBLE"]);
    assert_eq!(root.role, LvRole::Public);
    assert!(root.is_visible());
    assert!(root.is_directly_mappable());

    assert_eq!(hidden.role, LvRole::Internal);
    assert!(!hidden.is_visible());
    assert!(!hidden.is_directly_mappable());

    assert_eq!(tdata.role, LvRole::ThinData);
    assert_eq!(tmeta.role, LvRole::ThinMetadata);
    assert!(!tdata.is_visible());
    assert!(!tmeta.is_visible());
    assert!(!tdata.is_directly_mappable());
    assert!(!tmeta.is_directly_mappable());
}

#[test]
fn metadata_text_keeps_mirror_lvs_visible_but_not_directly_mappable() {
    let text = base_metadata_with_lv(
        "\
         mirror_root {\n\
             id = \"lv-mirror-root\"\n\
             status = [\"READ\", \"WRITE\", \"VISIBLE\"]\n\
             segment_count = 1\n\
             segment1 {\n\
                 start_extent = 0\n\
                 extent_count = 4\n\
                 type = \"raid1\"\n\
                 stripe_count = 2\n\
                 stripes = [\"pv0\", 0, \"pv0\", 4]\n\
             }\n\
         }\n\
         parity_root {\n\
             id = \"lv-parity-root\"\n\
             status = [\"READ\", \"WRITE\", \"VISIBLE\"]\n\
             segment_count = 1\n\
             segment1 {\n\
                 start_extent = 0\n\
                 extent_count = 4\n\
                 type = \"raid5\"\n\
                 stripe_count = 3\n\
                 stripes = [\"pv0\", 8, \"pv0\", 12, \"pv0\", 16]\n\
             }\n\
         }\n",
    );

    let vg = parse_metadata_text(&text).unwrap();
    let mirror = &vg.logical_volumes[0];
    assert_eq!(mirror.role, LvRole::Public);
    assert!(mirror.is_visible());
    assert!(!mirror.is_directly_mappable());
    assert!(matches!(
        mirror.segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));

    let parity = &vg.logical_volumes[1];
    assert_eq!(parity.role, LvRole::Public);
    assert!(parity.is_visible());
    assert!(!parity.is_directly_mappable());
    assert!(matches!(
        parity.segments[0].seg_type,
        SegmentType::Unsupported { .. }
    ));
}
