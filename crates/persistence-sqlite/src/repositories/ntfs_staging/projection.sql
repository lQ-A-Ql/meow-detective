CREATE VIEW file_entries AS
SELECT
    'mft:' || partition.value || ':' || record_num AS id,
    CASE WHEN parent_record_num IS NULL THEN NULL
         ELSE 'mft:' || partition.value || ':' || parent_record_num END AS parent_id,
    source.value AS data_source_id,
    path, name, CASE entry_type WHEN 1 THEN 'directory' ELSE 'file' END AS entry_type,
    size, ext, deleted, hidden, system, read_only, archive,
    NULL AS unix_mode, encrypted, created_at, modified_at, accessed_at, changed_at,
    NULL AS hash_sha256, CAST(partition.value AS INTEGER) AS partition_index
FROM ntfs_entries
CROSS JOIN ntfs_staging_meta source
CROSS JOIN ntfs_staging_meta partition
WHERE source.key='source_id' AND partition.key='partition';
