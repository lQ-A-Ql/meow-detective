-- 修复 timeline_events 表中的 case_id 字段
-- 将空的 case_id 更新为对应数据源的 case_id

UPDATE timeline_events 
SET case_id = (
    SELECT ds.case_id 
    FROM file_entries fe
    JOIN data_sources ds ON fe.data_source_id = ds.id
    WHERE fe.id = timeline_events.source_object_id
)
WHERE case_id = '' 
AND source_object_id IN (
    SELECT id FROM file_entries
);
