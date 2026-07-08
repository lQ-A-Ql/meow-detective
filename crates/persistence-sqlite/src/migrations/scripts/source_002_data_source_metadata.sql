CREATE TABLE IF NOT EXISTS data_sources (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_path TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    source_hash_sha256 TEXT,
    hash_status TEXT DEFAULT 'unknown',
    canonical_source_path TEXT,
    evidence_size INTEGER,
    reader_kind TEXT,
    provenance_status TEXT DEFAULT 'unknown',
    provenance_warnings TEXT DEFAULT '[]'
);
