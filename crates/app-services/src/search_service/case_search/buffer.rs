use super::{file_hit_to_dto, index_error, SearchSource};
use crate::{search_service::SearchError, source_db::encode_source_scoped_id};
use persistence_source::fjall::instrumentation;
use search::{FileSearchAfterKey, FileSearchRankedHit};
use std::collections::VecDeque;
use transport::dto::SearchFileHitDto;

struct Hit {
    ranked: FileSearchRankedHit,
    scoped_id: String,
}

pub(super) struct SourceBuffer {
    source: SearchSource,
    pub total: u64,
    pub after: Option<FileSearchAfterKey>,
    consumed: u64,
    batch: usize,
    hits: VecDeque<Hit>,
}

impl SourceBuffer {
    pub(super) fn new(
        source: SearchSource,
        after: Option<FileSearchAfterKey>,
        total: Option<u64>,
        limit: usize,
    ) -> Result<Self, SearchError> {
        let page = match total {
            Some(total) => source
                .session
                .rank_after_with_total(after.as_ref(), limit, total),
            None => source.session.rank_after(after.as_ref(), limit),
        }
        .map_err(index_error)?;
        instrumentation::record_ranked_hits(page.hits.len() as u64);
        let hits = scoped_hits(&source, page.hits);
        Ok(Self {
            source,
            total: page.total_count,
            after,
            consumed: 0,
            batch: limit.clamp(1, 64),
            hits,
        })
    }

    pub(super) fn front_key(&self) -> Option<(&str, &str)> {
        self.hits
            .front()
            .map(|hit| (hit.ranked.sort_value(), hit.scoped_id.as_str()))
    }

    pub(super) fn take(
        &mut self,
        materialize: bool,
    ) -> Result<Option<SearchFileHitDto>, SearchError> {
        let hit = self.hits.pop_front().ok_or_else(|| {
            SearchError::Index("search merge selected an empty source".to_owned())
        })?;
        self.after = Some(hit.ranked.after_key());
        self.consumed += 1;
        if !materialize {
            return Ok(None);
        }
        let hit = self
            .source
            .session
            .materialize(hit.ranked)
            .map_err(index_error)?;
        Ok(Some(file_hit_to_dto(
            hit,
            &self.source.data_source_id,
            &self.source.data_source_name,
        )))
    }

    pub(super) fn refill(&mut self) -> Result<(), SearchError> {
        if !self.hits.is_empty() || self.consumed >= self.total {
            return Ok(());
        }
        let page = self
            .source
            .session
            .rank_after_with_total(self.after.as_ref(), self.batch, self.total)
            .map_err(index_error)?;
        instrumentation::record_ranked_hits(page.hits.len() as u64);
        if page.hits.is_empty() {
            return Err(SearchError::Index(
                "search source exhausted before its result count".to_owned(),
            ));
        }
        self.hits = scoped_hits(&self.source, page.hits);
        Ok(())
    }
}

fn scoped_hits(source: &SearchSource, hits: Vec<FileSearchRankedHit>) -> VecDeque<Hit> {
    hits.into_iter()
        .map(|ranked| Hit {
            scoped_id: encode_source_scoped_id(&source.data_source_id, ranked.file_id()),
            ranked,
        })
        .collect()
}
