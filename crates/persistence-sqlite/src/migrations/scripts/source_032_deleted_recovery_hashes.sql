ALTER TABLE deleted_file_recoveries ADD COLUMN content_md5 TEXT CHECK (
    content_md5 IS NULL OR (
        length(content_md5) = 32
        AND content_md5 NOT GLOB '*[^0-9a-f]*'
    )
);

ALTER TABLE deleted_file_recoveries ADD COLUMN content_sha1 TEXT CHECK (
    content_sha1 IS NULL OR (
        length(content_sha1) = 40
        AND content_sha1 NOT GLOB '*[^0-9a-f]*'
    )
);

CREATE INDEX IF NOT EXISTS idx_deleted_file_recoveries_content_md5
ON deleted_file_recoveries(content_md5, scan_id)
WHERE content_md5 IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_deleted_file_recoveries_content_sha1
ON deleted_file_recoveries(content_sha1, scan_id)
WHERE content_sha1 IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_deleted_file_recoveries_content_sha256
ON deleted_file_recoveries(content_sha256, scan_id)
WHERE content_sha256 IS NOT NULL;
