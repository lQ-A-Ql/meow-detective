CREATE TABLE IF NOT EXISTS android_system_facts (
    id TEXT PRIMARY KEY,
    data_source_id TEXT NOT NULL,
    field_key TEXT NOT NULL,
    field_value TEXT NOT NULL,
    confidence TEXT NOT NULL,
    source_file_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    parser TEXT NOT NULL,
    source_rank INTEGER NOT NULL,
    warning TEXT,
    UNIQUE(data_source_id, field_key)
);

CREATE INDEX IF NOT EXISTS idx_android_system_facts_source
    ON android_system_facts(data_source_id, source_rank, field_key);

CREATE TABLE IF NOT EXISTS android_packages (
    id TEXT PRIMARY KEY,
    data_source_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    app_name TEXT,
    version_code TEXT,
    install_time TEXT,
    update_time TEXT,
    installer TEXT,
    uid INTEGER,
    code_path TEXT,
    source_file_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    parser TEXT NOT NULL,
    warning TEXT,
    UNIQUE(data_source_id, package_name)
);

CREATE INDEX IF NOT EXISTS idx_android_packages_source_name
    ON android_packages(data_source_id, package_name COLLATE NOCASE);
