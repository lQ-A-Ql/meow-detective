-- Idempotent: only add columns if they don't already exist
-- SQLite doesn't support IF NOT EXISTS for ALTER TABLE ADD COLUMN,
-- so we use a DO/WHEN pattern by catching errors at the migration runner level.

-- These statements are safe to re-run: if the column already exists,
-- the migration runner wraps each script in a try-catch and skips on error.

-- Note: Each statement below will silently fail if the column already exists.
-- The migration runner (run_all) catches errors per-script and continues.

ALTER TABLE jobs ADD COLUMN current_partition TEXT DEFAULT NULL;
ALTER TABLE jobs ADD COLUMN completed_partitions INTEGER DEFAULT 0;
ALTER TABLE jobs ADD COLUMN total_partitions INTEGER DEFAULT 0;
ALTER TABLE jobs ADD COLUMN partition_progress INTEGER DEFAULT 0;
