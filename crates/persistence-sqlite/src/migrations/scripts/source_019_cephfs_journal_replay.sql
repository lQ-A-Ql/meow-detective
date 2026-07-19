CREATE UNIQUE INDEX IF NOT EXISTS idx_ceph_fs_metadata_objects_identity_locator
ON ceph_fs_metadata_objects(
    filesystem_identity,
    inventory_id,
    object_identity_sha256,
    locator
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ceph_fs_metadata_inventories_source_binding
ON ceph_fs_metadata_inventories(
    filesystem_identity,
    inventory_id,
    data_source_id
);

CREATE TABLE IF NOT EXISTS ceph_fs_journal_replays (
    filesystem_identity TEXT NOT NULL,
    inventory_id TEXT NOT NULL,
    data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE,
    rank INTEGER NOT NULL CHECK (rank BETWEEN 0 AND 255),
    filesystem_id INTEGER NOT NULL CHECK (filesystem_id >= 0),
    fsmap_epoch INTEGER NOT NULL CHECK (fsmap_epoch BETWEEN 1 AND 4294967295),
    mdsmap_epoch INTEGER NOT NULL CHECK (mdsmap_epoch BETWEEN 1 AND 4294967295),
    rank_incarnation INTEGER NOT NULL CHECK (rank_incarnation >= 0),
    rank_gid_hex TEXT NOT NULL CHECK (
        length(rank_gid_hex) = 16
        AND rank_gid_hex NOT GLOB '*[^0-9a-f]*'
    ),
    pointer_front_inode_hex TEXT NOT NULL CHECK (
        length(pointer_front_inode_hex) = 16
        AND pointer_front_inode_hex NOT GLOB '*[^0-9a-f]*'
        AND pointer_front_inode_hex <> '0000000000000000'
    ),
    pointer_back_inode_hex TEXT NOT NULL CHECK (
        length(pointer_back_inode_hex) = 16
        AND pointer_back_inode_hex NOT GLOB '*[^0-9a-f]*'
    ),
    journal_inode_hex TEXT NOT NULL CHECK (
        length(journal_inode_hex) = 16
        AND journal_inode_hex NOT GLOB '*[^0-9a-f]*'
        AND journal_inode_hex <> '0000000000000000'
    ),
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    decoder_profile TEXT NOT NULL CHECK (decoder_profile = 'cephfs-journal-v1'),
    source_semantic_sha256 TEXT NOT NULL CHECK (
        length(source_semantic_sha256) = 64
        AND source_semantic_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    metadata_inventory_sha256 TEXT NOT NULL CHECK (
        length(metadata_inventory_sha256) = 64
        AND metadata_inventory_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    raw_fsmap_snapshot_sha256 TEXT NOT NULL CHECK (
        length(raw_fsmap_snapshot_sha256) = 64
        AND raw_fsmap_snapshot_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    raw_mdsmap_snapshot_sha256 TEXT NOT NULL CHECK (
        length(raw_mdsmap_snapshot_sha256) = 64
        AND raw_mdsmap_snapshot_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    map_provenance_sha256 TEXT NOT NULL CHECK (
        length(map_provenance_sha256) = 64
        AND map_provenance_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    map_provenance_count INTEGER NOT NULL CHECK (map_provenance_count > 0),
    pointer_locator TEXT NOT NULL CHECK (
        length(pointer_locator) > 0 AND instr(pointer_locator, char(0)) = 0
    ),
    pointer_object_identity_sha256 TEXT NOT NULL CHECK (
        length(pointer_object_identity_sha256) = 64
        AND pointer_object_identity_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    pointer_range_offset_hex TEXT NOT NULL CHECK (
        length(pointer_range_offset_hex) = 16
        AND pointer_range_offset_hex NOT GLOB '*[^0-9a-f]*'
    ),
    pointer_range_length_hex TEXT NOT NULL CHECK (
        length(pointer_range_length_hex) = 16
        AND pointer_range_length_hex NOT GLOB '*[^0-9a-f]*'
        AND pointer_range_length_hex <> '0000000000000000'
    ),
    pointer_range_sha256 TEXT NOT NULL CHECK (
        length(pointer_range_sha256) = 64
        AND pointer_range_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    header_locator TEXT NOT NULL CHECK (
        length(header_locator) > 0 AND instr(header_locator, char(0)) = 0
    ),
    header_object_identity_sha256 TEXT NOT NULL CHECK (
        length(header_object_identity_sha256) = 64
        AND header_object_identity_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    header_range_offset_hex TEXT NOT NULL CHECK (
        length(header_range_offset_hex) = 16
        AND header_range_offset_hex NOT GLOB '*[^0-9a-f]*'
    ),
    header_range_length_hex TEXT NOT NULL CHECK (
        length(header_range_length_hex) = 16
        AND header_range_length_hex NOT GLOB '*[^0-9a-f]*'
        AND header_range_length_hex <> '0000000000000000'
    ),
    header_range_sha256 TEXT NOT NULL CHECK (
        length(header_range_sha256) = 64
        AND header_range_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    trimmed_pos_hex TEXT NOT NULL CHECK (
        length(trimmed_pos_hex) = 16
        AND trimmed_pos_hex NOT GLOB '*[^0-9a-f]*'
    ),
    expire_pos_hex TEXT NOT NULL CHECK (
        length(expire_pos_hex) = 16
        AND expire_pos_hex NOT GLOB '*[^0-9a-f]*'
    ),
    unused_pos_hex TEXT NOT NULL CHECK (
        length(unused_pos_hex) = 16
        AND unused_pos_hex NOT GLOB '*[^0-9a-f]*'
    ),
    write_pos_hex TEXT NOT NULL CHECK (
        length(write_pos_hex) = 16
        AND write_pos_hex NOT GLOB '*[^0-9a-f]*'
    ),
    committed_header_tail_hex TEXT NOT NULL CHECK (
        length(committed_header_tail_hex) = 16
        AND committed_header_tail_hex NOT GLOB '*[^0-9a-f]*'
    ),
    framing_safe_pos_hex TEXT NOT NULL CHECK (
        length(framing_safe_pos_hex) = 16
        AND framing_safe_pos_hex NOT GLOB '*[^0-9a-f]*'
    ),
    namespace_safe_pos_hex TEXT CHECK (
        namespace_safe_pos_hex IS NULL
        OR (
            length(namespace_safe_pos_hex) = 16
            AND namespace_safe_pos_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    sequence_safe_pos_hex TEXT NOT NULL CHECK (
        length(sequence_safe_pos_hex) = 16
        AND sequence_safe_pos_hex NOT GLOB '*[^0-9a-f]*'
    ),
    stream_format TEXT NOT NULL CHECK (stream_format IN ('legacy', 'resilient')),
    framing_status TEXT NOT NULL CHECK (
        framing_status IN ('clean', 'complete_to_header_tail', 'incomplete')
    ),
    stop_reason TEXT CHECK (
        stop_reason IS NULL
        OR (
            length(stop_reason) BETWEEN 1 AND 64
            AND stop_reason NOT GLOB '*[^0-9a-z_-]*'
        )
    ),
    namespace_stop_reason TEXT CHECK (
        namespace_stop_reason IS NULL
        OR (
            length(namespace_stop_reason) BETWEEN 1 AND 64
            AND namespace_stop_reason NOT GLOB '*[^0-9a-z_-]*'
        )
    ),
    sequence_stop_reason TEXT CHECK (
        sequence_stop_reason IS NULL
        OR sequence_stop_reason IN (
            'conflict',
            'unknown_event',
            'unsupported_semantics',
            'overflow'
        )
    ),
    event_count INTEGER NOT NULL CHECK (event_count >= 0),
    input_sha256 TEXT NOT NULL CHECK (
        length(input_sha256) = 64
        AND input_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    consensus_replay_sha256 TEXT NOT NULL CHECK (
        length(consensus_replay_sha256) = 64
        AND consensus_replay_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    projection_sha256 TEXT NOT NULL CHECK (
        length(projection_sha256) = 64
        AND projection_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (filesystem_identity, inventory_id, rank),
    FOREIGN KEY (filesystem_identity, inventory_id)
        REFERENCES ceph_fs_metadata_inventories(filesystem_identity, inventory_id)
        ON DELETE CASCADE,
    FOREIGN KEY (filesystem_identity, inventory_id, data_source_id)
        REFERENCES ceph_fs_metadata_inventories(
            filesystem_identity,
            inventory_id,
            data_source_id
        ) ON DELETE CASCADE,
    FOREIGN KEY (
        filesystem_identity,
        inventory_id,
        pointer_object_identity_sha256,
        pointer_locator
    ) REFERENCES ceph_fs_metadata_objects(
        filesystem_identity,
        inventory_id,
        object_identity_sha256,
        locator
    ) ON DELETE CASCADE,
    FOREIGN KEY (
        filesystem_identity,
        inventory_id,
        header_object_identity_sha256,
        header_locator
    ) REFERENCES ceph_fs_metadata_objects(
        filesystem_identity,
        inventory_id,
        object_identity_sha256,
        locator
    ) ON DELETE CASCADE,
    CHECK (pointer_front_inode_hex = journal_inode_hex),
    CHECK (trimmed_pos_hex <= expire_pos_hex),
    -- Journaler Header::unused_pos is not a committed ordering boundary and
    -- may legitimately be zero on real images.
    CHECK (committed_header_tail_hex = write_pos_hex),
    CHECK (expire_pos_hex <= framing_safe_pos_hex),
    CHECK (framing_safe_pos_hex <= write_pos_hex),
    CHECK (
        namespace_safe_pos_hex IS NULL
        OR (
            expire_pos_hex <= namespace_safe_pos_hex
            AND namespace_safe_pos_hex <= framing_safe_pos_hex
        )
    ),
    CHECK (expire_pos_hex <= sequence_safe_pos_hex),
    CHECK (sequence_safe_pos_hex <= framing_safe_pos_hex),
    CHECK (
        (sequence_stop_reason IS NULL
            AND sequence_safe_pos_hex = framing_safe_pos_hex)
        OR (sequence_stop_reason IS NOT NULL
            AND sequence_safe_pos_hex < framing_safe_pos_hex)
    ),
    CHECK (
        (framing_status = 'clean'
            AND stop_reason IS NULL
            AND expire_pos_hex = write_pos_hex
            AND framing_safe_pos_hex = write_pos_hex
            AND event_count = 0)
        OR (framing_status = 'complete_to_header_tail'
            AND stop_reason IS NULL
            AND framing_safe_pos_hex = write_pos_hex)
        OR (framing_status = 'incomplete'
            AND stop_reason IS NOT NULL
            AND framing_safe_pos_hex < write_pos_hex)
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_journal_replays_source
ON ceph_fs_journal_replays(data_source_id, filesystem_identity, rank);

CREATE TABLE IF NOT EXISTS ceph_fs_journal_map_provenance (
    filesystem_identity TEXT NOT NULL,
    inventory_id TEXT NOT NULL,
    rank INTEGER NOT NULL,
    source_identity TEXT NOT NULL CHECK (
        length(source_identity) > 0 AND instr(source_identity, char(0)) = 0
    ),
    source_inventory_identity TEXT NOT NULL CHECK (
        length(source_inventory_identity) > 0
        AND instr(source_inventory_identity, char(0)) = 0
    ),
    captured_at TEXT NOT NULL CHECK (
        length(captured_at) BETWEEN 20 AND 40 AND instr(captured_at, char(0)) = 0
    ),
    raw_fsmap_snapshot_sha256 TEXT NOT NULL CHECK (
        length(raw_fsmap_snapshot_sha256) = 64
        AND raw_fsmap_snapshot_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    raw_mdsmap_snapshot_sha256 TEXT NOT NULL CHECK (
        length(raw_mdsmap_snapshot_sha256) = 64
        AND raw_mdsmap_snapshot_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY (
        filesystem_identity,
        inventory_id,
        rank,
        source_identity,
        source_inventory_identity
    ),
    FOREIGN KEY (filesystem_identity, inventory_id, rank)
        REFERENCES ceph_fs_journal_replays(filesystem_identity, inventory_id, rank)
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ceph_fs_journal_events (
    filesystem_identity TEXT NOT NULL,
    inventory_id TEXT NOT NULL,
    rank INTEGER NOT NULL,
    event_ordinal INTEGER NOT NULL CHECK (event_ordinal >= 0),
    segment_sequence_hex TEXT CHECK (
        segment_sequence_hex IS NULL
        OR (
            length(segment_sequence_hex) = 16
            AND segment_sequence_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    event_sequence_hex TEXT CHECK (
        event_sequence_hex IS NULL
        OR (
            length(event_sequence_hex) = 16
            AND event_sequence_hex NOT GLOB '*[^0-9a-f]*'
        )
    ),
    sequence_disposition TEXT NOT NULL CHECK (
        sequence_disposition IN ('resolved', 'semantic_unavailable', 'ignored_lid')
    ),
    logical_offset_hex TEXT NOT NULL CHECK (
        length(logical_offset_hex) = 16
        AND logical_offset_hex NOT GLOB '*[^0-9a-f]*'
    ),
    logical_end_hex TEXT NOT NULL CHECK (
        length(logical_end_hex) = 16
        AND logical_end_hex NOT GLOB '*[^0-9a-f]*'
        AND logical_offset_hex < logical_end_hex
    ),
    payload_length INTEGER NOT NULL CHECK (payload_length BETWEEN 0 AND 4294967295),
    payload_sha256 TEXT NOT NULL CHECK (
        length(payload_sha256) = 64
        AND payload_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    event_type INTEGER NOT NULL CHECK (event_type BETWEEN 0 AND 4294967295),
    event_kind TEXT NOT NULL CHECK (
        length(event_kind) BETWEEN 1 AND 64
        AND event_kind NOT GLOB '*[^0-9a-z_]*'
    ),
    event_encoding TEXT NOT NULL CHECK (event_encoding IN ('legacy', 'versioned')),
    event_version INTEGER CHECK (event_version IS NULL OR event_version BETWEEN 0 AND 255),
    event_compat_version INTEGER CHECK (
        event_compat_version IS NULL OR event_compat_version BETWEEN 0 AND 255
    ),
    PRIMARY KEY (filesystem_identity, inventory_id, rank, event_ordinal),
    FOREIGN KEY (filesystem_identity, inventory_id, rank)
        REFERENCES ceph_fs_journal_replays(filesystem_identity, inventory_id, rank)
        ON DELETE CASCADE,
    CHECK (
        (event_encoding = 'legacy'
            AND event_version IS NULL
            AND event_compat_version IS NULL)
        OR (event_encoding = 'versioned'
            AND event_version IS NOT NULL
            AND event_version > 0
            AND event_compat_version IS NOT NULL
            AND event_compat_version <= 1
            AND event_compat_version <= event_version)
    ),
    CHECK (
        CASE event_type
            WHEN 2 THEN event_kind = 'subtree_map'
            WHEN 3 THEN event_kind = 'export'
            WHEN 4 THEN event_kind = 'import_start'
            WHEN 5 THEN event_kind = 'import_finish'
            WHEN 6 THEN event_kind = 'fragment'
            WHEN 9 THEN event_kind = 'reset_journal'
            WHEN 10 THEN event_kind = 'session'
            WHEN 11 THEN event_kind = 'sessions_old'
            WHEN 12 THEN event_kind = 'sessions'
            WHEN 20 THEN event_kind = 'update'
            WHEN 21 THEN event_kind = 'peer_update'
            WHEN 22 THEN event_kind = 'open'
            WHEN 23 THEN event_kind = 'committed'
            WHEN 24 THEN event_kind = 'purged'
            WHEN 42 THEN event_kind = 'table_client'
            WHEN 43 THEN event_kind = 'table_server'
            WHEN 50 THEN event_kind = 'subtree_map_test'
            WHEN 51 THEN event_kind = 'noop'
            WHEN 100 THEN event_kind = 'segment'
            WHEN 101 THEN event_kind = 'lid'
            ELSE event_kind = 'unknown'
        END
    ),
    CHECK (
        (sequence_disposition = 'resolved' AND event_sequence_hex IS NOT NULL)
        OR (sequence_disposition = 'semantic_unavailable'
            AND segment_sequence_hex IS NULL
            AND event_sequence_hex IS NULL)
        OR (sequence_disposition = 'ignored_lid'
            AND event_type = 101
            AND event_kind = 'lid'
            AND segment_sequence_hex IS NULL
            AND event_sequence_hex IS NULL)
    ),
    CHECK (
        sequence_disposition <> 'resolved'
        OR event_type NOT IN (2, 9, 100, 101)
        OR (
            segment_sequence_hex IS NOT NULL
            AND event_sequence_hex = segment_sequence_hex
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_journal_events_sequence
ON ceph_fs_journal_events(
    filesystem_identity,
    inventory_id,
    rank,
    segment_sequence_hex,
    event_sequence_hex
);

CREATE TABLE IF NOT EXISTS ceph_fs_journal_event_spans (
    filesystem_identity TEXT NOT NULL,
    inventory_id TEXT NOT NULL,
    rank INTEGER NOT NULL,
    event_ordinal INTEGER NOT NULL,
    span_ordinal INTEGER NOT NULL CHECK (span_ordinal >= 0),
    object_locator TEXT NOT NULL CHECK (
        length(object_locator) > 0 AND instr(object_locator, char(0)) = 0
    ),
    object_identity_sha256 TEXT NOT NULL CHECK (
        length(object_identity_sha256) = 64
        AND object_identity_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    logical_offset_hex TEXT NOT NULL CHECK (
        length(logical_offset_hex) = 16
        AND logical_offset_hex NOT GLOB '*[^0-9a-f]*'
    ),
    object_offset_hex TEXT NOT NULL CHECK (
        length(object_offset_hex) = 16
        AND object_offset_hex NOT GLOB '*[^0-9a-f]*'
    ),
    range_length_hex TEXT NOT NULL CHECK (
        length(range_length_hex) = 16
        AND range_length_hex NOT GLOB '*[^0-9a-f]*'
        AND range_length_hex <> '0000000000000000'
    ),
    range_sha256 TEXT NOT NULL CHECK (
        length(range_sha256) = 64
        AND range_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY (
        filesystem_identity,
        inventory_id,
        rank,
        event_ordinal,
        span_ordinal
    ),
    FOREIGN KEY (filesystem_identity, inventory_id, rank, event_ordinal)
        REFERENCES ceph_fs_journal_events(
            filesystem_identity,
            inventory_id,
            rank,
            event_ordinal
        ) ON DELETE CASCADE,
    FOREIGN KEY (
        filesystem_identity,
        inventory_id,
        object_identity_sha256,
        object_locator
    ) REFERENCES ceph_fs_metadata_objects(
        filesystem_identity,
        inventory_id,
        object_identity_sha256,
        locator
    ) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_journal_event_spans_object
ON ceph_fs_journal_event_spans(
    filesystem_identity,
    inventory_id,
    object_identity_sha256
);

CREATE TRIGGER IF NOT EXISTS trg_ceph_fs_journal_object_delete_replay
BEFORE DELETE ON ceph_fs_metadata_objects
BEGIN
    DELETE FROM ceph_fs_journal_replays
    WHERE filesystem_identity = OLD.filesystem_identity
      AND inventory_id = OLD.inventory_id
      AND EXISTS (
          SELECT 1
          FROM ceph_fs_journal_event_spans AS span
          WHERE span.filesystem_identity = ceph_fs_journal_replays.filesystem_identity
            AND span.inventory_id = ceph_fs_journal_replays.inventory_id
            AND span.rank = ceph_fs_journal_replays.rank
            AND span.object_identity_sha256 = OLD.object_identity_sha256
            AND span.object_locator = OLD.locator
      );
END;
