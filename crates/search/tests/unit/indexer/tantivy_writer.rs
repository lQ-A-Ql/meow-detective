use super::*;
use crate::extractor::ExtractedText;
use tempfile::tempdir;

fn sample_extracted_texts() -> Vec<ExtractedText> {
    vec![
        ExtractedText {
            file_id: "f1".to_string(),
            content: "The quick brown fox jumps over the lazy dog".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 43,
        },
        ExtractedText {
            file_id: "f2".to_string(),
            content: "Rust is a systems programming language".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 39,
        },
        ExtractedText {
            file_id: "f3".to_string(),
            content: "Binary file cannot be extracted".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 32,
        },
    ]
}

fn sample_paths() -> Vec<(String, String)> {
    vec![
        ("f1".to_string(), "/evidence/doc1.txt".to_string()),
        ("f2".to_string(), "/evidence/doc2.rs".to_string()),
        ("f3".to_string(), "/evidence/doc3.bin".to_string()),
    ]
}

#[test]
fn create_search_index_creates_directory() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let result = SearchIndex::create(&index_path);
    assert!(result.is_ok());
    assert!(index_path.exists());
}

#[test]
fn index_documents_returns_count() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();
    let count = index
        .index_documents(&sample_extracted_texts(), &sample_paths())
        .unwrap();
    assert_eq!(count, 3);
}

#[test]
fn search_finds_indexed_document() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();
    index
        .index_documents(&sample_extracted_texts(), &sample_paths())
        .unwrap();

    let result = index.search("rust", 10).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].file_id, "f2");
    assert!(result.hits[0].path.contains("doc2.rs"));
}

#[test]
fn search_returns_snippets_with_highlights() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();
    index
        .index_documents(&sample_extracted_texts(), &sample_paths())
        .unwrap();

    let result = index.search("fox", 10).unwrap();
    assert_eq!(result.total_count, 1);
    assert!(!result.hits[0].snippets.is_empty());
    assert!(result.hits[0].snippets[0].text.contains("fox"));
}

#[test]
fn search_no_results() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();
    index
        .index_documents(&sample_extracted_texts(), &sample_paths())
        .unwrap();

    let result = index.search("nonexistent", 10).unwrap();
    assert_eq!(result.total_count, 0);
    assert!(result.hits.is_empty());
}

#[test]
fn index_documents_skips_non_extractable() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();

    let texts = vec![
        ExtractedText {
            file_id: "f1".to_string(),
            content: "extractable content".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 19,
        },
        ExtractedText {
            file_id: "f2".to_string(),
            content: String::new(),
            encoding: "binary".to_string(),
            extractable: false,
            byte_count: 0,
        },
    ];
    let paths = vec![
        ("f1".to_string(), "/file1.txt".to_string()),
        ("f2".to_string(), "/file2.bin".to_string()),
    ];

    let count = index.index_documents(&texts, &paths).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn index_documents_skips_empty_content() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();

    let texts = vec![ExtractedText {
        file_id: "f1".to_string(),
        content: String::new(),
        encoding: "utf-8".to_string(),
        extractable: true,
        byte_count: 0,
    }];
    let paths = vec![("f1".to_string(), "/file1.txt".to_string())];

    let count = index.index_documents(&texts, &paths).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn open_existing_index() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    {
        let index = SearchIndex::create(&index_path).unwrap();
        index
            .index_documents(&sample_extracted_texts(), &sample_paths())
            .unwrap();
    }
    // Reopen the index
    let index = SearchIndex::open(&index_path).unwrap();
    let result = index.search("rust", 10).unwrap();
    assert_eq!(result.total_count, 1);
}

#[test]
fn search_score_is_positive() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();
    index
        .index_documents(&sample_extracted_texts(), &sample_paths())
        .unwrap();

    let result = index.search("fox", 10).unwrap();
    assert!(result.hits[0].score > 0.0);
}

#[test]
fn index_documents_replaces_existing() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("test_index");
    let index = SearchIndex::create(&index_path).unwrap();

    // Index original
    let texts1 = vec![ExtractedText {
        file_id: "f1".to_string(),
        content: "original content".to_string(),
        encoding: "utf-8".to_string(),
        extractable: true,
        byte_count: 16,
    }];
    let paths1 = vec![("f1".to_string(), "/file1.txt".to_string())];
    index.index_documents(&texts1, &paths1).unwrap();

    // Re-index same file_id with new content
    let texts2 = vec![ExtractedText {
        file_id: "f1".to_string(),
        content: "updated content".to_string(),
        encoding: "utf-8".to_string(),
        extractable: true,
        byte_count: 16,
    }];
    let paths2 = vec![("f1".to_string(), "/file1.txt".to_string())];
    index.index_documents(&texts2, &paths2).unwrap();

    // Search for old content should return nothing
    let result = index.search("original", 10).unwrap();
    assert_eq!(result.total_count, 0);

    // Search for new content should return 1
    let result = index.search("updated", 10).unwrap();
    assert_eq!(result.total_count, 1);
}

#[test]
fn create_reuses_existing_index_without_clearing_documents() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("existing-index");

    let index = SearchIndex::create(&index_path).unwrap();
    index
        .index_documents(
            &[ExtractedText {
                file_id: "f1".to_string(),
                content: "persistent token".to_string(),
                encoding: "utf-8".to_string(),
                extractable: true,
                byte_count: 16,
            }],
            &[("f1".to_string(), "/file1.txt".to_string())],
        )
        .unwrap();

    let reopened = SearchIndex::create(&index_path).unwrap();
    let result = reopened.search("persistent", 10).unwrap();
    assert_eq!(result.total_count, 1);
    assert_eq!(result.hits[0].file_id, "f1");
}

// ------------------------------------------------------------------
// Chunked / incremental indexing tests
// ------------------------------------------------------------------

#[test]
fn chunked_index_produces_same_results_as_batch() {
    let dir = tempdir().unwrap();

    // Build via batch (single commit).
    let batch_path = dir.path().join("batch");
    let batch_idx = SearchIndex::create(&batch_path).unwrap();
    let texts = sample_extracted_texts();
    let paths = sample_paths();
    batch_idx.index_documents(&texts, &paths).unwrap();
    let batch_result = batch_idx.search("fox", 10).unwrap();

    // Build via chunked (multiple commits).
    let chunked_path = dir.path().join("chunked");
    let chunked_idx = SearchIndex::create(&chunked_path).unwrap();
    // Use a small effective chunk size so we actually exercise the loop.
    // CHUNK_COMMIT_INTERVAL is 1000 so with 3 docs we get 1 chunk — that is
    // fine; the key property is correctness, not chunk count.
    chunked_idx
        .index_files_chunked(&texts, &paths, None)
        .unwrap();
    let chunked_result = chunked_idx.search("fox", 10).unwrap();

    assert_eq!(batch_result.total_count, chunked_result.total_count);
    assert_eq!(batch_result.hits.len(), chunked_result.hits.len());
    for (bh, ch) in batch_result.hits.iter().zip(chunked_result.hits.iter()) {
        assert_eq!(bh.file_id, ch.file_id);
        assert!((bh.score - ch.score).abs() < 0.001);
    }
}

#[test]
fn incremental_index_only_adds_new_files() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("incr_index");
    let index = SearchIndex::create(&index_path).unwrap();

    // Seed the index with two files.
    let seed_texts = vec![
        ExtractedText {
            file_id: "existing-1".to_string(),
            content: "alpha bravo charlie".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 18,
        },
        ExtractedText {
            file_id: "existing-2".to_string(),
            content: "delta echo foxtrot".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 18,
        },
    ];
    let seed_paths = vec![
        ("existing-1".to_string(), "/a/existing1.txt".to_string()),
        ("existing-2".to_string(), "/a/existing2.txt".to_string()),
    ];
    index.index_documents(&seed_texts, &seed_paths).unwrap();

    // Mix: two existing + one new file.
    let mixed_texts = vec![
        // already indexed
        ExtractedText {
            file_id: "existing-1".to_string(),
            content: "alpha bravo charlie revised".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 26,
        },
        // new
        ExtractedText {
            file_id: "new-1".to_string(),
            content: "golf hotel india".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 17,
        },
        // already indexed
        ExtractedText {
            file_id: "existing-2".to_string(),
            content: "delta echo foxtrot revised".to_string(),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 26,
        },
    ];
    let mixed_paths = vec![
        ("existing-1".to_string(), "/a/existing1.txt".to_string()),
        ("new-1".to_string(), "/b/new1.txt".to_string()),
        ("existing-2".to_string(), "/a/existing2.txt".to_string()),
    ];

    let stats = index
        .index_files_incremental(&mixed_texts, &mixed_paths, 2)
        .unwrap();

    // Only the one truly-new file should have been indexed.
    assert_eq!(stats.total_docs, 1);

    // The existing files still contain their original content.
    let result = index.search("charlie", 10).unwrap();
    assert_eq!(result.total_count, 1);
    // "revised" was in the incoming text for existing-1 but was skipped,
    // so it should NOT be found.
    let result_revised = index.search("revised", 10).unwrap();
    assert_eq!(result_revised.total_count, 0);

    // The new file IS searchable.
    let result_new = index.search("hotel", 10).unwrap();
    assert_eq!(result_new.total_count, 1);
    assert_eq!(result_new.hits[0].file_id, "new-1");
}

#[test]
fn partial_index_is_searchable_mid_build() {
    let dir = tempdir().unwrap();
    let index_path = dir.path().join("partial_index");
    let index = SearchIndex::create(&index_path).unwrap();

    // Batch 1: index 5 documents and commit.
    let batch1_texts: Vec<ExtractedText> = (0..5)
        .map(|i| ExtractedText {
            file_id: format!("batch1-{i}"),
            content: format!("batch one document {i} with unique token zebra{i}"),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 50,
        })
        .collect();
    let batch1_paths: Vec<(String, String)> = (0..5)
        .map(|i| (format!("batch1-{i}"), format!("/batch1/doc{i}.txt")))
        .collect();
    index.index_documents(&batch1_texts, &batch1_paths).unwrap();

    // After commit, batch 1 must be searchable.
    let r1 = index.search("zebra0", 10).unwrap();
    assert_eq!(
        r1.total_count, 1,
        "batch 1 should be searchable after commit"
    );

    // Batch 2: index another 5 documents and commit.
    let batch2_texts: Vec<ExtractedText> = (0..5)
        .map(|i| ExtractedText {
            file_id: format!("batch2-{i}"),
            content: format!("batch two document {i} with unique token giraffe{i}"),
            encoding: "utf-8".to_string(),
            extractable: true,
            byte_count: 50,
        })
        .collect();
    let batch2_paths: Vec<(String, String)> = (0..5)
        .map(|i| (format!("batch2-{i}"), format!("/batch2/doc{i}.txt")))
        .collect();
    index.index_documents(&batch2_texts, &batch2_paths).unwrap();

    // Both batches should now be searchable.
    let r_both_zebra = index.search("zebra3", 10).unwrap();
    assert_eq!(r_both_zebra.total_count, 1, "batch 1 docs should persist");

    let r_both_giraffe = index.search("giraffe4", 10).unwrap();
    assert_eq!(
        r_both_giraffe.total_count, 1,
        "batch 2 docs should be searchable"
    );
}
