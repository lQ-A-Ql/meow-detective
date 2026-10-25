use super::types::{BASE_BLOCK_SIZE, INVALID_OFFSET};
use crate::registry::{reader::HiveView, RegistryHiveSource};
use std::borrow::Cow;

mod bounded;
mod cells;
mod class_name;
mod navigation;
mod values;

pub(crate) struct RegistryHiveReader<'a> {
    pub(crate) view: Cow<'a, HiveView>,
    pub(crate) root_cell_offset: u32,
}

impl<'a> RegistryHiveReader<'a> {
    pub(crate) fn new(source: &'a (impl RegistryHiveSource + ?Sized)) -> Result<Self, String> {
        let view = source.registry_view().map_err(|error| error.to_string())?;
        let root_cell_offset = view.root;
        Ok(Self {
            view,
            root_cell_offset,
        })
    }

    pub(crate) fn abs(&self, hive_offset: u32) -> Result<usize, String> {
        if hive_offset == INVALID_OFFSET {
            return Err("invalid registry offset".to_string());
        }
        BASE_BLOCK_SIZE
            .checked_add(hive_offset as usize)
            .ok_or_else(|| "registry offset overflow".to_string())
            .and_then(|absolute| {
                (absolute < self.view.bytes.len())
                    .then_some(absolute)
                    .ok_or_else(|| format!("registry offset {hive_offset:#x} out of bounds"))
            })
    }
}
