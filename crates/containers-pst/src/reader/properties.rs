use super::PstReader;
use crate::header::PAGE_SIZE;
use crate::props::{
    find_prop_filetime, find_prop_string, find_prop_string_array, parse_property_context,
    prop_type, PropValue,
};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

impl PstReader {
    pub(crate) fn read_subnode_block(&self, nid: u32) -> Option<&[u8]> {
        let entry = self.nbt_cache.get(&nid)?;
        let bid = if entry.bid_sub != 0 {
            entry.bid_sub
        } else {
            entry.bid_data
        };
        if bid == 0 {
            return None;
        }
        let offset = self.bid_to_file_offset(bid);
        let end = (offset + PAGE_SIZE).min(self.data.len());
        self.data.get(offset..end)
    }

    pub(super) fn get_property_string(&self, nid: u32, tag: u16) -> Option<String> {
        find_prop_string(self.read_subnode_block(nid)?, tag)
    }

    pub(super) fn get_property_filetime(&self, nid: u32, tag: u16) -> Option<DateTime<Utc>> {
        find_prop_filetime(self.read_subnode_block(nid)?, tag)
    }

    pub(super) fn get_property_string_array(&self, nid: u32, tag: u16) -> Option<Vec<String>> {
        find_prop_string_array(self.read_subnode_block(nid)?, tag)
    }

    pub(crate) fn parse_property_context(&self, data: &[u8]) -> BTreeMap<u32, PropValue> {
        parse_property_context(data)
    }

    pub(super) fn get_property_binary(&self, nid: u32, tag: u16) -> Option<Vec<u8>> {
        let properties = self.parse_property_context(self.read_subnode_block(nid)?);
        let key = ((tag as u32) << 16) | prop_type::PtypBinary as u32;
        match properties.get(&key) {
            Some(PropValue::Binary(data)) => Some(data.clone()),
            _ => None,
        }
    }
}
