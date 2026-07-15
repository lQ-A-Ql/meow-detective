CREATE TABLE IF NOT EXISTS ceph_osd_device_bindings (
    inventory_id TEXT PRIMARY KEY NOT NULL
        REFERENCES ceph_osd_inventory(id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL,
    source_path TEXT NOT NULL CHECK (
        length(source_path) > 0
        AND instr(source_path, char(0)) = 0
    ),
    canonical_source_path TEXT NOT NULL CHECK (
        length(canonical_source_path) > 0
        AND instr(canonical_source_path, char(0)) = 0
    ),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('e01', 'raw')),
    lvm_vg_uuid TEXT NOT NULL CHECK (
        length(lvm_vg_uuid) > 0
        AND instr(lvm_vg_uuid, char(0)) = 0
    ),
    lvm_vg_name TEXT NOT NULL CHECK (
        length(lvm_vg_name) > 0
        AND instr(lvm_vg_name, char(0)) = 0
    ),
    lvm_lv_uuid TEXT NOT NULL CHECK (
        length(lvm_lv_uuid) > 0
        AND instr(lvm_lv_uuid, char(0)) = 0
    ),
    lvm_lv_name TEXT NOT NULL CHECK (
        length(lvm_lv_name) > 0
        AND instr(lvm_lv_name, char(0)) = 0
    ),
    device_size INTEGER NOT NULL CHECK (device_size > 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (inventory_id, data_source_id)
        REFERENCES ceph_osd_inventory(id, data_source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_osd_device_bindings_source
ON ceph_osd_device_bindings(data_source_id, inventory_id);

CREATE TABLE IF NOT EXISTS ceph_osd_device_binding_pvs (
    inventory_id TEXT NOT NULL
        REFERENCES ceph_osd_device_bindings(inventory_id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_path TEXT NOT NULL CHECK (
        length(source_path) > 0
        AND instr(source_path, char(0)) = 0
    ),
    canonical_source_path TEXT NOT NULL CHECK (
        length(canonical_source_path) > 0
        AND instr(canonical_source_path, char(0)) = 0
    ),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('e01', 'raw')),
    pv_offset INTEGER NOT NULL CHECK (pv_offset >= 0),
    pv_uuid TEXT NOT NULL CHECK (
        length(pv_uuid) > 0
        AND instr(pv_uuid, char(0)) = 0
    ),
    pv_name TEXT CHECK (
        pv_name IS NULL
        OR (
            length(pv_name) > 0
            AND instr(pv_name, char(0)) = 0
        )
    ),
    PRIMARY KEY (inventory_id, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_ceph_osd_device_binding_pvs_uuid
ON ceph_osd_device_binding_pvs(inventory_id, pv_uuid);
