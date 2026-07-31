ALTER TABLE file_entries
ADD COLUMN read_only INTEGER NOT NULL DEFAULT 0 CHECK (read_only IN (0, 1));
