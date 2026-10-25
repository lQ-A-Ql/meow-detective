use super::{BrowserError, BrowserOverlaySource, HiveSnapshot, ProvenanceSpan};

pub(super) fn normalize(
    mut spans: Vec<ProvenanceSpan>,
    length: usize,
) -> Result<Vec<ProvenanceSpan>, BrowserError> {
    spans.sort_unstable_by_key(|span| span.start);
    let mut used = 0;
    for index in 0..spans.len() {
        let span = spans[index];
        if span.start >= span.end || span.end > length {
            return Err(BrowserError::Corrupt);
        }
        if used > 0 && span.start <= spans[used - 1].end {
            spans[used - 1].end = spans[used - 1].end.max(span.end);
        } else {
            spans[used] = span;
            used += 1;
        }
    }
    spans.truncate(used);
    spans.shrink_to_fit();
    Ok(spans)
}

impl HiveSnapshot {
    pub(crate) fn overlay_for_range(&self, start: usize, end: usize) -> BrowserOverlaySource {
        if start >= end {
            return BrowserOverlaySource::Base;
        }
        let next = self.provenance.partition_point(|span| span.end <= start);
        let Some(span) = self.provenance.get(next).filter(|span| span.start < end) else {
            return BrowserOverlaySource::Base;
        };
        if span.start <= start && end <= span.end {
            BrowserOverlaySource::Recovered
        } else {
            BrowserOverlaySource::Merged
        }
    }

    pub(crate) fn overlay_for_cell(&self, offset: u32, length: usize) -> BrowserOverlaySource {
        let start = 4096usize.saturating_add(offset as usize);
        self.overlay_for_range(start, start.saturating_add(length.saturating_add(4)))
    }
}
