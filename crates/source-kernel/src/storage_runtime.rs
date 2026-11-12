//! Source storage runtime facade.
//!
//! The Fjall source-store runtime, shared-store registry, storage handles, and
//! projection markers live in `persistence-source`; this module keeps the
//! existing `crate::storage_runtime::*` paths for application services.

pub use persistence_source::runtime::{
    close_case_source_stores, close_source_store, close_source_store_at_path,
    open_source_read_only, open_source_read_write, open_source_read_write_at_path,
    source_storage_root, CaseStorageHandle, ReadOnly, ReadOnlyCaseStorageHandle,
    ReadOnlySourceStorageHandle, ReadOnlyStorageRuntime, ReadWrite, ReadWriteCaseStorageHandle,
    ReadWriteSourceStorageHandle, ReadWriteStorageRuntime, SourceStorageHandle, StorageRuntime,
    StorageRuntimeError,
};
