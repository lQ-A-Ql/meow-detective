mod header;
mod props;
mod reader;
mod types;

pub mod error;
pub mod mbox;
pub mod ost;
pub mod pst;

pub use error::PstError;
pub use types::{MboxMessage, PstAttachment, PstCalendar, PstContact, PstFolder, PstMessage};

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
