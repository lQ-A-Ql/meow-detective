//! Controlled-emulation services.
//!
//! Owns the bypass, EFI fallback, filesystem-repair and OS-data cleanup
//! workflows layered on the copy-on-write virtual disks from
//! `evidence-emulation`. It sits above `evidence-access` and `source-kernel`
//! and must never depend on `app-services` or Tauri.

pub mod emulation_bypass;
pub mod emulation_cow_reader;
pub mod emulation_efi_fallback;
pub mod emulation_ext4_repair;
pub mod emulation_fs_repair;
pub mod emulation_linux_bypass;
pub mod emulation_osdata;
