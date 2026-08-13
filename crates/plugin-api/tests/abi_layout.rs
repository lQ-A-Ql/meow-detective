//! Layout locks for the plugin ABI: any accidental field reordering or
//! padding change must break this test before it breaks loaded plugins.

use plugin_api::{
    MeowEvidencePlatform, MeowExtractRequest, MeowExtractResponse, MeowPluginInfo, MeowStatus,
};
use std::mem::{offset_of, size_of};

#[test]
fn plugin_info_layout_is_locked() {
    assert_eq!(size_of::<MeowPluginInfo>(), 56);
    assert_eq!(offset_of!(MeowPluginInfo, struct_size), 0);
    assert_eq!(offset_of!(MeowPluginInfo, abi_version), 4);
    assert_eq!(offset_of!(MeowPluginInfo, plugin_id), 8);
    assert_eq!(offset_of!(MeowPluginInfo, plugin_version), 16);
    assert_eq!(offset_of!(MeowPluginInfo, display_name), 24);
    assert_eq!(offset_of!(MeowPluginInfo, evidence_platform), 32);
    assert_eq!(offset_of!(MeowPluginInfo, families_json), 40);
    assert_eq!(offset_of!(MeowPluginInfo, path_patterns_json), 48);
}

#[test]
fn extract_request_layout_is_locked() {
    assert_eq!(size_of::<MeowExtractRequest>(), 40);
    assert_eq!(offset_of!(MeowExtractRequest, struct_size), 0);
    assert_eq!(offset_of!(MeowExtractRequest, file_path), 8);
    assert_eq!(offset_of!(MeowExtractRequest, file_id), 16);
    assert_eq!(offset_of!(MeowExtractRequest, data), 24);
    assert_eq!(offset_of!(MeowExtractRequest, data_len), 32);
}

#[test]
fn extract_response_layout_is_locked() {
    assert_eq!(size_of::<MeowExtractResponse>(), 32);
    assert_eq!(offset_of!(MeowExtractResponse, struct_size), 0);
    assert_eq!(offset_of!(MeowExtractResponse, status), 4);
    assert_eq!(offset_of!(MeowExtractResponse, payload), 8);
    assert_eq!(offset_of!(MeowExtractResponse, payload_len), 16);
    assert_eq!(offset_of!(MeowExtractResponse, error_message), 24);
}

#[test]
fn enum_discriminants_are_locked() {
    assert_eq!(MeowEvidencePlatform::Windows as u32, 0);
    assert_eq!(MeowEvidencePlatform::Linux as u32, 1);
    assert_eq!(MeowStatus::Ok as u32, 0);
    assert_eq!(MeowStatus::ParseError as u32, 1);
    assert_eq!(MeowStatus::Unsupported as u32, 2);
    assert_eq!(MeowStatus::InternalError as u32, 3);
    assert_eq!(size_of::<MeowStatus>(), 4);
    assert_eq!(size_of::<MeowEvidencePlatform>(), 4);
}
