use super::operation::parse_log_operations;
use super::record::collect_log_records;
use super::transaction::assemble_transactions;
use super::{
    XfsDeletedFileCandidate, XfsLogError, XfsLogIssue, XfsLogIssueKind, XfsLogOperation,
    XfsLogRecord, XfsLogSnapshot, XfsLogTransaction, XfsMetadataCandidate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XfsLogParseLimits {
    pub max_records: usize,
    pub max_operations: usize,
}

impl Default for XfsLogParseLimits {
    fn default() -> Self {
        Self {
            max_records: 4096,
            max_operations: 262_144,
        }
    }
}

#[derive(Debug, Clone)]
pub struct XfsParsedLogRecord {
    pub record: XfsLogRecord,
    pub operations: Vec<XfsLogOperation>,
}

#[derive(Debug, Clone)]
pub struct XfsLogAnalysis {
    pub records: Vec<XfsParsedLogRecord>,
    pub transactions: Vec<XfsLogTransaction>,
    pub metadata_candidates: Vec<XfsMetadataCandidate>,
    pub deleted_file_candidates: Vec<XfsDeletedFileCandidate>,
    pub issues: Vec<XfsLogIssue>,
}

pub fn analyze_log_snapshot(
    snapshot: &XfsLogSnapshot,
    limits: XfsLogParseLimits,
) -> Result<XfsLogAnalysis, XfsLogError> {
    if limits.max_records == 0 || limits.max_operations == 0 {
        return Err(XfsLogError::InvalidGeometry(
            "log parse limits must be non-zero".into(),
        ));
    }

    let collection = collect_log_records(snapshot, limits.max_records)?;
    let mut parsed_records = Vec::with_capacity(collection.records.len());
    let mut issues = collection.issues;
    let mut operation_count = 0usize;

    for record in collection.records {
        let log_block = u64::from(record.log_block);
        let operations = match parse_log_operations(&record) {
            Ok(operations) => operations,
            Err(error) => {
                issues.push(XfsLogIssue::new(
                    XfsLogIssueKind::InvalidOperation,
                    Some(log_block),
                    error.to_string(),
                ));
                Vec::new()
            }
        };
        if operation_count.saturating_add(operations.len()) > limits.max_operations {
            issues.push(XfsLogIssue::new(
                XfsLogIssueKind::LimitReached,
                Some(log_block),
                format!("operation limit {} reached", limits.max_operations),
            ));
            parsed_records.push(XfsParsedLogRecord {
                record,
                operations: Vec::new(),
            });
            break;
        }
        operation_count += operations.len();
        parsed_records.push(XfsParsedLogRecord { record, operations });
    }

    let (transactions, metadata_candidates, deleted_file_candidates, transaction_issues) =
        assemble_transactions(
            parsed_records
                .iter()
                .flat_map(|record| record.operations.iter()),
        );
    issues.extend(transaction_issues);
    if deleted_file_candidates.is_empty() {
        issues.push(XfsLogIssue::new(
            XfsLogIssueKind::DeletionEvidenceUnavailable,
            None,
            "no committed, complete XFS inode log item proved both inode identity and nlink=0; ordinary metadata and IUNLINK updates were not promoted",
        ));
    }
    Ok(XfsLogAnalysis {
        records: parsed_records,
        transactions,
        metadata_candidates,
        deleted_file_candidates,
        issues,
    })
}
