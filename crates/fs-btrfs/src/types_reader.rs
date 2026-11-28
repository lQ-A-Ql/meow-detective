use btrfs_driver::Filesystem;
use std::sync::Arc;

#[derive(Clone)]
pub struct BtrfsReader {
    pub(crate) filesystem: Arc<Filesystem>,
}
