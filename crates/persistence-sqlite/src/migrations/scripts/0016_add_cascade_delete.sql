-- 添加外键级联删除支持
-- 注意: SQLite 不支持 ALTER TABLE ADD CONSTRAINT
-- 需要重建表来添加外键约束

-- 1. 创建临时表并迁移数据
CREATE TABLE data_sources_new (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_path TEXT NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO data_sources_new SELECT * FROM data_sources;
DROP TABLE data_sources;
ALTER TABLE data_sources_new RENAME TO data_sources;

CREATE INDEX idx_data_sources_case_id ON data_sources(case_id);

-- 2. 重建 file_entries 表
CREATE TABLE file_entries_new (
    id TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT REFERENCES file_entries(id),
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    size INTEGER,
    ext TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    modified_at TEXT,
    accessed_at TEXT,
    changed_at TEXT,
    hash_sha256 TEXT
);

INSERT INTO file_entries_new SELECT * FROM file_entries;
DROP TABLE file_entries;
ALTER TABLE file_entries_new RENAME TO file_entries;

CREATE INDEX idx_file_entries_parent ON file_entries(parent_id);
CREATE INDEX idx_file_entries_data_source ON file_entries(data_source_id);
CREATE INDEX idx_file_entries_path ON file_entries(path);

-- 3. 重建 jobs 表
CREATE TABLE jobs_new (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    progress INTEGER NOT NULL DEFAULT 0,
    detail TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    finished_at TEXT
);

INSERT INTO jobs_new SELECT * FROM jobs;
DROP TABLE jobs;
ALTER TABLE jobs_new RENAME TO jobs;

CREATE INDEX idx_jobs_case ON jobs(case_id);
CREATE INDEX idx_jobs_status ON jobs(status);

-- 4. 重建 reports 表
CREATE TABLE reports_new (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    template_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    created_by TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'running',
    progress INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO reports_new SELECT * FROM reports;
DROP TABLE reports;
ALTER TABLE reports_new RENAME TO reports;

CREATE INDEX idx_reports_case ON reports(case_id);
