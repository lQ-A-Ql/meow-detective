DROP TRIGGER storage_object_version_retirement;

CREATE INDEX idx_ceph_rbd_lineage_storage_object
    ON ceph_rbd_derived_lineage(parent_storage_object_id);
CREATE INDEX idx_ceph_fs_lineage_storage_object
    ON ceph_fs_derived_lineage(parent_storage_object_id);

CREATE TRIGGER storage_object_version_retirement
AFTER DELETE ON storage_object_versions
BEGIN
    DELETE FROM storage_objects
    WHERE id = OLD.storage_object_id
      AND NOT EXISTS (
          SELECT 1 FROM storage_object_versions WHERE storage_object_id = OLD.storage_object_id
      )
      AND NOT EXISTS (
          SELECT 1 FROM ceph_rbd_derived_lineage WHERE parent_storage_object_id = OLD.storage_object_id
      )
      AND NOT EXISTS (
          SELECT 1 FROM ceph_fs_derived_lineage WHERE parent_storage_object_id = OLD.storage_object_id
      );
END;

CREATE TRIGGER ceph_rbd_object_retirement
AFTER DELETE ON ceph_rbd_derived_lineage
BEGIN
    DELETE FROM storage_objects
    WHERE id = OLD.parent_storage_object_id
      AND NOT EXISTS (
          SELECT 1 FROM storage_object_versions WHERE storage_object_id = OLD.parent_storage_object_id
      )
      AND NOT EXISTS (
          SELECT 1 FROM ceph_rbd_derived_lineage WHERE parent_storage_object_id = OLD.parent_storage_object_id
      )
      AND NOT EXISTS (
          SELECT 1 FROM ceph_fs_derived_lineage WHERE parent_storage_object_id = OLD.parent_storage_object_id
      );
END;

CREATE TRIGGER ceph_fs_object_retirement
AFTER DELETE ON ceph_fs_derived_lineage
BEGIN
    DELETE FROM storage_objects
    WHERE id = OLD.parent_storage_object_id
      AND NOT EXISTS (
          SELECT 1 FROM storage_object_versions WHERE storage_object_id = OLD.parent_storage_object_id
      )
      AND NOT EXISTS (
          SELECT 1 FROM ceph_rbd_derived_lineage WHERE parent_storage_object_id = OLD.parent_storage_object_id
      )
      AND NOT EXISTS (
          SELECT 1 FROM ceph_fs_derived_lineage WHERE parent_storage_object_id = OLD.parent_storage_object_id
      );
END;
