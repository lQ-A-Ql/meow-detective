use super::RegistryHiveReader;

impl RegistryHiveReader<'_> {
    pub(crate) fn read_class_name_at(&self, path: &[&str]) -> Result<Option<String>, String> {
        let Some(key) = self.navigate_to(path)? else {
            return Ok(None);
        };
        let key = self
            .view
            .key(key.cell_offset, String::new())
            .map_err(|error| error.to_string())?;
        self.view
            .class_name(&key)
            .map_err(|error| error.to_string())
    }
}
