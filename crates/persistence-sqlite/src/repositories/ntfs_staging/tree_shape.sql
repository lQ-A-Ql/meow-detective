SELECT (SELECT COUNT(*) FROM ntfs_entries),
              (SELECT COUNT(*) FROM ntfs_paths p JOIN ntfs_entries f USING(record_num) WHERE p.record_num=5 AND p.catalog_parent_record_num IS NULL AND f.entry_type=1),
              (SELECT COUNT(*) FROM ntfs_paths child WHERE catalog_parent_record_num IS NOT NULL AND NOT EXISTS
                (SELECT 1 FROM ntfs_paths parent WHERE parent.record_num=child.catalog_parent_record_num)),
              (WITH RECURSIVE reachable(id) AS (SELECT record_num FROM ntfs_paths WHERE record_num=5
                UNION SELECT child.record_num FROM ntfs_paths child JOIN reachable parent ON child.catalog_parent_record_num=parent.id)
                SELECT COUNT(*) FROM reachable);
