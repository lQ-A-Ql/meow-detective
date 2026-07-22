use super::inode_item::{parse_inode_log_format, parse_logged_inode_core, XfsInodeLogFormat};
use super::{
    XfsLogChecksumStatus, XfsLogClient, XfsLogFormat, XfsLogIssue, XfsLogIssueKind,
    XfsLogOperation, XLOG_COMMIT_TRANS, XLOG_START_TRANS,
};
use std::collections::BTreeMap;

mod parser;
mod types;

use parser::{parse_item_header, parse_metadata_candidate, parse_transaction_header};

pub use types::{
    XfsDeletedFileCandidate, XfsDeletionProof, XfsDeletionStatus, XfsLogTransaction,
    XfsMetadataCandidate, XfsMetadataCandidateKind, XfsRecoveryCompleteness, XfsTransactionHeader,
    XFS_LI_ATTRD, XFS_LI_ATTRI, XFS_LI_BUD, XFS_LI_BUF, XFS_LI_BUI, XFS_LI_CUD, XFS_LI_CUI,
    XFS_LI_DQUOT, XFS_LI_EFD, XFS_LI_EFI, XFS_LI_ICREATE, XFS_LI_INODE, XFS_LI_IUNLINK,
    XFS_LI_QUOTAOFF, XFS_LI_RUD, XFS_LI_RUI, XFS_LI_XMD, XFS_LI_XMI,
};

const XFS_TRANS_CHECKPOINT: u32 = 40;
const MAX_REASSEMBLED_REGION_BYTES: usize = 1024 * 1024;

struct PendingTransaction {
    summary: XfsLogTransaction,
    candidates: Vec<XfsMetadataCandidate>,
    deleted_candidates: Vec<XfsDeletedFileCandidate>,
    pending_item: Option<PendingLogItem>,
    completed_item_count: u32,
    invalid_reason: Option<(u32, String)>,
    partial_region: Vec<u8>,
    partial_format: Option<XfsLogFormat>,
    partial_origin: Option<RegionOrigin>,
}

struct PendingLogItem {
    expected_regions: u16,
    observed_regions: u16,
    format: XfsLogFormat,
    inode: Option<PendingInodeItem>,
}

struct PendingInodeItem {
    descriptor: XfsInodeLogFormat,
    origin: RegionOrigin,
}

#[derive(Clone, Copy)]
struct RegionOrigin {
    transaction_id: u32,
    record_lsn: u64,
    record_log_block: u32,
    record_source_offset: u64,
    record_provenance: super::XfsLogRecordProvenance,
    record_checksum_status: XfsLogChecksumStatus,
    record_format: XfsLogFormat,
    operation_index: u32,
}

impl From<&XfsLogOperation> for RegionOrigin {
    fn from(operation: &XfsLogOperation) -> Self {
        Self {
            transaction_id: operation.transaction_id,
            record_lsn: operation.record_lsn,
            record_log_block: operation.record_log_block,
            record_source_offset: operation.record_source_offset,
            record_provenance: operation.record_provenance,
            record_checksum_status: operation.record_checksum_status,
            record_format: operation.record_format,
            operation_index: operation.operation_index,
        }
    }
}

pub(crate) fn assemble_transactions<'a>(
    operations: impl IntoIterator<Item = &'a XfsLogOperation>,
) -> (
    Vec<XfsLogTransaction>,
    Vec<XfsMetadataCandidate>,
    Vec<XfsDeletedFileCandidate>,
    Vec<XfsLogIssue>,
) {
    let mut active = BTreeMap::<(u32, XfsLogClient), PendingTransaction>::new();
    let mut transactions = Vec::new();
    let mut candidates = Vec::new();
    let mut deleted_candidates = Vec::new();
    let mut issues = Vec::new();

    for operation in operations {
        let key = (operation.transaction_id, operation.client);
        if operation.flags.contains(XLOG_START_TRANS) {
            if let Some(previous) = active.remove(&key) {
                finalize(
                    previous,
                    &mut transactions,
                    &mut candidates,
                    &mut deleted_candidates,
                    &mut issues,
                );
            }
            active.insert(key, PendingTransaction::new(operation, true));
        }
        let pending = active
            .entry(key)
            .or_insert_with(|| PendingTransaction::new(operation, false));
        pending.observe(operation);

        if operation.flags.contains(XLOG_COMMIT_TRANS) {
            if let Some(mut completed) = active.remove(&key) {
                completed.summary.committed = true;
                finalize(
                    completed,
                    &mut transactions,
                    &mut candidates,
                    &mut deleted_candidates,
                    &mut issues,
                );
            }
        }
    }
    for pending in active.into_values() {
        finalize(
            pending,
            &mut transactions,
            &mut candidates,
            &mut deleted_candidates,
            &mut issues,
        );
    }
    transactions.sort_by_key(|transaction| transaction.first_lsn);
    candidates.sort_by_key(|candidate| (candidate.record_lsn, candidate.transaction_id));
    deleted_candidates.sort_by_key(|candidate| (candidate.record_lsn, candidate.inode));
    (transactions, candidates, deleted_candidates, issues)
}

impl PendingTransaction {
    fn new(operation: &XfsLogOperation, started: bool) -> Self {
        Self {
            summary: XfsLogTransaction {
                transaction_id: operation.transaction_id,
                client: operation.client,
                first_lsn: operation.record_lsn,
                last_lsn: operation.record_lsn,
                started,
                committed: false,
                operation_count: 0,
                region_count: 0,
                item_region_count: 0,
                header: None,
            },
            candidates: Vec::new(),
            deleted_candidates: Vec::new(),
            pending_item: None,
            completed_item_count: 0,
            invalid_reason: None,
            partial_region: Vec::new(),
            partial_format: None,
            partial_origin: None,
        }
    }

    fn observe(&mut self, operation: &XfsLogOperation) {
        self.summary.last_lsn = self.summary.last_lsn.max(operation.record_lsn);
        self.summary.operation_count = self.summary.operation_count.saturating_add(1);
        if operation.region.is_empty() {
            return;
        }
        let origin = RegionOrigin::from(operation);
        let was_continued = operation.flags.contains(super::XLOG_WAS_CONT_TRANS);
        if !was_continued {
            if !self.partial_region.is_empty() {
                self.invalidate(origin, "an unfinished continued region was replaced");
                self.clear_partial_region();
                return;
            }
            self.partial_region.clear();
            self.partial_format = Some(operation.record_format);
            self.partial_origin = Some(origin);
        } else if self.partial_region.is_empty() {
            self.invalidate(origin, "continued region has no preceding fragment");
            return;
        }
        if self.partial_format != Some(operation.record_format)
            || self
                .partial_region
                .len()
                .saturating_add(operation.region.len())
                > MAX_REASSEMBLED_REGION_BYTES
        {
            self.invalidate(
                origin,
                "continued region changed byte order or exceeded the 1 MiB bound",
            );
            self.clear_partial_region();
            return;
        }
        self.partial_region.extend_from_slice(&operation.region);
        if operation.flags.contains(super::XLOG_CONTINUE_TRANS) {
            return;
        }

        let region = std::mem::take(&mut self.partial_region);
        let Some(origin) = self.partial_origin.take() else {
            self.invalidate(origin, "completed region has no source provenance");
            self.clear_partial_region();
            return;
        };
        self.partial_format = None;
        self.summary.region_count = self.summary.region_count.saturating_add(1);
        if self.invalid_reason.is_some() {
            return;
        }
        if self.summary.header.is_none() {
            match parse_transaction_header(origin.record_format, &region) {
                Some(header) => self.summary.header = Some(header),
                None => self.invalidate(origin, "transaction header is missing or invalid"),
            }
            return;
        }
        self.summary.item_region_count = self.summary.item_region_count.saturating_add(1);
        self.observe_item_region(origin, &region);
    }

    fn observe_item_region(&mut self, origin: RegionOrigin, region: &[u8]) {
        if let Some(mut item) = self.pending_item.take() {
            if item.format != origin.record_format {
                self.invalidate(origin, "log item regions use inconsistent byte order");
                return;
            }
            item.observed_regions = item.observed_regions.saturating_add(1);
            if item.observed_regions == 2 {
                if let Some(inode) = item.inode.as_ref() {
                    if let Err(error) = self.observe_inode_core(inode, origin, region) {
                        self.invalidate(origin, error.to_string());
                        return;
                    }
                }
            }
            if item.observed_regions < item.expected_regions {
                self.pending_item = Some(item);
            } else {
                self.completed_item_count = self.completed_item_count.saturating_add(1);
            }
            return;
        }

        self.start_item(origin, region);
    }

    fn start_item(&mut self, origin: RegionOrigin, region: &[u8]) {
        let Some((item_type, expected_regions)) = parse_item_header(origin.record_format, region)
        else {
            self.invalidate(origin, "log item descriptor has an invalid region count");
            return;
        };
        let inode = if item_type == XFS_LI_INODE {
            match parse_inode_log_format(origin.record_format, region) {
                Ok(descriptor) => Some(PendingInodeItem { descriptor, origin }),
                Err(error) => {
                    self.invalidate(origin, error.to_string());
                    return;
                }
            }
        } else {
            None
        };
        if let Some(candidate) = parse_metadata_candidate(origin, region) {
            self.candidates.push(candidate);
        }
        if expected_regions == 1 {
            self.completed_item_count = self.completed_item_count.saturating_add(1);
        } else {
            self.pending_item = Some(PendingLogItem {
                expected_regions,
                observed_regions: 1,
                format: origin.record_format,
                inode,
            });
        }
    }

    fn observe_inode_core(
        &mut self,
        inode: &PendingInodeItem,
        core_origin: RegionOrigin,
        region: &[u8],
    ) -> Result<(), super::XfsLogError> {
        let core = parse_logged_inode_core(inode.origin.record_format, region)?;
        if core
            .inode
            .is_some_and(|identity| identity != inode.descriptor.inode)
        {
            return Err(super::XfsLogError::InvalidData(
                "logged v3 inode identity does not match its inode log format".into(),
            ));
        }
        if core.link_count == 0 {
            self.deleted_candidates.push(XfsDeletedFileCandidate {
                inode: inode.descriptor.inode,
                record_lsn: inode.origin.record_lsn,
                record_log_block: inode.origin.record_log_block,
                record_source_offset: inode.origin.record_source_offset,
                operation_index: inode.origin.operation_index,
                provenance: merged_record_provenance(inode.origin, core_origin),
                proof: XfsDeletionProof::InodeCoreNlinkZero,
                completeness: XfsRecoveryCompleteness::MetadataOnly,
            });
        }
        Ok(())
    }

    fn invalidate(&mut self, origin: RegionOrigin, reason: impl Into<String>) {
        if self.invalid_reason.is_none() {
            self.invalid_reason = Some((origin.record_log_block, reason.into()));
        }
        self.pending_item = None;
        self.deleted_candidates.clear();
    }

    fn clear_partial_region(&mut self) {
        self.partial_region.clear();
        self.partial_format = None;
        self.partial_origin = None;
    }
}

fn merged_record_provenance(
    descriptor: RegionOrigin,
    core: RegionOrigin,
) -> Vec<super::XfsLogSourceSpan> {
    let mut spans = descriptor
        .record_provenance
        .spans()
        .chain(core.record_provenance.spans())
        .collect::<Vec<_>>();
    spans.sort_by_key(|span| (span.source_offset, span.length));
    spans.dedup();
    spans
}

fn finalize(
    pending: PendingTransaction,
    transactions: &mut Vec<XfsLogTransaction>,
    candidates: &mut Vec<XfsMetadataCandidate>,
    deleted_candidates: &mut Vec<XfsDeletedFileCandidate>,
    issues: &mut Vec<XfsLogIssue>,
) {
    let structural_error = committed_transaction_error(&pending);
    let committed = pending.summary.committed;
    let structurally_committed = committed && structural_error.is_none();
    candidates.extend(pending.candidates.into_iter().map(|mut candidate| {
        candidate.transaction_committed = structurally_committed;
        candidate
    }));
    if structurally_committed {
        deleted_candidates.extend(pending.deleted_candidates);
    }
    if let Some((log_block, message)) = structural_error {
        issues.push(XfsLogIssue::new(
            XfsLogIssueKind::InvalidOperation,
            Some(u64::from(log_block)),
            message,
        ));
    }
    transactions.push(pending.summary);
}

fn committed_transaction_error(pending: &PendingTransaction) -> Option<(u32, String)> {
    if !pending.summary.committed {
        return None;
    }
    if let Some(error) = pending.invalid_reason.clone() {
        return Some(error);
    }
    let log_block = (pending.summary.first_lsn & u64::from(u32::MAX)) as u32;
    if !pending.summary.started {
        return Some((
            log_block,
            "committed transaction has no observed start".into(),
        ));
    }
    let Some(header) = pending.summary.header.as_ref() else {
        return Some((
            log_block,
            "committed transaction has no valid header".into(),
        ));
    };
    if !pending.partial_region.is_empty() || pending.pending_item.is_some() {
        return Some((
            log_block,
            "committed transaction contains an incomplete log item".into(),
        ));
    }
    if header.transaction_type != XFS_TRANS_CHECKPOINT {
        return Some((
            log_block,
            format!(
                "transaction type {} has unverified th_num_items semantics",
                header.transaction_type
            ),
        ));
    }
    if header.item_count != pending.summary.item_region_count {
        return Some((
            log_block,
            format!(
                "checkpoint header declares {} item regions, but {} complete item regions were reassembled across {} logical items",
                header.item_count, pending.summary.item_region_count, pending.completed_item_count
            ),
        ));
    }
    None
}
