ALTER TABLE file_entries
ADD COLUMN encrypted INTEGER
CHECK (encrypted IS NULL OR encrypted IN (0, 1));
