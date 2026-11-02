use super::{
    index_write, orphan_key, record_key, CatalogPage, CatalogRecordStore, DIRECTORY_FAMILY,
    ROOT_PARENT,
};
use crate::{
    codec::{encode_key, KeyPart},
    database::SourceReadSnapshot,
    errors::Result,
    scan::ScanDirection,
};

impl CatalogRecordStore {
    pub(super) fn index_children_as_orphans(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        parent_id: &str,
        actions: &mut Vec<crate::database::WriteAction>,
    ) -> Result<()> {
        let mut cursor = None;
        loop {
            let page = self.list_directories_by_parent(
                snapshot,
                source_id,
                Some(parent_id),
                cursor.as_deref(),
                ScanDirection::Forward,
                256,
            )?;
            for child in page.records {
                if child.id != parent_id {
                    actions.push(index_write(
                        orphan_key(&child)?,
                        record_key(source_id, &child.id)?,
                    ));
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(());
            }
        }
    }
    pub fn list_directories_by_parent(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        parent_id: Option<&str>,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage> {
        let parent = parent_id.unwrap_or(ROOT_PARENT);
        let prefix = encode_key(
            DIRECTORY_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(parent.to_owned()),
            ],
        )?;
        self.page_from_index(snapshot, &prefix, cursor, direction, limit)
    }
}
