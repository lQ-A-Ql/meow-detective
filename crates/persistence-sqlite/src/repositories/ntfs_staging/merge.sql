WITH folded_root AS MATERIALIZED (
    SELECT f.record_num FROM staging.ntfs_entries f JOIN staging.ntfs_paths p USING(record_num)
    WHERE f.record_num=5 AND f.entry_type=1 AND p.catalog_parent_record_num IS NULL
      AND p.catalog_name IN ('\', '/', '.')
)
INSERT INTO main.file_entries
    (id,parent_id,data_source_id,path,name,entry_type,size,ext,deleted,hidden,system,
     read_only,archive,unix_mode,encrypted,created_at,modified_at,accessed_at,changed_at,hash_sha256,partition_index)
SELECT
    'mft:' || partition.value || ':' || f.record_num,
    CASE WHEN p.catalog_parent_record_num IS NULL OR p.catalog_parent_record_num=f.record_num
              OR p.catalog_parent_record_num IN (SELECT record_num FROM folded_root) THEN ?1
         ELSE 'mft:' || partition.value || ':' || p.catalog_parent_record_num END,
    source.value,p.resolved_path,p.catalog_name,CASE f.entry_type WHEN 1 THEN 'directory' ELSE 'file' END,
    f.size,f.ext,f.deleted,f.hidden,f.system,f.read_only,f.archive,NULL,f.encrypted,
    f.created_at,f.modified_at,f.accessed_at,f.changed_at,NULL,?2
FROM staging.ntfs_staging_meta source
CROSS JOIN staging.ntfs_staging_meta partition
CROSS JOIN staging.ntfs_entries f
JOIN staging.ntfs_paths p ON p.record_num=f.record_num
WHERE source.key='source_id' AND partition.key='partition'
  AND f.record_num NOT IN (SELECT record_num FROM folded_root);
