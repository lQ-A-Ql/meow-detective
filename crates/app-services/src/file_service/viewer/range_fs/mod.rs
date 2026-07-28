mod bitlocker;
mod common;
mod exfat;
mod fat;
mod linux;
mod ntfs;

pub(crate) use bitlocker::try_read_bitlocker_ntfs_range_for_descriptor;
pub(crate) use exfat::{
    try_read_exfat_image_range_for_descriptor, try_read_exfat_image_range_for_entry,
};
pub(crate) use fat::{try_read_fat_image_range_for_descriptor, try_read_fat_image_range_for_entry};
pub(crate) use linux::{
    try_read_linux_image_range_for_descriptor, try_read_linux_image_range_for_entry,
};
pub(crate) use ntfs::{
    try_read_ntfs_image_range_for_descriptor, try_read_ntfs_image_range_for_entry,
};
