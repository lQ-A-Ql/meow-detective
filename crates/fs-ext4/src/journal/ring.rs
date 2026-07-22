use super::commit::parse_commit_block;
use super::descriptor::{parse_descriptor_block, verify_payload_checksum};
use super::error::{require_len, JournalError, JournalResult};
use super::revoke::parse_revoke_block;
use super::types::{
    IncompleteTransaction, JournalBlockMapping, JournalBlockType, JournalCommit, JournalDescriptor,
    JournalHeader, JournalHistoryScan, JournalRevoke, JournalScan, JournalScanIssue,
    JournalSuperblock, JournalTransaction, JBD2_MAGIC_NUMBER,
};
use std::collections::{hash_map::Entry, HashMap};

pub fn parse_journal(journal_data: &[u8]) -> JournalResult<JournalScan> {
    let superblock = JournalSuperblock::parse(journal_data)?;
    let view = JournalView::new(journal_data, &superblock)?;
    if superblock.start == 0 {
        return Ok(empty_active_scan(superblock));
    }
    let ring_capacity = view.capacity();
    let mut current = superblock.start;
    let mut expected_sequence = superblock.sequence;
    let mut scanned = 0u32;
    let mut transactions = Vec::new();
    let mut incomplete_transaction = None;

    while scanned < ring_capacity {
        let header =
            active_transaction_header(&view, current, expected_sequence, transactions.is_empty())?;
        let Some(header) = header else {
            break;
        };
        let attempt = scan_transaction(
            &view,
            &superblock,
            current,
            header.sequence,
            ring_capacity - scanned,
        )?;
        scanned += attempt.consumed;
        current = attempt.next_block;
        if let Some(transaction) = attempt.transaction {
            transactions.push(transaction);
            expected_sequence = expected_sequence.wrapping_add(1);
            continue;
        }
        incomplete_transaction = Some(IncompleteTransaction {
            sequence: expected_sequence,
            start_journal_block: attempt.start_block,
            stopped_at_journal_block: current,
            reason: attempt.reason,
        });
        break;
    }

    apply_revoke_semantics(&mut transactions);
    Ok(JournalScan {
        superblock,
        transactions,
        incomplete_transaction,
        scanned_ring_blocks: scanned,
        next_journal_block: current,
    })
}

/// Scans committed historical transactions even when `s_start == 0`.
/// Structurally invalid candidate chains are reported and never published.
pub fn parse_journal_history(journal_data: &[u8]) -> JournalResult<JournalHistoryScan> {
    let superblock = JournalSuperblock::parse(journal_data)?;
    let view = JournalView::new(journal_data, &superblock)?;
    let mut candidates = HashMap::<u32, (usize, u32, JournalTransaction)>::new();
    let mut rejected_candidates = Vec::new();

    for block in view.first..view.last {
        let data = view.block(block)?;
        if !has_journal_magic(data) {
            continue;
        }
        let header = match JournalHeader::parse(data) {
            Ok(header) => header,
            Err(error) => {
                rejected_candidates.push(scan_issue(block, None, error.to_string()));
                continue;
            }
        };
        if !matches!(
            header.block_type,
            JournalBlockType::Descriptor | JournalBlockType::Revoke
        ) {
            continue;
        }
        collect_history_candidate(
            &view,
            &superblock,
            block,
            header.sequence,
            &mut candidates,
            &mut rejected_candidates,
        );
    }

    let mut transactions = candidates
        .into_values()
        .map(|(_, _, transaction)| transaction)
        .collect::<Vec<_>>();
    sort_transactions_by_sequence(&mut transactions);
    apply_revoke_semantics(&mut transactions);
    Ok(JournalHistoryScan {
        superblock,
        transactions,
        rejected_candidates,
        scanned_ring_blocks: view.capacity(),
    })
}

pub(crate) fn journal_block_data<'a>(
    journal_data: &'a [u8],
    superblock: &JournalSuperblock,
    block: u32,
) -> JournalResult<&'a [u8]> {
    JournalView::new(journal_data, superblock)?.block(block)
}

fn collect_history_candidate(
    view: &JournalView<'_>,
    superblock: &JournalSuperblock,
    block: u32,
    sequence: u32,
    candidates: &mut HashMap<u32, (usize, u32, JournalTransaction)>,
    rejected: &mut Vec<JournalScanIssue>,
) {
    let attempt = match scan_transaction(view, superblock, block, sequence, view.capacity()) {
        Ok(attempt) => attempt,
        Err(error) => {
            rejected.push(scan_issue(block, Some(sequence), error.to_string()));
            return;
        }
    };
    let Some(transaction) = attempt.transaction else {
        rejected.push(scan_issue(block, Some(sequence), attempt.reason));
        return;
    };
    let score =
        transaction.mappings.len() + transaction.descriptors.len() + transaction.revokes.len();
    match candidates.entry(sequence) {
        Entry::Vacant(entry) => {
            entry.insert((score, attempt.consumed, transaction));
        }
        Entry::Occupied(mut entry) => {
            let existing = entry.get();
            if (score, attempt.consumed) > (existing.0, existing.1) {
                entry.insert((score, attempt.consumed, transaction));
            }
        }
    }
}

fn scan_transaction(
    view: &JournalView<'_>,
    superblock: &JournalSuperblock,
    start_block: u32,
    sequence: u32,
    budget: u32,
) -> JournalResult<TransactionAttempt> {
    let mut state = TransactionState::new(start_block, sequence);
    while state.consumed < budget {
        let control_data = view.block(state.current)?;
        if !has_journal_magic(control_data) {
            return Ok(state.incomplete("transaction ended before its commit block"));
        }
        let header = JournalHeader::parse(control_data)?;
        if header.sequence != sequence {
            return Ok(state.incomplete(format!(
                "encountered sequence {} before transaction {} committed",
                header.sequence, sequence
            )));
        }
        match header.block_type {
            JournalBlockType::Descriptor => {
                if let Some(reason) = consume_descriptor(view, superblock, &mut state, budget)? {
                    return Ok(state.incomplete(reason));
                }
            }
            JournalBlockType::Revoke => consume_revoke(view, superblock, &mut state)?,
            JournalBlockType::Commit => {
                return complete_transaction(view, superblock, state, control_data)
            }
            JournalBlockType::SuperblockV1 | JournalBlockType::SuperblockV2 => {
                return Ok(state.incomplete("superblock encountered inside a transaction"));
            }
        }
    }
    Ok(state.incomplete("transaction exhausted the journal ring"))
}

fn consume_descriptor(
    view: &JournalView<'_>,
    superblock: &JournalSuperblock,
    state: &mut TransactionState,
    budget: u32,
) -> JournalResult<Option<String>> {
    let descriptor_block = state.current;
    let descriptor = parse_descriptor_block(view.block(descriptor_block)?, superblock)?;
    state.advance(view);
    for tag in &descriptor.tags {
        if state.consumed >= budget {
            state.descriptors.push(JournalDescriptor {
                journal_block: descriptor_block,
                descriptor,
            });
            return Ok(Some(
                "descriptor payload crosses the entire journal ring".to_string(),
            ));
        }
        let payload = view.block(state.current)?;
        verify_payload_checksum(superblock, state.sequence, tag, payload)?;
        state.mappings.push(JournalBlockMapping {
            transaction_sequence: state.sequence,
            descriptor_journal_block: descriptor_block,
            payload_journal_block: state.current,
            target_filesystem_block: tag.target_block,
            flags: tag.flags,
            uuid: tag.uuid,
            checksum: tag.checksum,
            revoked: false,
        });
        state.advance(view);
    }
    state.descriptors.push(JournalDescriptor {
        journal_block: descriptor_block,
        descriptor,
    });
    Ok(None)
}

fn consume_revoke(
    view: &JournalView<'_>,
    superblock: &JournalSuperblock,
    state: &mut TransactionState,
) -> JournalResult<()> {
    let journal_block = state.current;
    let revoke = parse_revoke_block(view.block(journal_block)?, superblock)?;
    state.revokes.push(JournalRevoke {
        journal_block,
        revoke,
    });
    state.advance(view);
    Ok(())
}

fn complete_transaction(
    view: &JournalView<'_>,
    superblock: &JournalSuperblock,
    mut state: TransactionState,
    control_data: &[u8],
) -> JournalResult<TransactionAttempt> {
    let journal_block = state.current;
    let commit = parse_commit_block(control_data, superblock)?;
    state.advance(view);
    let transaction = JournalTransaction {
        sequence: state.sequence,
        start_journal_block: state.start_block,
        next_journal_block: state.current,
        descriptors: state.descriptors,
        mappings: state.mappings,
        revokes: state.revokes,
        commit: JournalCommit {
            journal_block,
            commit,
        },
    };
    Ok(TransactionAttempt {
        start_block: transaction.start_journal_block,
        next_block: state.current,
        consumed: state.consumed,
        transaction: Some(transaction),
        reason: String::new(),
    })
}

fn active_transaction_header(
    view: &JournalView<'_>,
    block: u32,
    expected_sequence: u32,
    first_transaction: bool,
) -> JournalResult<Option<JournalHeader>> {
    let data = view.block(block)?;
    if !has_journal_magic(data) {
        if first_transaction {
            return Err(JournalError::Invalid(format!(
                "journal start block {block} has no JBD2 header"
            )));
        }
        return Ok(None);
    }
    let header = JournalHeader::parse(data)?;
    if header.sequence != expected_sequence {
        if first_transaction {
            return Err(JournalError::Invalid(format!(
                "journal starts at sequence {}, expected {}",
                header.sequence, expected_sequence
            )));
        }
        return Ok(None);
    }
    Ok(Some(header))
}

fn apply_revoke_semantics(transactions: &mut [JournalTransaction]) {
    let mut latest_revoke = HashMap::<u64, usize>::new();
    for (transaction_index, transaction) in transactions.iter().enumerate() {
        for block in transaction
            .revokes
            .iter()
            .flat_map(|record| record.revoke.revoked_blocks.iter())
        {
            latest_revoke.insert(*block, transaction_index);
        }
    }
    for (transaction_index, transaction) in transactions.iter_mut().enumerate() {
        for mapping in &mut transaction.mappings {
            mapping.revoked = latest_revoke
                .get(&mapping.target_filesystem_block)
                .is_some_and(|revoke_index| transaction_index <= *revoke_index);
        }
    }
}

fn sort_transactions_by_sequence(transactions: &mut [JournalTransaction]) {
    let Some(first) = transactions.first() else {
        return;
    };
    let mut oldest = first.sequence;
    for transaction in transactions.iter().skip(1) {
        if sequence_is_after(oldest, transaction.sequence) {
            oldest = transaction.sequence;
        }
    }
    transactions.sort_by_key(|transaction| transaction.sequence.wrapping_sub(oldest));
}

fn sequence_is_after(left: u32, right: u32) -> bool {
    left.wrapping_sub(right) as i32 > 0
}

fn scan_issue(journal_block: u32, sequence: Option<u32>, reason: String) -> JournalScanIssue {
    JournalScanIssue {
        journal_block,
        sequence,
        reason,
    }
}

fn empty_active_scan(superblock: JournalSuperblock) -> JournalScan {
    JournalScan {
        next_journal_block: superblock.first,
        superblock,
        transactions: Vec::new(),
        incomplete_transaction: None,
        scanned_ring_blocks: 0,
    }
}

fn has_journal_magic(data: &[u8]) -> bool {
    data.get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_be_bytes)
        == Some(JBD2_MAGIC_NUMBER)
}

struct TransactionState {
    sequence: u32,
    start_block: u32,
    current: u32,
    consumed: u32,
    descriptors: Vec<JournalDescriptor>,
    mappings: Vec<JournalBlockMapping>,
    revokes: Vec<JournalRevoke>,
}

impl TransactionState {
    fn new(start_block: u32, sequence: u32) -> Self {
        Self {
            sequence,
            start_block,
            current: start_block,
            consumed: 0,
            descriptors: Vec::new(),
            mappings: Vec::new(),
            revokes: Vec::new(),
        }
    }

    fn advance(&mut self, view: &JournalView<'_>) {
        self.current = view.next(self.current);
        self.consumed += 1;
    }

    fn incomplete(self, reason: impl Into<String>) -> TransactionAttempt {
        TransactionAttempt {
            start_block: self.start_block,
            next_block: self.current,
            consumed: self.consumed,
            transaction: None,
            reason: reason.into(),
        }
    }
}

struct TransactionAttempt {
    start_block: u32,
    next_block: u32,
    consumed: u32,
    transaction: Option<JournalTransaction>,
    reason: String,
}

struct JournalView<'a> {
    data: &'a [u8],
    block_size: usize,
    first: u32,
    last: u32,
}

impl<'a> JournalView<'a> {
    fn new(data: &'a [u8], superblock: &JournalSuperblock) -> JournalResult<Self> {
        let block_size = superblock.block_size as usize;
        let required = usize::try_from(
            u64::from(superblock.max_len)
                .checked_mul(u64::from(superblock.block_size))
                .ok_or_else(|| JournalError::Invalid("journal byte length overflows".into()))?,
        )
        .map_err(|_| JournalError::Unsupported("journal exceeds addressable memory".into()))?;
        require_len(data, required, "journal snapshot")?;
        Ok(Self {
            data: &data[..required],
            block_size,
            first: superblock.first,
            last: superblock.log_last_exclusive()?,
        })
    }

    fn capacity(&self) -> u32 {
        self.last - self.first
    }

    fn block(&self, block: u32) -> JournalResult<&'a [u8]> {
        if block != 0 && (block < self.first || block >= self.last) {
            return Err(JournalError::Invalid(format!(
                "journal block {block} is outside normal log ring"
            )));
        }
        let start = (block as usize)
            .checked_mul(self.block_size)
            .ok_or_else(|| JournalError::Invalid("journal block offset overflows".into()))?;
        let end = start
            .checked_add(self.block_size)
            .ok_or_else(|| JournalError::Invalid("journal block end overflows".into()))?;
        require_len(self.data, end, "journal block")?;
        Ok(&self.data[start..end])
    }

    fn next(&self, block: u32) -> u32 {
        let next = block + 1;
        if next >= self.last {
            self.first
        } else {
            next
        }
    }
}
