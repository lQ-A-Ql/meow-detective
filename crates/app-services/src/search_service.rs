use domain::{EntryType, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use search::{extract_text, SearchIndex};
use std::path::Path;
use transport::dto::{SearchHighlightDto, SearchHitDto, SearchResultPageDto, SearchSnippetDto};

pub struct IndexStats {
    pub indexed_count: u64,
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

    for file_id in file_ids {
        let entry = repo.find_by_id(file_id).map_err(|e| e.to_string())?;
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
            }
        }
    }

    if texts.is_empty() {
        return Ok(IndexStats { indexed_count: 0 });
    }

    let index = SearchIndex::create(index_dir).map_err(|e| e.to_string())?;
    let count = index.index_documents(&texts, &paths).map_err(|e| e.to_string())?;

    Ok(IndexStats { indexed_count: count })
}

pub fn search_files_real(index_dir: &Path, query: &str) -> Result<SearchResultPageDto, String> {
    let index = SearchIndex::open(index_dir).map_err(|e| e.to_string())?;
    let hits = index.search(query, 50).map_err(|e| e.to_string())?;

    let items: Vec<SearchHitDto> = hits
        .into_iter()
        .map(|h| SearchHitDto {
            file_id: h.file_id,
            path: h.path,
            score: h.score,
            snippets: vec![SearchSnippetDto {
                text: String::new(),
                highlights: vec![],
            }],
        })
        .collect();

    Ok(SearchResultPageDto {
        total: items.len() as u64,
        took_ms: 0,
        items,
    })
}

pub fn search_files(_query: String) -> SearchResultPageDto {
    SearchResultPageDto {
        total: 2,
        took_ms: 45,
        items: vec![
            SearchHitDto { file_id: "file-001".into(), path: "C:/.../AnyDesk.exe".into(), score: 0.96, snippets: vec![SearchSnippetDto { text: "AnyDesk.exe downloaded...".into(), highlights: vec![SearchHighlightDto { start: 0, end: 7 }] }] },
            SearchHitDto { file_id: "file-002".into(), path: "C:/.../history.txt".into(), score: 0.88, snippets: vec![SearchSnippetDto { text: "powershell Invoke-WebRequest...".into(), highlights: vec![SearchHighlightDto { start: 11, end: 28 }] }] },
        ],
    }
}
