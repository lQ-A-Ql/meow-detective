ALTER TABLE deleted_file_recoveries
ADD COLUMN mft_sequence INTEGER
CHECK (mft_sequence IS NULL OR (mft_sequence > 0 AND mft_sequence <= 65535));
