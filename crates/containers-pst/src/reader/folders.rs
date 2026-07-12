use super::PstReader;
use crate::header::{NID_ROOT_FOLDER, NID_SEARCH_ROOT, NID_TOP_OF_PERSONAL_FOLDERS};
use crate::props::PROP_TAG_MESSAGE_CLASS;
use crate::{PstError, PstFolder};

impl PstReader {
    pub fn read_folders(&self) -> Result<Vec<PstFolder>, PstError> {
        Ok(self
            .collect_folder_nids()?
            .into_iter()
            .map(|nid| {
                let parent_path = self.get_folder_path(nid);
                PstFolder {
                    name: self
                        .get_property_string(nid, 0x3001)
                        .unwrap_or_else(|| format!("Folder_{nid:X}")),
                    depth: parent_path
                        .split('/')
                        .filter(|part| !part.is_empty())
                        .count() as u32,
                    parent_path,
                    message_count: self.get_subnode_nids(nid).unwrap_or_default().len() as u64,
                    subfolder_count: 0,
                }
            })
            .collect())
    }

    pub(super) fn collect_folder_nids(&self) -> Result<Vec<u32>, PstError> {
        let mut folders = Vec::new();
        for root in [
            NID_ROOT_FOLDER,
            NID_TOP_OF_PERSONAL_FOLDERS,
            NID_SEARCH_ROOT,
        ] {
            if self.nbt_cache.contains_key(&root) {
                folders.push(root);
                self.collect_child_folders(root, &mut folders);
            }
        }
        if folders.is_empty() {
            folders.extend(
                self.nbt_cache
                    .keys()
                    .copied()
                    .filter(|nid| self.looks_like_folder(*nid)),
            );
        }
        Ok(folders)
    }

    fn looks_like_folder(&self, nid: u32) -> bool {
        if self.read_subnode_block(nid).is_none() {
            return false;
        }
        !matches!(
            self.get_property_string(nid, PROP_TAG_MESSAGE_CLASS)
                .as_deref(),
            Some("IPM.Note" | "IPM.Appointment" | "IPM.Contact")
        )
    }

    fn collect_child_folders(&self, parent: u32, result: &mut Vec<u32>) {
        let Ok(children) = self.get_subnode_nids(parent) else {
            return;
        };
        for child in children {
            let class = self.get_property_string(child, PROP_TAG_MESSAGE_CLASS);
            if matches!(
                class.as_deref(),
                Some("IPM.Note" | "IPM.Note.SMIME" | "IPM.Appointment" | "IPM.Contact")
            ) {
                continue;
            }
            if self.get_property_string(child, 0x3613).is_some() {
                result.push(child);
                self.collect_child_folders(child, result);
            }
        }
    }

    pub(super) fn get_subnode_nids(&self, folder_nid: u32) -> Result<Vec<u32>, PstError> {
        if self.read_subnode_block(folder_nid).is_none() {
            return Ok(Vec::new());
        }
        Ok(self
            .nbt_cache
            .iter()
            .filter(|(nid, entry)| **nid != folder_nid && entry.bid_sub != 0)
            .filter_map(|(nid, _)| {
                self.read_subnode_block(*nid)
                    .map(|block| (*nid, self.parse_property_context(block)))
            })
            .filter(|(_, properties)| !properties.is_empty())
            .map(|(nid, _)| nid)
            .collect())
    }

    pub(super) fn get_folder_path(&self, nid: u32) -> String {
        let name = self
            .get_property_string(nid, 0x3001)
            .unwrap_or_else(|| format!("Folder_{nid:X}"));
        format!("/{name}")
    }
}
