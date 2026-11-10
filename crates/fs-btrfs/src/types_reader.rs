use btrfs_driver::Filesystem;

pub struct BtrfsReader {
    pub(crate) filesystem: Filesystem,
}
