WITH raw_roots AS MATERIALIZED (
    SELECT id FROM file_entries
    WHERE lower(entry_type) = 'directory'
      AND (parent_id IS NULL OR parent_id = id)
      AND name IN ('\', '/', '.')
)
SELECT id,
       CASE WHEN parent_id IS NULL OR parent_id = id
                  OR parent_id IN (SELECT id FROM raw_roots) THEN ?1
            ELSE parent_id END,
       data_source_id, path, name, entry_type, size, ext, deleted, hidden, system,
       created_at, modified_at, accessed_at, changed_at, hash_sha256,
       encrypted, read_only, archive, unix_mode
FROM file_entries
WHERE id NOT IN (SELECT id FROM raw_roots)
ORDER BY id
