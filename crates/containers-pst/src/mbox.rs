mod projection;
mod read;

use crate::{MboxMessage, PstError};
pub use io_mbox::format::MboxFormat as MboxVariant;
pub use read::{detect_variant, visit_mbox, MboxRecord, MboxSource};

pub fn parse_mbox(bytes: &[u8]) -> Result<Vec<MboxMessage>, PstError> {
    collect(bytes, true)
}

pub fn parse_mbox_metadata(bytes: &[u8]) -> Result<Vec<MboxMessage>, PstError> {
    collect(bytes, false)
}

fn collect(bytes: &[u8], contents: bool) -> Result<Vec<MboxMessage>, PstError> {
    let mut messages = Vec::new();
    visit_mbox(bytes, contents, |record| {
        messages.push(record.message.into());
        Ok(())
    })?;
    Ok(messages)
}
