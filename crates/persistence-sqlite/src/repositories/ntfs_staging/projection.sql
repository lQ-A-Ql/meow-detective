CREATE VIEW file_entries AS
SELECT
    'mft:' || partition.value || ':' || fact.record_num AS id,
    CASE WHEN paths.record_num IS NOT NULL THEN
        CASE WHEN paths.catalog_parent_record_num IS NULL THEN NULL
             ELSE 'mft:' || partition.value || ':' || paths.catalog_parent_record_num END
        ELSE CASE WHEN fact.parent_record_num IS NULL THEN NULL
             ELSE 'mft:' || partition.value || ':' || fact.parent_record_num END END AS parent_id,
    source.value AS data_source_id,
    COALESCE(paths.resolved_path, '') AS path, COALESCE(paths.catalog_name, fact.name) AS name,
    CASE fact.entry_type WHEN 1 THEN 'directory' ELSE 'file' END AS entry_type,
    fact.size, fact.ext, fact.deleted, fact.hidden, fact.system, fact.read_only, fact.archive,
    NULL AS unix_mode, fact.encrypted, fact.created_at, fact.modified_at, fact.accessed_at, fact.changed_at,
    NULL AS hash_sha256, CAST(partition.value AS INTEGER) AS partition_index
FROM ntfs_staging_meta source
CROSS JOIN ntfs_staging_meta partition
CROSS JOIN ntfs_entries fact
LEFT JOIN ntfs_paths paths ON paths.record_num=fact.record_num
WHERE source.key='source_id' AND partition.key='partition';
