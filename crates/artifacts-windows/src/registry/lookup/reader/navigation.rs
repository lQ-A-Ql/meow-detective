use super::{cells::node, RegistryHiveReader};
use crate::registry::{
    lookup::types::{NkRecord, RegistryValue, MAX_KEY_LOOKUP_DEPTH},
    reader::BrowserError,
};

impl RegistryHiveReader<'_> {
    pub(crate) fn lookup_value(
        &self,
        path: &[&str],
        name: &str,
    ) -> Result<Option<RegistryValue>, String> {
        match self.navigate_to(path)? {
            Some(key) => self.read_value(&key, name),
            None => Ok(None),
        }
    }

    pub(crate) fn navigate_to(&self, path: &[&str]) -> Result<Option<NkRecord>, String> {
        if path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {MAX_KEY_LOOKUP_DEPTH}",
                path.len()
            ));
        }
        if path.is_empty() {
            return self.parse_nk(self.root_cell_offset).map(Some);
        }
        match self.view.resolve(&path.join("\\")) {
            Ok(key) => Ok(Some(node(key))),
            Err(BrowserError::NotFound) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    pub(crate) fn read_subkeys_from_nk(
        &self,
        key: &NkRecord,
    ) -> Result<Vec<(String, NkRecord)>, String> {
        let key = self
            .view
            .key(key.cell_offset, String::new())
            .map_err(|error| error.to_string())?;
        let page = self
            .view
            .subkeys(&key, 0, 100_000)
            .map_err(|error| error.to_string())?;
        if let Some(error) = page.diagnostic {
            return Err(error.to_string());
        }
        page.items
            .into_iter()
            .map(|(_, key)| {
                key.map(|key| (key.name.clone(), node(key)))
                    .map_err(|error| error.to_string())
            })
            .collect()
    }

    pub(crate) fn read_subkey_names_from_nk(&self, key: &NkRecord) -> Result<Vec<String>, String> {
        Ok(self
            .read_subkeys_from_nk(key)?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }

    pub(crate) fn control_set_candidates(&self, warnings: &mut Vec<String>) -> Vec<String> {
        let mut candidates = Vec::new();
        match self.lookup_value(&["Select"], "Current") {
            Ok(Some(RegistryValue::Dword(value))) if (1..=999).contains(&value) => {
                candidates.push(format!("ControlSet{value:03}"));
            }
            Ok(Some(value)) => warnings.push(format!(
                "Select\\Current has unsupported type: {:?}; falling back to common ControlSet names", value
            )),
            Ok(None) => warnings.push("Select\\Current not found; falling back to common ControlSet names".into()),
            Err(error) => warnings.push(format!("Select\\Current parse error: {error}; falling back to common ControlSet names")),
        }
        for candidate in ["ControlSet001", "ControlSet002", "CurrentControlSet"] {
            if !candidates
                .iter()
                .any(|name| name.eq_ignore_ascii_case(candidate))
            {
                candidates.push(candidate.into());
            }
        }
        candidates
    }
}
