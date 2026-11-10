//! Extended attributes (`XATTR_ITEM`).
//!
//! # Where they live
//!
//! An attribute is filed under the key `(ino, 24, hash)`, where `hash`
//! is [`crate::dir::name_hash`] of the *fully-qualified* name — `user.colour`, not
//! `colour`. Btrfs stores no namespace table and no prefix encoding: the
//! name on disk is the name a caller asks for, prefix included. That
//! makes this driver's job smaller than its siblings', where ext4 packs
//! the prefix into a one-byte index and EROFS keeps a dictionary of
//! long prefixes shared across the image.
//!
//! # Why an item is a list
//!
//! The key's offset is a hash, so two names on the same inode can share
//! a key. When they do, their records are concatenated inside that one
//! item, exactly as colliding directory entries are. Reading only the
//! first record would drop the second attribute silently — and silently
//! is the problem: a caller cannot tell a file with one attribute from a
//! file whose second attribute this driver could not see.
//!
//! Collisions are not hypothetical and they are not rare enough to
//! ignore: a 32-bit hash over a few hundred attribute names on one inode
//! is a birthday problem, and a name can be chosen to collide on
//! purpose. `tests/xattr_oracle.rs` sets two names that do — found by
//! searching the hash — and requires both to come back.
//!
//! # The record
//!
//! Same `struct btrfs_dir_item` the directory entries use, laid out at
//! [`crate::dir`]. For an attribute the interesting fields are the name
//! and the `data_len` bytes that follow it; the `location` key is
//! unused, and the `type` byte is [`ftype::XATTR`].
//!
//! # Read only
//!
//! Nothing here writes. Setting an attribute means inserting into a tree
//! that a transaction has to commit, which is a different piece of work
//! from decoding one.

use crate::dir::{ftype, parse_items};
use crate::error::{Error, Result};

/// One extended attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XattrEntry {
    /// The fully-qualified name, exactly as stored — namespace prefix
    /// included, not NUL-terminated, and not required to be UTF-8.
    pub name: Vec<u8>,
    /// The raw value. May be empty: a zero-length value is a real thing
    /// to store, and is not the same as the attribute being absent.
    pub value: Vec<u8>,
}

/// Parse the sequence of attribute records packed into one item's data.
///
/// # Errors
///
/// As `parse_items`, plus [`Error::BadSuperblock`] if a record's
/// `type` byte is not [`ftype::XATTR`].
///
/// # Why the type byte is checked
///
/// It is redundant against a correct caller — this is only ever handed
/// the data of an item whose key type is already
/// [`crate::dir::XATTR_ITEM_KEY`] — and that is exactly why it is cheap to keep. If
/// it ever fires, the item being read is not the item that was meant,
/// and the alternative to noticing is returning a directory entry's
/// name as an attribute with an empty value. The oracle fixture
/// confirms the kernel does write `8` here. `XATTR_ITEM_KEY` is
/// [`crate::dir::XATTR_ITEM_KEY`].
pub fn parse_xattr_items(data: &[u8]) -> Result<Vec<XattrEntry>> {
    parse_items(data, "extended attribute item")?
        .into_iter()
        .map(|r| {
            if r.ftype != ftype::XATTR {
                return Err(Error::BadSuperblock(format!(
                    "extended attribute {:?} carries file type {} rather than \
                     {} — this is not an XATTR_ITEM",
                    String::from_utf8_lossy(r.name),
                    r.ftype,
                    ftype::XATTR
                )));
            }
            Ok(XattrEntry {
                name: r.name.to_vec(),
                value: r.value.to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Unit tests over hand-built items.
    //!
    //! **Necessary but not sufficient**, for the same reason the sibling
    //! tests in `dir.rs` say: the encoder here uses the offsets the
    //! parser decodes with, so a misreading of the struct would be baked
    //! into both. What is settled here is the value's span, the
    //! packed-sequence handling and the type check. Whether the offsets
    //! are right at all is settled by `tests/xattr_oracle.rs` against a
    //! filesystem the Linux kernel wrote.

    use super::*;
    use crate::dir::{offsets, DIR_ITEM_HEADER_SIZE, XATTR_ITEM_KEY};

    fn encode(name: &[u8], value: &[u8], ft: u8) -> Vec<u8> {
        let mut b = vec![0u8; DIR_ITEM_HEADER_SIZE];
        b[offsets::DATA_LEN..offsets::DATA_LEN + 2]
            .copy_from_slice(&(value.len() as u16).to_le_bytes());
        b[offsets::NAME_LEN..offsets::NAME_LEN + 2]
            .copy_from_slice(&(name.len() as u16).to_le_bytes());
        b[offsets::TYPE] = ft;
        b.extend_from_slice(name);
        b.extend_from_slice(value);
        b
    }

    fn one(name: &[u8], value: &[u8]) -> Vec<u8> {
        encode(name, value, ftype::XATTR)
    }

    #[test]
    fn reads_a_name_and_its_value() {
        let got = parse_xattr_items(&one(b"user.colour", b"blue")).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, b"user.colour");
        assert_eq!(got[0].value, b"blue");
    }

    /// A zero-length value is a real attribute, not an absent one, and
    /// the two have to stay distinguishable.
    #[test]
    fn an_empty_value_is_still_an_attribute() {
        let got = parse_xattr_items(&one(b"user.flag", b"")).unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[0].value.is_empty());
    }

    /// The whole reason an item is parsed as a list: names that hash to
    /// the same key share one, and reading only the first would lose the
    /// rest without saying so.
    #[test]
    fn reads_several_attributes_packed_into_one_item() {
        let mut data = one(b"user.first", b"1");
        data.extend(one(b"user.second", b"22"));
        data.extend(one(b"user.third", b"333"));
        let got = parse_xattr_items(&data).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].name, b"user.first");
        assert_eq!(got[1].value, b"22");
        assert_eq!(got[2].name, b"user.third");
    }

    /// Values are arbitrary bytes — NULs, high bytes, anything.
    #[test]
    fn a_value_may_be_any_bytes_at_all() {
        let value = [0u8, 1, 0xff, 0x7f, b'\n', 0];
        let got = parse_xattr_items(&one(b"user.binary", &value)).unwrap();
        assert_eq!(got[0].value, value);
    }

    #[test]
    fn rejects_a_record_that_is_not_an_xattr() {
        let data = encode(b"notanattr", b"", crate::dir::ftype::REG_FILE);
        match parse_xattr_items(&data) {
            Err(Error::BadSuperblock(m)) => assert!(m.contains("file type"), "message: {m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_value_running_past_the_end() {
        let mut data = one(b"user.x", b"short");
        data[offsets::DATA_LEN..offsets::DATA_LEN + 2].copy_from_slice(&9000u16.to_le_bytes());
        assert!(matches!(
            parse_xattr_items(&data),
            Err(Error::BadSuperblock(_))
        ));
    }

    /// A value one byte shorter than it should be leaves a trailing byte
    /// the walk cannot account for, which must be an error rather than a
    /// quietly shorter list.
    #[test]
    fn rejects_data_the_records_do_not_exactly_consume() {
        let mut data = one(b"user.x", b"value");
        data.push(0);
        assert!(matches!(
            parse_xattr_items(&data),
            Err(Error::BadSuperblock(_))
        ));
    }

    #[test]
    fn empty_data_is_an_empty_list() {
        assert!(parse_xattr_items(&[]).unwrap().is_empty());
    }

    /// The key type is the format's, and it is what the tree walk ranges
    /// over. Pinned so a typo in it cannot pass silently.
    #[test]
    fn the_key_type_is_the_one_the_format_defines() {
        assert_eq!(XATTR_ITEM_KEY, 24);
    }
}
