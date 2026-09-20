ALTER TABLE ceph_rbd_derived_lineage
    ADD COLUMN replica_policy_fingerprint TEXT NOT NULL DEFAULT ''
    CHECK (
        length(replica_policy_fingerprint) = 0
        OR (
            length(replica_policy_fingerprint) = 64
            AND replica_policy_fingerprint NOT GLOB '*[^0-9a-f]*'
        )
    );
