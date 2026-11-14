use btrfs_driver::Filesystem;
use std::sync::Arc;

pub struct BtrfsReader {
    pub(crate) filesystem: Arc<Filesystem>,
}
