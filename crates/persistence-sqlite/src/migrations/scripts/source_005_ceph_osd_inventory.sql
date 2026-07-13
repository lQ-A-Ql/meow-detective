CREATE TABLE IF NOT EXISTS ceph_osd_inventory (
    id TEXT PRIMARY KEY NOT NULL,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    partition_index INTEGER CHECK (partition_index IS NULL OR partition_index >= 0),
    lvm_vg_uuid TEXT,
    lvm_vg_name TEXT,
    lvm_lv_uuid TEXT,
    lvm_lv_name TEXT,
    osd_uuid TEXT NOT NULL,
    ceph_fsid TEXT,
    whoami INTEGER CHECK (whoami IS NULL OR whoami >= 0),
    device_role TEXT NOT NULL,
    device_size INTEGER NOT NULL CHECK (device_size >= 0),
    birth_time_seconds INTEGER NOT NULL,
    birth_time_nanoseconds INTEGER NOT NULL
        CHECK (birth_time_nanoseconds >= 0 AND birth_time_nanoseconds < 1000000000),
    description TEXT NOT NULL,
    is_multi INTEGER NOT NULL CHECK (is_multi IN (0, 1)),
    selected_epoch INTEGER,
    valid_label_count INTEGER NOT NULL CHECK (valid_label_count >= 0),
    label_health TEXT NOT NULL,
    osd_key_present INTEGER NOT NULL CHECK (osd_key_present IN (0, 1)),
    kv_backend TEXT,
    bluefs_enabled INTEGER CHECK (bluefs_enabled IS NULL OR bluefs_enabled IN (0, 1)),
    ceph_version_when_created TEXT,
    require_osd_release INTEGER,
    sanitized_metadata_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_ceph_osd_inventory_data_source
ON ceph_osd_inventory(data_source_id, whoami, osd_uuid);

CREATE INDEX IF NOT EXISTS idx_ceph_osd_inventory_osd_uuid
ON ceph_osd_inventory(osd_uuid);

CREATE TABLE IF NOT EXISTS ceph_osd_label_replicas (
    inventory_id TEXT NOT NULL REFERENCES ceph_osd_inventory(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    device_size INTEGER NOT NULL CHECK (device_size >= 0),
    birth_time_seconds INTEGER NOT NULL,
    birth_time_nanoseconds INTEGER NOT NULL
        CHECK (birth_time_nanoseconds >= 0 AND birth_time_nanoseconds < 1000000000),
    description TEXT NOT NULL,
    is_multi INTEGER NOT NULL CHECK (is_multi IN (0, 1)),
    epoch INTEGER,
    is_selected INTEGER NOT NULL CHECK (is_selected IN (0, 1)),
    struct_version INTEGER NOT NULL CHECK (struct_version >= 0 AND struct_version <= 255),
    struct_compat_version INTEGER NOT NULL
        CHECK (struct_compat_version >= 0 AND struct_compat_version <= 255),
    PRIMARY KEY (inventory_id, position)
);

CREATE INDEX IF NOT EXISTS idx_ceph_osd_label_replicas_selected
ON ceph_osd_label_replicas(inventory_id, is_selected, position);
