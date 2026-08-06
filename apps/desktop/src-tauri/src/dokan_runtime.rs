use std::sync::Once;

static DOKAN_INIT: Once = Once::new();

pub(crate) fn initialize() {
    DOKAN_INIT.call_once(dokan::init);
}
