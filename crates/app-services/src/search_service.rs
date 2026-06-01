use domain::{EntryType, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use search::{extract_text, SearchIndex, SearchResult};
use std::path::Path;
use transport::dto::{SearchHighlightDto, SearchHitDto, SearchResultPageDto, SearchSnippetDto};

pub struct IndexStats {
    pub indexed_count: u64,
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

pub fn index_files(
    conn: &Connection,
    index_dir: &Path,
    file_ids: &[FileEntryId],
    reader_fn: impl Fn(&FileEntryId) -> Option<Box<dyn std::io::Read>>,
) -> Result<IndexStats, String> {
    let repo = FileRepo::new(conn);
    let mut texts = Vec::new();
    let mut paths = Vec::new();
    let mut warning_count = 0u32;
    let mut skipped_count = 0u32;
    let failed_count = 0u32;

    for file_id in file_ids {
        let entry = match repo.find_by_id(file_id) {
            Ok(entry) => entry,
            Err(e) => return Err(e.to_string()),
        };
        if let Some(entry) = entry {
            if entry.entry_type == EntryType::Directory {
                continue;
            }
            let ext = entry.ext.as_deref().unwrap_or("");
            let mime = if matches!(ext, "txt" | "log" | "csv" | "json" | "xml" | "html" | "md") {
                Some("text/plain")
            } else {
                None
            };

            if let Some(reader) = reader_fn(&entry.id) {
                let text = extract_text(reader, &entry.id.0, mime);
                if text.extractable {
                    texts.push(text);
                    paths.push((entry.id.0.clone(), entry.path.clone()));
                }
            } else {
                warning_count += 1;
                skipped_count += 1;
            }
        }
    }

    if texts.is_empty() {
        return Ok(IndexStats {
            indexed_count: 0,
            warning_count,
            skipped_count,
            failed_count,
        });
    }

    let index = SearchIndex::create(index_dir).map_err(|e| e.to_string())?;
    let count = index
        .index_documents(&texts, &paths)
        .map_err(|e| e.to_string())?;

    Ok(IndexStats {
        indexed_count: count,
        warning_count,
        skipped_count,
        failed_count,
    })
}

pub fn search_files_real(
    index_dir: &Path,
    query: &str,
    offset: u64,
    limit: u32,
) -> Result<SearchResultPageDto, String> {
    let index = SearchIndex::open(index_dir).map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    // Request more results than needed to support offset
    let search_limit = (offset + limit as u64).min(1000) as usize;
    let SearchResult { hits, total_count } = index
        .search(query, search_limit)
        .map_err(|e| e.to_string())?;
    let took_ms = start.elapsed().as_millis() as u64;

    // Apply offset
    let hits: Vec<_> = hits.into_iter().skip(offset as usize).collect();

    let items: Vec<SearchHitDto> = hits
        .into_iter()
        .map(|h| {
            let snippets: Vec<SearchSnippetDto> = h
                .snippets
                .into_iter()
                .map(|s| SearchSnippetDto {
                    text: s.text,
                    highlights: s
                        .highlights
                        .into_iter()
                        .map(|hl| SearchHighlightDto {
                            start: hl.start,
                            end: hl.end,
                        })
                        .collect(),
                })
                .collect();
            SearchHitDto {
                file_id: h.file_id,
                path: h.path,
                score: h.score,
                snippets: if snippets.is_empty() {
                    vec![SearchSnippetDto {
                        text: String::new(),
                        highlights: vec![],
                    }]
                } else {
                    snippets
                },
            }
        })
        .collect();

    Ok(SearchResultPageDto {
        total: total_count,
        took_ms,
        items,
    })
}
