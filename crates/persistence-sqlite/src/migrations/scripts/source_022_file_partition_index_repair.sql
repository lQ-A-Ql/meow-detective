-- Repair file rows created before partition_index was propagated through
-- filesystem staging. Only NULL values are changed; explicit indexes remain
-- authoritative.
WITH RECURSIVE
root_scan(id, data_source_id, suffix, position) AS (
    SELECT
        id,
        data_source_id,
        substr(name, length('Partition ') + 1),
        1
    FROM file_entries
    WHERE parent_id IS NULL
      AND substr(name, 1, length('Partition ')) = 'Partition '

    UNION ALL

    SELECT
        id,
        data_source_id,
        suffix,
        position + 1
    FROM root_scan
    WHERE substr(suffix, position, 1) BETWEEN '0' AND '9'
),
partition_roots(id, data_source_id, partition_index) AS (
    SELECT
        id,
        data_source_id,
        CAST(substr(suffix, 1, position - 1) AS INTEGER)
    FROM root_scan
    WHERE position > 1
      AND substr(suffix, position, 1) IN ('', ' ', '(', ':')
),
partition_tree(id, data_source_id, partition_index) AS (
    SELECT id, data_source_id, partition_index
    FROM partition_roots

    UNION

    SELECT child.id, child.data_source_id, parent.partition_index
    FROM file_entries AS child
    JOIN partition_tree AS parent
      ON child.parent_id = parent.id
     AND child.data_source_id = parent.data_source_id
)
UPDATE file_entries AS target
SET partition_index = partition_tree.partition_index
FROM partition_tree
WHERE target.partition_index IS NULL
  AND target.id = partition_tree.id
  AND target.data_source_id = partition_tree.data_source_id;

CREATE INDEX IF NOT EXISTS idx_source_file_entries_partition
ON file_entries(data_source_id, partition_index);

