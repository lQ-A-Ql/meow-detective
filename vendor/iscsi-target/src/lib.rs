//! A pure Rust iSCSI target implementation
//!
//! This library provides a reusable iSCSI target server that can be integrated
//! into storage applications. Users implement the `ScsiBlockDevice` trait to
//! provide the actual storage backend.
//!
//! # Example
//!
//! ```no_run
//! use iscsi_target::{IscsiTarget, ScsiBlockDevice, ScsiResult};
//!
//! struct MyStorage {
//!     data: Vec<u8>,
//! }
//!
//! impl ScsiBlockDevice for MyStorage {
//!     fn read(&self, lba: u64, blocks: u32, block_size: u32) -> ScsiResult<Vec<u8>> {
//!         let offset = (lba * block_size as u64) as usize;
//!         let len = (blocks * block_size) as usize;
//!         Ok(self.data[offset..offset + len].to_vec())
//!     }
//!
//!     fn write(&mut self, lba: u64, data: &[u8], block_size: u32) -> ScsiResult<()> {
//!         let offset = (lba * block_size as u64) as usize;
//!         self.data[offset..offset + data.len()].copy_from_slice(data);
//!         Ok(())
//!     }
//!
//!     fn capacity(&self) -> u64 {
//!         (self.data.len() / 512) as u64
//!     }
//!
//!     fn block_size(&self) -> u32 {
//!         512
//!     }
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = MyStorage { data: vec![0u8; 1024 * 1024] };
//! let target = IscsiTarget::builder()
//!     .bind_addr("0.0.0.0:3260")
//!     .target_name("iqn.2025-12.local:storage.disk1")
//!     .build(storage)?;
//! target.run()?;
//! # Ok(())
//! # }
//! ```

pub mod auth;
pub mod client;
pub mod error;
pub mod pdu;
pub mod scsi;
pub mod session;
pub mod target;
pub mod typestate_session;

pub use auth::{AuthConfig, ChapCredentials};
pub use client::IscsiClient;
pub use error::{IscsiError, ScsiResult};
pub use scsi::ScsiBlockDevice;
pub use target::{IscsiTarget, IscsiTargetBuilder, IscsiServer, IscsiServerBuilder};

/// Version of this library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
