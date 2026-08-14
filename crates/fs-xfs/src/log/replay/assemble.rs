//! Assembly of committed transactions from CRC-validated log records.
//!
//! Mirrors the kernel's transaction rebuild (`xlog_recover_process_ophdr` /
//! `xlog_recover_add_to_trans` / `xlog_recover_add_to_cont_trans`):
//!
//! - An op with `XLOG_START_TRANS` opens a new transaction for its tid; ops
//!   for an unknown tid without the start flag are slack and skipped.
//! - The first region of a transaction is the 16-byte `xfs_trans_header`
//!   (host-order magic `TRAN`); it may be split across a continuation op.
//! - Every following region starts or extends an item; the first region of
//!   an item carries `type:u16 size:u16` in host order where `size` is the
//!   item's total region count.
//! - `XLOG_WAS_CONT_TRANS` ops append their bytes to the tail region of the
//!   transaction's current item (a region split across ops).
//! - `XLOG_COMMIT_TRANS` closes the transaction and queues it for replay;
//!   `XLOG_UNMOUNT_TRANS` and unexpected flag combinations drop it, which
//!   discards uncommitted/aborted work exactly like the kernel.

use std::collections::HashMap;

use super::super::operation::parse_log_operations;
use super::super::{
    XfsLogError, XfsLogFormat, XfsLogRecord, XLOG_COMMIT_TRANS, XLOG_CONTINUE_TRANS,
    XLOG_END_TRANS, XLOG_UNMOUNT_TRANS, XLOG_WAS_CONT_TRANS,
};
use super::MAX_REPLAY_TRANSACTIONS;

/// `sizeof(struct xfs_trans_header)`: magic + type + tid + item count.
const TRANSACTION_HEADER_BYTES: usize = 16;
/// `XFS_TRANS_HEADER_MAGIC` ("TRAN"), in host order.
const TRANSACTION_HEADER_MAGIC: u32 = 0x5452_414E;
/// Sanity bound on the regions of a single item; real items have at most 5.
const MAX_REGIONS_PER_ITEM: u16 = 128;

pub(super) struct AssembledItem {
    pub(super) regions: Vec<Vec<u8>>,
}

pub(super) struct CommittedTransaction {
    /// LSN of the first record the transaction appeared in (the kernel's
    /// `r_lsn`, stamped into recovered v3 inode cores).
    pub(super) lsn: u64,
    pub(super) format: XfsLogFormat,
    pub(super) items: Vec<AssembledItem>,
}

pub(super) struct Assembly {
    pub(super) transactions: Vec<CommittedTransaction>,
}

struct ItemBuilder {
    declared_regions: u16,
    regions: Vec<Vec<u8>>,
}

impl ItemBuilder {
    fn placeholder() -> Self {
        Self {
            declared_regions: 0,
            regions: Vec::new(),
        }
    }

    fn is_complete(&self) -> bool {
        self.declared_regions != 0 && self.regions.len() == usize::from(self.declared_regions)
    }

    fn is_placeholder(&self) -> bool {
        self.declared_regions == 0 && self.regions.is_empty()
    }
}

struct TransactionBuilder {
    lsn: u64,
    format: XfsLogFormat,
    header: Vec<u8>,
    items: Vec<ItemBuilder>,
    poisoned: bool,
}

impl TransactionBuilder {
    fn new(lsn: u64, format: XfsLogFormat) -> Self {
        Self {
            lsn,
            format,
            header: Vec::new(),
            items: Vec::new(),
            poisoned: false,
        }
    }

    fn header_complete(&self) -> bool {
        !self.items.is_empty()
    }

    /// Accumulate transaction-header bytes until the fixed-size header is
    /// present, then push the placeholder item the first real region lands
    /// in — the same shape the kernel builds.
    fn extend_header(&mut self, region: &[u8]) {
        self.header.extend_from_slice(region);
        if self.header.len() >= 4 {
            let magic = self.format.native_u32(&self.header, 0);
            if magic != Some(TRANSACTION_HEADER_MAGIC) {
                self.poisoned = true;
                return;
            }
        }
        if self.header.len() > TRANSACTION_HEADER_BYTES {
            self.poisoned = true;
            return;
        }
        if self.header.len() == TRANSACTION_HEADER_BYTES {
            self.items.push(ItemBuilder::placeholder());
        }
    }

    fn add_region(&mut self, region: Vec<u8>) {
        if self.poisoned || region.is_empty() {
            return;
        }
        if !self.header_complete() {
            self.extend_header(&region);
            return;
        }
        if self.items.last().is_some_and(ItemBuilder::is_complete) {
            self.items.push(ItemBuilder::placeholder());
        }
        let Some(tail) = self.items.last_mut() else {
            self.poisoned = true;
            return;
        };
        if tail.declared_regions == 0 {
            if region.len() < 4 {
                self.poisoned = true;
                return;
            }
            let Some(size) = self.format.native_u16(&region, 2) else {
                self.poisoned = true;
                return;
            };
            if size == 0 || size > MAX_REGIONS_PER_ITEM {
                self.poisoned = true;
                return;
            }
            tail.declared_regions = size;
        }
        tail.regions.push(region);
    }

    fn append_continuation(&mut self, region: &[u8]) {
        if self.poisoned || region.is_empty() {
            return;
        }
        if !self.header_complete() {
            self.extend_header(region);
            return;
        }
        match self.items.last_mut() {
            Some(tail) if !tail.regions.is_empty() => {
                if let Some(last) = tail.regions.last_mut() {
                    last.extend_from_slice(region);
                }
            }
            _ => self.poisoned = true,
        }
    }

    fn finish(self) -> Result<Option<CommittedTransaction>, XfsLogError> {
        if self.poisoned {
            return Err(XfsLogError::InvalidData(
                "committed transaction contains malformed regions".to_string(),
            ));
        }
        let mut items = Vec::new();
        for item in self.items {
            if item.is_complete() {
                items.push(AssembledItem {
                    regions: item.regions,
                });
            } else if !item.is_placeholder() {
                return Err(XfsLogError::InvalidData(
                    "committed transaction contains an incomplete log item".to_string(),
                ));
            }
        }
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(CommittedTransaction {
            lsn: self.lsn,
            format: self.format,
            items,
        }))
    }
}

/// Rebuild every committed transaction from the records, which must already
/// be sorted by increasing `h_lsn` (`collect_log_records` guarantees this).
pub(super) fn assemble_committed(records: &[XfsLogRecord]) -> Result<Assembly, XfsLogError> {
    let mut builders: HashMap<u32, TransactionBuilder> = HashMap::new();
    let mut transactions = Vec::new();
    for record in records {
        for operation in parse_log_operations(record)? {
            step(&mut builders, &mut transactions, operation)?;
        }
        if transactions.len() > MAX_REPLAY_TRANSACTIONS {
            return Err(XfsLogError::InvalidData(format!(
                "committed transaction count exceeds the {MAX_REPLAY_TRANSACTIONS} replay cap"
            )));
        }
    }
    Ok(Assembly { transactions })
}

fn step(
    builders: &mut HashMap<u32, TransactionBuilder>,
    transactions: &mut Vec<CommittedTransaction>,
    operation: super::super::XfsLogOperation,
) -> Result<(), XfsLogError> {
    let tid = operation.transaction_id;
    let Some(mut builder) = builders.remove(&tid) else {
        if operation.flags.starts_transaction() {
            builders.insert(
                tid,
                TransactionBuilder::new(operation.record_lsn, operation.record_format),
            );
        }
        return Ok(());
    };
    let mut flags = operation.flags.bits() & !XLOG_END_TRANS;
    if flags & XLOG_WAS_CONT_TRANS != 0 {
        flags &= !XLOG_CONTINUE_TRANS;
    }
    match flags {
        XLOG_COMMIT_TRANS => {
            let transaction = builder.finish()?;
            if let Some(transaction) = transaction {
                transactions.push(transaction);
            }
            return Ok(());
        }
        XLOG_UNMOUNT_TRANS => return Ok(()),
        XLOG_WAS_CONT_TRANS => builder.append_continuation(&operation.region),
        0 | XLOG_CONTINUE_TRANS => builder.add_region(operation.region),
        _ => return Ok(()),
    }
    builders.insert(tid, builder);
    Ok(())
}
