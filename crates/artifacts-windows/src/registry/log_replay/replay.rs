use super::{
    header::{checksum, u32_at},
    types::failure,
    LogDisposition, LogFormat, LogReplayError, LogReplayFailure, LogReplayPlan, RecoveryState,
    ReplayedHive,
};
use crate::registry::browser::HiveSnapshot;
use std::sync::Arc;

impl LogReplayPlan<'_> {
    pub fn apply(mut self, mut bytes: Vec<u8>) -> Result<ReplayedHive, LogReplayFailure> {
        if bytes.len() < 4096 || bytes.get(..512) != Some(self.base.bytes.as_slice()) {
            return Err(failure(LogReplayError::Header, self.report));
        }
        if self.report.state == RecoveryState::NotNeeded {
            return Ok(ReplayedHive {
                bytes,
                report: self.report,
                provenance: Vec::new(),
                allocation_map: None,
            });
        }
        let mut expected = None;
        let mut last = None;
        let mut provenance = Vec::new();
        let mut header = self.base.bytes;
        for index in &self.order {
            let log = &self.logs[*index];
            if expected.is_some_and(|seq| seq != log.header.primary) {
                self.report.diagnostics.push(LogReplayError::Sequence);
                break;
            }
            if !self.base.checksum_valid {
                header = log.header.bytes;
                bytes[512..4096].fill(0);
            }
            header[144..148].copy_from_slice(&log.header.bytes[144..148]);
            for record in &log.records {
                let length = 4096 + record.bins;
                if bytes.len() < length {
                    bytes.resize(length, 0);
                }
                for page in &record.pages {
                    let at = 4096 + page.offset;
                    bytes[at..at + page.length]
                        .copy_from_slice(&log.data[page.data_start..page.data_start + page.length]);
                    provenance.push(crate::registry::browser::ProvenanceSpan {
                        start: at,
                        end: at + page.length,
                    });
                }
                header[40..44].copy_from_slice(&(record.bins as u32).to_le_bytes());
                if log.format == LogFormat::Hvle {
                    let flags =
                        u32_at(&header, 144).map_err(|e| failure(e, self.report.clone()))?;
                    header[144..148]
                        .copy_from_slice(&((flags & !1) | (record.flags & 1)).to_le_bytes());
                }
                self.report.applied_entries += 1;
                self.report.applied_pages += record.pages.len() as u32;
                let item = self
                    .report
                    .logs
                    .iter_mut()
                    .find(|r| r.slot == log.slot)
                    .expect("known log");
                item.disposition = LogDisposition::Applied;
                item.applied_entries += 1;
                item.applied_pages += record.pages.len() as u32;
                last = Some(record.sequence);
                expected = Some(record.sequence.wrapping_add(1));
            }
            if let Some(error) = log.tail {
                self.report.diagnostics.push(error);
            }
        }
        let sequence = last.ok_or_else(|| failure(LogReplayError::NoLog, self.report.clone()))?;
        let sequence =
            if self.logs[self.order[0]].format == LogFormat::Dirt && self.base.checksum_valid {
                self.base.primary
            } else {
                sequence
            };
        header[4..8].copy_from_slice(&sequence.to_le_bytes());
        header[8..12].copy_from_slice(&sequence.to_le_bytes());
        header[28..32].fill(0);
        let checksum = checksum(&header);
        header[508..512].copy_from_slice(&checksum.to_le_bytes());
        bytes[..512].copy_from_slice(&header);
        // Validate a compact allocated-cell map before exposing a derived view.
        let shared = Arc::new(bytes);
        let allocation_map = Some(validated_allocation_map(&shared, &self.report)?);
        let bytes = Arc::try_unwrap(shared).expect("validation lease dropped");
        self.report.result_sequence = Some(sequence);
        self.report.state = if self.report.diagnostics.is_empty() {
            RecoveryState::Recovered
        } else {
            RecoveryState::Partial
        };
        Ok(ReplayedHive {
            bytes,
            report: self.report,
            provenance,
            allocation_map,
        })
    }
}

fn validated_allocation_map(
    bytes: &Arc<Vec<u8>>,
    report: &super::LogReplayReport,
) -> Result<Arc<Vec<u64>>, LogReplayFailure> {
    let hive = HiveSnapshot::open(bytes.clone())
        .map_err(|_| failure(LogReplayError::InvalidHive, report.clone()))?;
    if hive.is_partial() {
        return Err(failure(LogReplayError::InvalidHive, report.clone()));
    }
    Ok(hive.cells.clone())
}
