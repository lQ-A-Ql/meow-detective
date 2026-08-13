use artifacts_core::ExtractorRegistry;

pub fn create_registry() -> ExtractorRegistry {
    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(artifacts_windows::PrefetchExtractor));
    registry.register(Box::new(artifacts_windows::LnkExtractor));
    registry.register(Box::new(artifacts_windows::RecycleBinExtractor));
    // Registry hives use the canonical analysis-service lookup path.
    registry.register(Box::new(artifacts_windows::JumpListExtractor));
    registry.register(Box::new(artifacts_windows::SruExtractor));
    registry.register(Box::new(artifacts_windows::ThumbcacheExtractor));
    // Parser plugins (design doc §5.6): appended after the built-ins with the
    // same priority. All failures are logged inside the loader, never fatal.
    for extractor in crate::plugin_loader::load_all() {
        registry.register(extractor);
    }
    registry
}
