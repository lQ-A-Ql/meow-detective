//! Foxmail 7 account, index, and mailbox container parsing.

mod account;
mod index;
mod mailbox;

pub use account::{
    decode_account_password, parse_account_info, parse_account_password, FoxmailAccountInfo,
    FoxmailAccountPassword, FoxmailAccountVersion, FoxmailPasswordError,
};
pub use index::{is_index, is_list_container, parse_index, FoxmailIndex, FoxmailIndexRecord};
pub use mailbox::{parse_box, FoxmailBox};
