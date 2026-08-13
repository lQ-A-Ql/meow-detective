//! Version constant and exported symbol name locks.

use plugin_api::{
    MEOW_PLUGIN_ABI_VERSION, MEOW_PLUGIN_EXTRACT_SYMBOL, MEOW_PLUGIN_FREE_BUFFER_SYMBOL,
    MEOW_PLUGIN_INFO_SYMBOL,
};

#[test]
fn abi_version_is_one() {
    assert_eq!(MEOW_PLUGIN_ABI_VERSION, 1);
}

#[test]
fn exported_symbol_names_are_locked() {
    assert_eq!(MEOW_PLUGIN_INFO_SYMBOL, b"meow_plugin_info\0");
    assert_eq!(MEOW_PLUGIN_EXTRACT_SYMBOL, b"meow_plugin_extract\0");
    assert_eq!(MEOW_PLUGIN_FREE_BUFFER_SYMBOL, b"meow_plugin_free_buffer\0");
}
