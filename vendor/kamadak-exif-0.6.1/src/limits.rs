/// Resource limits applied by the native TIFF directory parser.
#[derive(Debug, Clone, Copy)]
pub struct ParseLimits {
    pub max_directories: usize,
    pub max_entries: usize,
    pub max_value_bytes: usize,
    pub max_total_value_bytes: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_directories: usize::MAX,
            max_entries: usize::MAX,
            max_value_bytes: usize::MAX,
            max_total_value_bytes: usize::MAX,
        }
    }
}
