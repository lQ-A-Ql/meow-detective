use std::io::{Cursor, Write};

use artifacts_android::{
    find_setting_value, inspect_apk, parse_build_properties, parse_package_restrictions,
    parse_packages_list, parse_packages_xml, BuildPropertyError,
};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[test]
fn parses_android_build_properties_without_interpreting_shell_syntax() {
    let properties = parse_build_properties(
        b"# comment\nro.product.model = Pixel Test\nro.build.version.release=15 # literal\n",
    )
    .expect("property file parses");

    assert_eq!(properties.len(), 2);
    assert_eq!(properties[0].key, "ro.product.model");
    assert_eq!(properties[0].value, "Pixel Test");
    assert_eq!(properties[1].value, "15 # literal");
}

#[test]
fn rejects_nul_in_property_data() {
    let error = parse_build_properties(b"ro.product.model=Pixel\0Test")
        .expect_err("NUL must not become a property value");

    assert_eq!(error, BuildPropertyError::NulByte);
}

#[test]
fn parses_package_manager_hex_timestamps() {
    let packages = parse_packages_xml(
        br#"<packages><package name="org.example.app" codePath="/data/app/a" version="42" userId="10042" installer="com.store" it="18d3e123456" ut="18d4e123456"/></packages>"#,
    )
    .expect("package metadata parses");

    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_name, "org.example.app");
    assert_eq!(packages[0].uid, Some(10_042));
    assert_eq!(packages[0].version_code.as_deref(), Some("42"));
    assert!(packages[0].install_time_millis.is_some());
}

#[test]
fn uses_ft_when_packages_xml_has_no_it_timestamp() {
    let packages = parse_packages_xml(
        br#"<packages><package name="org.example.app" ft="18d3e123456"/></packages>"#,
    )
    .expect("package metadata parses");

    assert!(packages[0].install_time_millis.is_some());
}

#[test]
fn parses_packages_list_leading_identity_columns() {
    let records = parse_packages_list(
        b"org.example.app 10042 0 /data/user/0/org.example.app platform:targetSdkVersion=35\n",
    )
    .expect("packages.list parses");

    assert_eq!(records[0].package_name, "org.example.app");
    assert_eq!(records[0].uid, Some(10_042));
}

#[test]
fn parses_package_restrictions_state_fields() {
    let restrictions = parse_package_restrictions(
        br#"<package-restrictions><pkg name="org.example.app" installed="true" enabled="2" stopped="true"/></package-restrictions>"#,
    )
    .expect("package restrictions parse");

    assert_eq!(restrictions[0].package_name, "org.example.app");
    assert_eq!(restrictions[0].installed, Some(true));
    assert_eq!(restrictions[0].enabled_state, Some(2));
    assert_eq!(restrictions[0].stopped, Some(true));
}

#[test]
fn parses_abx_package_restrictions() {
    let bytes = android_abx::events_to_abx(&[
        android_abx::Event::StartDocument,
        android_abx::Event::StartTag {
            name: "package-restrictions".into(),
            attributes: Vec::new(),
        },
        android_abx::Event::StartTag {
            name: "pkg".into(),
            attributes: vec![
                android_abx::Attribute {
                    name: "name".into(),
                    value: android_abx::AttributeValue::String("org.example.abx".to_string()),
                },
                android_abx::Attribute {
                    name: "suspended".into(),
                    value: android_abx::AttributeValue::Boolean(true),
                },
            ],
        },
        android_abx::Event::EndTag { name: "pkg".into() },
        android_abx::Event::EndTag {
            name: "package-restrictions".into(),
        },
        android_abx::Event::EndDocument,
    ])
    .expect("create ABX fixture");
    let restrictions = parse_package_restrictions(&bytes).expect("ABX restrictions parse");

    assert_eq!(restrictions[0].package_name, "org.example.abx");
    assert_eq!(restrictions[0].suspended, Some(true));
}

#[test]
fn parses_abx_package_manager_metadata() {
    let bytes = android_abx::events_to_abx(&[
        android_abx::Event::StartDocument,
        android_abx::Event::StartTag {
            name: "packages".into(),
            attributes: Vec::new(),
        },
        android_abx::Event::StartTag {
            name: "package".into(),
            attributes: vec![
                android_abx::Attribute {
                    name: "name".into(),
                    value: android_abx::AttributeValue::String("org.example.abx".to_string()),
                },
                android_abx::Attribute {
                    name: "userId".into(),
                    value: android_abx::AttributeValue::Int(10_321),
                },
                android_abx::Attribute {
                    name: "it".into(),
                    value: android_abx::AttributeValue::LongHex(0x18d3e123456),
                },
            ],
        },
        android_abx::Event::EndTag {
            name: "package".into(),
        },
        android_abx::Event::EndTag {
            name: "packages".into(),
        },
        android_abx::Event::EndDocument,
    ])
    .expect("create ABX fixture");
    let packages = parse_packages_xml(&bytes).expect("ABX package metadata parses");

    assert_eq!(packages[0].package_name, "org.example.abx");
    assert_eq!(packages[0].uid, Some(10_321));
    assert!(packages[0].install_time_millis.is_some());
}

#[test]
fn ignores_unknown_package_nodes() {
    let packages = parse_packages_xml(br#"<packages><shared-user name="x"/></packages>"#)
        .expect("unknown nodes are ignored");

    assert!(packages.is_empty());
}

#[test]
fn rejects_malformed_package_xml() {
    assert!(parse_packages_xml(b"<packages><package>").is_err());
}

#[test]
fn rejects_truncated_package_restrictions_xml() {
    assert!(
        parse_package_restrictions(b"<package-restrictions><pkg name=\"org.example\">").is_err()
    );
}

#[test]
fn finds_a_named_setting_value() {
    let value = find_setting_value(
        br#"<settings><setting name="android_id" value="abcdef0123456789"/></settings>"#,
        "android_id",
    )
    .expect("settings XML parses");

    assert_eq!(value.as_deref(), Some("abcdef0123456789"));
}

#[test]
fn rejects_truncated_settings_xml() {
    assert!(find_setting_value(
        br#"<settings><setting name="android_id" value="abc">"#,
        "android_id",
    )
    .is_err());
}

#[test]
fn finds_a_named_setting_value_in_abx() {
    let bytes = android_abx::events_to_abx(&[
        android_abx::Event::StartDocument,
        android_abx::Event::StartTag {
            name: "settings".into(),
            attributes: Vec::new(),
        },
        android_abx::Event::StartTag {
            name: "setting".into(),
            attributes: vec![
                android_abx::Attribute {
                    name: "name".into(),
                    value: android_abx::AttributeValue::String("android_id".to_string()),
                },
                android_abx::Attribute {
                    name: "value".into(),
                    value: android_abx::AttributeValue::String("abcdef0123456789".to_string()),
                },
            ],
        },
        android_abx::Event::EndTag {
            name: "setting".into(),
        },
        android_abx::Event::EndTag {
            name: "settings".into(),
        },
        android_abx::Event::EndDocument,
    ])
    .expect("create ABX fixture");

    let value = find_setting_value(&bytes, "android_id").expect("ABX settings parse");
    assert_eq!(value.as_deref(), Some("abcdef0123456789"));
}

#[test]
fn handles_an_apk_without_a_manifest() {
    let cursor = Cursor::new(Vec::new());
    let mut archive = ZipWriter::new(cursor);
    archive
        .start_file("assets/placeholder", SimpleFileOptions::default())
        .expect("entry starts");
    archive.write_all(b"data").expect("entry writes");
    let bytes = archive.finish().expect("archive finishes").into_inner();

    let presentation = inspect_apk(Cursor::new(bytes)).expect("archive can be inspected");
    assert_eq!(presentation.app_name, None);
    assert_eq!(presentation.icon, None);
}

#[test]
fn rejects_invalid_apk_data() {
    assert!(inspect_apk(Cursor::new(b"not a zip".to_vec())).is_err());
}

#[test]
fn resolves_an_apk_label_and_png_icon_from_binary_resources() {
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    write_zip_entry(&mut archive, "AndroidManifest.xml", &binary_manifest());
    write_zip_entry(&mut archive, "resources.arsc", &resource_table());
    write_zip_entry(
        &mut archive,
        "res/mipmap/ic_launcher.png",
        b"\x89PNG\r\n\x1a\nfixture",
    );

    let bytes = archive.finish().expect("archive finishes").into_inner();
    let presentation = inspect_apk(Cursor::new(bytes)).expect("APK parses");

    assert_eq!(presentation.app_name.as_deref(), Some("Example app"));
    let icon = presentation.icon.expect("PNG launcher icon");
    assert_eq!(icon.mime_type, "image/png");
    assert_eq!(icon.bytes, b"\x89PNG\r\n\x1a\nfixture");
}

fn write_zip_entry(archive: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, bytes: &[u8]) {
    archive
        .start_file(name, SimpleFileOptions::default())
        .expect("entry starts");
    archive.write_all(bytes).expect("entry writes");
}

fn binary_manifest() -> Vec<u8> {
    let strings = string_pool(&["application", "label", "icon"]);
    let mut resource_map = chunk(0x0180, 8, vec![0; 12]);
    resource_map[8..12].copy_from_slice(&0u32.to_le_bytes());
    resource_map[12..16].copy_from_slice(&0x0101_0001u32.to_le_bytes());
    resource_map[16..20].copy_from_slice(&0x0101_0002u32.to_le_bytes());

    let mut application = vec![0; 76];
    write_chunk_header(&mut application, 0x0102, 16, 76);
    application[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    application[20..24].copy_from_slice(&0u32.to_le_bytes());
    application[24..26].copy_from_slice(&20u16.to_le_bytes());
    application[26..28].copy_from_slice(&20u16.to_le_bytes());
    application[28..30].copy_from_slice(&2u16.to_le_bytes());
    write_attribute(&mut application[36..56], 1, 0x7f01_0000);
    write_attribute(&mut application[56..76], 2, 0x7f02_0000);

    chunk(0x0003, 8, [strings, resource_map, application].concat())
}

fn write_attribute(target: &mut [u8], name_index: u32, resource_id: u32) {
    target[4..8].copy_from_slice(&name_index.to_le_bytes());
    target[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    target[12..14].copy_from_slice(&8u16.to_le_bytes());
    target[15] = 0x01;
    target[16..20].copy_from_slice(&resource_id.to_le_bytes());
}

fn resource_table() -> Vec<u8> {
    let strings = string_pool(&["Example app", "res/mipmap/ic_launcher.png"]);
    let label_type = resource_type(1, 0);
    let icon_type = resource_type(2, 1);

    let mut package = vec![0; 288];
    write_chunk_header(
        &mut package,
        0x0200,
        288,
        (288 + label_type.len() + icon_type.len()) as u32,
    );
    package[8..12].copy_from_slice(&0x7fu32.to_le_bytes());
    package.extend(label_type);
    package.extend(icon_type);

    let mut root = vec![0; 12];
    write_chunk_header(
        &mut root,
        0x0002,
        12,
        (12 + strings.len() + package.len()) as u32,
    );
    root[8..12].copy_from_slice(&1u32.to_le_bytes());
    root.extend(strings);
    root.extend(package);
    root
}

fn resource_type(type_id: u8, string_index: u32) -> Vec<u8> {
    let mut value = vec![0; 68];
    write_chunk_header(&mut value, 0x0201, 48, 68);
    value[8] = type_id;
    value[12..16].copy_from_slice(&1u32.to_le_bytes());
    value[16..20].copy_from_slice(&52u32.to_le_bytes());
    value[20..24].copy_from_slice(&28u32.to_le_bytes());
    value[52..54].copy_from_slice(&8u16.to_le_bytes());
    value[60..62].copy_from_slice(&8u16.to_le_bytes());
    value[63] = 0x03;
    value[64..68].copy_from_slice(&string_index.to_le_bytes());
    value
}

fn string_pool(strings: &[&str]) -> Vec<u8> {
    let mut payload = Vec::new();
    let mut offsets = Vec::new();
    for value in strings {
        offsets.push(payload.len() as u32);
        payload.push(value.len() as u8);
        payload.push(value.len() as u8);
        payload.extend(value.as_bytes());
        payload.push(0);
    }
    let strings_start = 28 + offsets.len() * 4;
    let mut pool = vec![0; strings_start];
    write_chunk_header(
        &mut pool,
        0x0001,
        28,
        (strings_start + payload.len()) as u32,
    );
    pool[8..12].copy_from_slice(&(strings.len() as u32).to_le_bytes());
    pool[16..20].copy_from_slice(&0x0000_0100u32.to_le_bytes());
    pool[20..24].copy_from_slice(&(strings_start as u32).to_le_bytes());
    for (index, offset) in offsets.iter().enumerate() {
        let start = 28 + index * 4;
        pool[start..start + 4].copy_from_slice(&offset.to_le_bytes());
    }
    pool.extend(payload);
    pool
}

fn chunk(chunk_type: u16, header_size: u16, payload: Vec<u8>) -> Vec<u8> {
    let mut result = vec![0; header_size as usize];
    result.extend(payload);
    let size = result.len() as u32;
    write_chunk_header(&mut result, chunk_type, header_size, size);
    result
}

fn write_chunk_header(target: &mut [u8], chunk_type: u16, header_size: u16, size: u32) {
    target[..2].copy_from_slice(&chunk_type.to_le_bytes());
    target[2..4].copy_from_slice(&header_size.to_le_bytes());
    target[4..8].copy_from_slice(&size.to_le_bytes());
}
