//! Read-only Bolt container access through the Go-compatible bbolt implementation.
mod error;
mod model;
mod read;
mod walk;
#[cfg(feature = "worker")]
mod worker;

pub use error::BoltError;
pub use model::{BoltEntry, BoltMetadata, BoltReadOptions, BoltReadReport};
pub use read::read_bolt;
#[cfg(feature = "worker")]
pub use worker::exit_code as worker_exit_code;
