CREATE TABLE data_source_partitions (
    id TEXT PRIMARY KEY,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    partition_index INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind_label TEXT NOT NULL,
    status TEXT NOT NULL,
    type_guid TEXT,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    filesystem TEXT,
    unlock_hint TEXT,
    lvm_vg_uuid TEXT,
    lvm_vg_name TEXT,
    lvm_lv_uuid TEXT,
    lvm_lv_name TEXT,
    lvm_pv_offsets_json TEXT
);

CREATE INDEX idx_data_source_partitions_data_source
ON data_source_partitions(data_source_id, partition_index);
