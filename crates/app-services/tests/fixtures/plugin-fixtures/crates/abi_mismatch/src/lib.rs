//! Fixture plugin: reports a future ABI version. The host must refuse it
//! during the handshake without calling any other export.

use plugin_api::{MeowEvidencePlatform, MeowPluginInfo, MEOW_PLUGIN_ABI_VERSION};

#[no_mangle]
pub unsafe extern "C" fn meow_plugin_info() -> MeowPluginInfo {
    MeowPluginInfo {
        struct_size: std::mem::size_of::<MeowPluginInfo>() as u32,
        abi_version: MEOW_PLUGIN_ABI_VERSION + 1,
        plugin_id: b"meow.fixture.abi-mismatch\0".as_ptr(),
        plugin_version: b"0.1.0\0".as_ptr(),
        display_name: b"Fixture ABI Mismatch\0".as_ptr(),
        evidence_platform: MeowEvidencePlatform::Windows,
        families_json: b"[\"Fixture\"]\0".as_ptr(),
        path_patterns_json: b"[\"*.abx\"]\0".as_ptr(),
    }
}
