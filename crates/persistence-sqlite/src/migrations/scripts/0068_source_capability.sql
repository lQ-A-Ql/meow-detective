CREATE TABLE IF NOT EXISTS source_capability (
  case_id TEXT NOT NULL,
  data_source_id TEXT NOT NULL,
  capability TEXT NOT NULL,
  state TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 0,
  progress_done INTEGER NOT NULL DEFAULT 0,
  progress_total INTEGER,
  phase TEXT,
  detail TEXT,
  error_code TEXT,
  derived INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (case_id, data_source_id, capability)
);

CREATE INDEX IF NOT EXISTS idx_source_capability_case
  ON source_capability (case_id, data_source_id);
