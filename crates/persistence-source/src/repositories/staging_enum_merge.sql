INSERT INTO main.file_entries
                    (id, parent_id, data_source_id, path, name, entry_type,
                      size, ext, deleted, hidden, system, read_only, archive, unix_mode, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256,
                      partition_index)
                     SELECT
                        id,
                        CASE
                          WHEN parent_id IS NULL THEN ?1
                          WHEN parent_id = id THEN ?1
                          WHEN parent_id IN (
                            SELECT id FROM staging.file_entries
                            WHERE entry_type = 'directory'
                              AND (
                                parent_id IS NULL
                                OR parent_id = id
                              )
                              AND name IN ('\', '/', '.')
                          ) THEN ?1
                          ELSE parent_id
                        END,
                        data_source_id, path, name, LOWER(entry_type),
                        size, ext, deleted, hidden, system, read_only, archive, unix_mode, encrypted,
                        created_at, modified_at, accessed_at, changed_at, hash_sha256,
                        ?2
                     FROM staging.file_entries
                     WHERE NOT (
                        entry_type = 'directory'
                        AND (parent_id IS NULL OR parent_id = id)
                        AND name IN ('\', '/', '.')
                     );
