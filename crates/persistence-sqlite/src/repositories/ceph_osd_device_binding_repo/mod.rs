mod query;
mod records;
mod validation;
mod write;

use std::collections::HashSet;

use rusqlite::Connection;

use crate::connection::DbResult;

pub use records::{
    CephOsdDeviceBindingAggregate, CephOsdDeviceBindingRecord, CephOsdPvBindingRecord,
    CephOsdRegisteredSourceIdentity, CephOsdSourceBoundDevice,
};

pub struct CephOsdDeviceBindingRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephOsdDeviceBindingRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_source_bound_device(
        &self,
        data_source_id: &str,
        inventory_id: &str,
    ) -> DbResult<Option<CephOsdSourceBoundDevice>> {
        query::find_source_bound_device(self.conn, data_source_id, inventory_id)
    }
}

pub(crate) fn validate_replacement(
    data_source_id: &str,
    inventory_ids: &HashSet<&str>,
    bindings: &[CephOsdDeviceBindingAggregate],
) -> DbResult<()> {
    validation::validate_replacement(data_source_id, inventory_ids, bindings)
}

pub(crate) fn replace_for_data_source_on(
    conn: &Connection,
    bindings: &[CephOsdDeviceBindingAggregate],
) -> DbResult<()> {
    write::replace_for_data_source_on(conn, bindings)
}
