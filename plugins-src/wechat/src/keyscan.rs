//! WeChat 4.x database-key recovery from Windows crash dumps (PAGEDU64).
//!
//! WCDB passes the SQLCipher key as an `x'<64 hex>'` ASCII literal (see the
//! `CipherHandle::setCipherKey` buffer), and those buffers remain visible in a
//! full memory dump long after the call. A plain byte-stream scan over the
//! dump therefore recovers every key literal without page-table translation.
//! Each candidate is verified offline against the target database's page-1
//! HMAC, so false positives are impossible to confuse with real keys.
//!
//! Keys are secrets: they are stored in `Zeroizing` buffers and never logged.

use crate::sqlcipher4;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zeroize::Zeroizing;

/// A unique 32-byte key candidate recovered from a dump, with the number of
/// memory locations it was found at (higher counts are more trustworthy).
pub struct KeyCandidate {
    pub key: Zeroizing<[u8; 32]>,
    pub occurrences: usize,
}

/// Stream-scan a memory dump for `x'<hex>'` SQLCipher key literals.
///
/// Both the 64-hex form (key only) and the 96-hex form (key + salt) are
/// recognized; for the latter only the leading 32 key bytes are used.
pub fn scan_dump_for_keys(dump: &Path) -> std::io::Result<Vec<KeyCandidate>> {
    let mut file = File::open(dump)?;
    let mut found: Vec<KeyCandidate> = Vec::new();
    let mut chunk = vec![0u8; 64 << 20];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&chunk[..n]);
        scan_buffer(&carry, &mut found);
        // Keep a tail window so literals spanning the chunk boundary survive.
        let keep = carry.len().min(128);
        carry.drain(..carry.len() - keep);
    }
    found.sort_by_key(|candidate| std::cmp::Reverse(candidate.occurrences));
    Ok(found)
}

fn scan_buffer(buf: &[u8], found: &mut Vec<KeyCandidate>) {
    let mut i = 0usize;
    while i + 2 + 64 < buf.len() {
        if buf[i] == b'x' && buf[i + 1] == b'\'' {
            for hex_len in [64usize, 96usize] {
                let end = i + 2 + hex_len;
                if end < buf.len() && buf[end] == b'\'' {
                    if let Some(key) = parse_hex_key(&buf[i + 2..end]) {
                        match found.iter_mut().find(|c| c.key[..] == key[..]) {
                            Some(candidate) => candidate.occurrences += 1,
                            None => found.push(KeyCandidate {
                                key: Zeroizing::new(key),
                                occurrences: 1,
                            }),
                        }
                    }
                    break;
                }
            }
        }
        i += 1;
    }
}

fn parse_hex_key(hex: &[u8]) -> Option<[u8; 32]> {
    if !hex.iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut key = [0u8; 32];
    for (idx, pair) in hex.chunks(2).take(32).enumerate() {
        key[idx] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(key)
}

/// Recover the key for one encrypted database from a dump: read page 1,
/// then return the first candidate whose page-1 HMAC verifies.
pub fn recover_key_for_db(
    candidates: &[KeyCandidate],
    db: &Path,
) -> std::io::Result<Option<Zeroizing<[u8; 32]>>> {
    let mut file = File::open(db)?;
    let mut page1 = vec![0u8; sqlcipher4::PAGE_SZ];
    file.read_exact(&mut page1)?;
    Ok(candidates
        .iter()
        .find(|candidate| sqlcipher4::validate_page1(&candidate.key, &page1))
        .map(|candidate| Zeroizing::new(*candidate.key)))
}

/// Convenience: scan a dump once and recover keys for many databases.
pub fn recover_keys_for_dbs(
    dump: &Path,
    dbs: &[&Path],
) -> std::io::Result<Vec<(String, Zeroizing<[u8; 32]>)>> {
    let candidates = scan_dump_for_keys(dump)?;
    let mut recovered = Vec::new();
    for db in dbs {
        if let Some(key) = recover_key_for_db(&candidates, db)? {
            let name = db
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            recovered.push((name, key));
        }
    }
    Ok(recovered)
}

/// Hex rendering for interactive tooling output only; never use in logs that
/// ship to the host.
pub fn key_to_hex(key: &[u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_64_hex_literal() {
        let text = format!("x'{}'", "ab".repeat(32));
        let mut found = Vec::new();
        scan_buffer(text.as_bytes(), &mut found);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key[0], 0xab);
    }

    #[test]
    fn parses_96_hex_literal_and_takes_key_prefix() {
        let text = format!("x'{}'", "cd".repeat(48));
        let mut found = Vec::new();
        scan_buffer(text.as_bytes(), &mut found);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key[0], 0xcd);
    }

    #[test]
    fn rejects_non_hex_and_deduplicates() {
        let good = format!("x'{}'", "01".repeat(32));
        let mut buf = good.clone().into_bytes();
        buf.extend_from_slice(b" noise ");
        buf.extend_from_slice(good.as_bytes());
        buf.extend_from_slice(b"x'not-hex-at-all'");
        let mut found = Vec::new();
        scan_buffer(&buf, &mut found);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].occurrences, 2);
    }
}
