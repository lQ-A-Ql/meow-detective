//! SQLite WAL frame validation, SQLCipher-4 page decryption, and merge of
//! un-checkpointed WAL data into a decrypted WeChat 4.x database image.
//!
//! Format notes (empirically confirmed against the qianqian-image WALs):
//! - The 32-byte WAL header is plaintext: magic `0x377f0682` (little-endian
//!   checksums), format version 3007000, page size 4096, checkpoint
//!   sequence, salt-1/salt-2, header checksum (over the first 24 bytes).
//! - Frames start at offset 32 with stride `24 + 4096`: a 24-byte plaintext
//!   frame header (pgno, post-commit db size — nonzero marks a commit
//!   frame, salt-1, salt-2, checksum-1/2) followed by one SQLCipher page
//!   (`ciphertext(4016) | IV(16) | HMAC(64)`). Page-1 frames keep the
//!   main-database page-1 layout: `salt(16) | ciphertext(4000) | IV(16) |
//!   HMAC(64)` — the salt prefix is carried into the WAL verbatim, and the
//!   HMAC still covers `ciphertext | IV | pgno_le` (verified against the
//!   qianqian-image WALs, where every page-1 frame starts with the
//!   database salt).
//! - Frame checksums chain cumulatively from the previous frame's checksum
//!   (from the header checksum for the first frame) over the frame header's
//!   first 8 bytes plus the 4096 page bytes, computed on the ciphertext
//!   with the standard SQLite `walChecksumBytes` recurrence.
//!
//! Captured WALs routinely contain frames whose salts do not match the
//! on-disk header: a checkpoint + WAL restart rewrites the header with a
//! fresh salt pair, and if no further frames were written the file still
//! holds the previous generation's frames. In that case the newest
//! generation is the contiguous run starting at frame 0; its first frame
//! cannot be chain-validated (the generation's header is gone), so it is
//! gated on the SQLCipher page HMAC instead, and frames after it chain off
//! its stored checksum. Frames belonging to older generations (higher
//! offsets, older salts) are already checkpointed into the main database
//! and are never applied.
//!
//! Merge semantics: validated frames are applied in order (later writes
//! overwrite earlier ones) up to and including the last commit frame;
//! frames past it belong to an uncommitted transaction and are dropped.
//! When the applied generation matches the on-disk header, its commit
//! db-size is authoritative and the merged image is truncated/extended to
//! it; a stale (previous-generation) run predates the last checkpoint, so
//! its db-size is older than the main file's and may only extend, never
//! shrink, the image. The returned image is standalone: the WAL
//! journal-mode version bytes (18/19) are downgraded to 1, mirroring
//! `db.rs`, so the result deserializes without a sidecar WAL.
//!
//! No `unsafe` in this module.

use crate::sqlcipher4;

const WAL_HDR_SZ: usize = 32;
const FRAME_HDR_SZ: usize = 24;
const WAL_MAGIC_LE: u32 = 0x377f_0682;
const WAL_MAGIC_BE: u32 = 0x377f_0683;
const WAL_FORMAT_VERSION: u32 = 3_007_000;
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
/// Valid first bytes of a B-tree page (interior/leaf table/index).
const BTREE_PAGE_TYPES: [u8; 4] = [0x02, 0x05, 0x0a, 0x0d];

/// Outcome counters of a WAL merge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalMergeReport {
    /// Complete frames physically present in the WAL file.
    pub frames_seen: usize,
    /// Frames of the selected (newest) generation that passed salt,
    /// checksum-chain, and page-HMAC validation.
    pub frames_valid: usize,
    /// Valid frames actually written into the image (up to the last
    /// commit frame).
    pub frames_applied: usize,
    /// Valid frames past the last commit frame (uncommitted transaction).
    pub frames_dropped_uncommitted: usize,
    /// Page writes performed (rewrites of the same page count each time).
    pub pages_written: usize,
    /// Page count of the merged image after the final size adjustment.
    pub final_page_count: usize,
}

/// Parsed WAL header.
#[derive(Debug, Clone)]
struct WalHeader {
    little_endian: bool,
    checkpoint_seq: u32,
    salt1: u32,
    salt2: u32,
    checksum: (u32, u32),
}

/// One parsed frame header plus the offset of its page data.
#[derive(Debug, Clone)]
struct FrameRef {
    pgno: u32,
    /// Post-commit database size in pages; nonzero marks a commit frame.
    db_size: u32,
    salt: (u32, u32),
    checksum: (u32, u32),
    data_offset: usize,
}

/// Per-generation statistics for the offline `walinfo` tooling.
#[derive(Debug, Clone)]
pub struct WalGenerationInfo {
    pub salt1: u32,
    pub salt2: u32,
    /// Index of the generation's first frame in the file.
    pub first_frame: usize,
    pub frame_count: usize,
    pub commit_frames: usize,
    pub max_pgno: u32,
    pub matches_header_salt: bool,
    /// Frames whose chained checksum verified against the previous frame
    /// (or the WAL header, when `matches_header_salt`). A generation's
    /// first frame is not countable here when its salts are stale.
    pub chain_verified_frames: usize,
}

/// Read-only WAL inventory returned by [`inspect`].
#[derive(Debug, Clone)]
pub struct WalInfo {
    pub page_size: u32,
    pub checkpoint_seq: u32,
    pub salt1: u32,
    pub salt2: u32,
    pub header_checksum_ok: bool,
    pub frames: usize,
    pub trailing_bytes: usize,
    pub generations: Vec<WalGenerationInfo>,
}

/// Standard SQLite `walChecksumBytes` recurrence over 8-byte word pairs.
fn wal_checksum(data: &[u8], s1: u32, s2: u32, little: bool) -> (u32, u32) {
    debug_assert!(data.len().is_multiple_of(8));
    let read = |at: usize| -> u32 {
        let bytes: [u8; 4] = data[at..at + 4].try_into().expect("word");
        if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        }
    };
    let (mut s1, mut s2) = (s1, s2);
    for pair in 0..data.len() / 8 {
        s1 = s1.wrapping_add(read(pair * 8)).wrapping_add(s2);
        s2 = s2.wrapping_add(read(pair * 8 + 4)).wrapping_add(s1);
    }
    (s1, s2)
}

fn be_u32(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(data[at..at + 4].try_into().expect("be word"))
}

/// Parse and validate the WAL header shape. The header checksum result is
/// returned alongside (callers decide whether a mismatch is fatal).
fn parse_header(wal: &[u8]) -> Result<(WalHeader, bool), String> {
    if wal.len() < WAL_HDR_SZ {
        return Err(format!("WAL too short for a header ({} bytes)", wal.len()));
    }
    let magic = be_u32(wal, 0);
    let little_endian = match magic {
        WAL_MAGIC_LE => true,
        WAL_MAGIC_BE => false,
        other => return Err(format!("bad WAL magic 0x{other:08x}")),
    };
    let version = be_u32(wal, 4);
    if version != WAL_FORMAT_VERSION {
        return Err(format!("unsupported WAL format version {version}"));
    }
    let page_size = match be_u32(wal, 8) {
        1 => 65536,
        n => n,
    };
    if page_size != sqlcipher4::PAGE_SZ as u32 {
        return Err(format!(
            "WAL page size {page_size} does not match SQLCipher page size {}",
            sqlcipher4::PAGE_SZ
        ));
    }
    let header = WalHeader {
        little_endian,
        checkpoint_seq: be_u32(wal, 12),
        salt1: be_u32(wal, 16),
        salt2: be_u32(wal, 20),
        checksum: (be_u32(wal, 24), be_u32(wal, 28)),
    };
    let computed = wal_checksum(&wal[..24], 0, 0, little_endian);
    let checksum_ok = computed == header.checksum;
    Ok((header, checksum_ok))
}

/// Split the WAL body into complete frames; trailing partial bytes are
/// reported through `trailing`.
fn parse_frames(wal: &[u8]) -> (Vec<FrameRef>, usize) {
    let stride = FRAME_HDR_SZ + sqlcipher4::PAGE_SZ;
    let body = wal.len().saturating_sub(WAL_HDR_SZ);
    let count = body / stride;
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let at = WAL_HDR_SZ + index * stride;
        frames.push(FrameRef {
            pgno: be_u32(wal, at),
            db_size: be_u32(wal, at + 4),
            salt: (be_u32(wal, at + 8), be_u32(wal, at + 12)),
            checksum: (be_u32(wal, at + 16), be_u32(wal, at + 20)),
            data_offset: at + FRAME_HDR_SZ,
        });
    }
    (frames, body % stride)
}

/// Group frames into contiguous same-salt runs (generations).
fn generation_runs(frames: &[FrameRef]) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        match runs.last_mut() {
            Some((_, end)) if frames[*end - 1].salt == frame.salt => *end = index + 1,
            _ => runs.push((index, index + 1)),
        }
    }
    runs
}

/// Merge un-checkpointed WAL frames into a decrypted database image.
///
/// - `key`: raw 32-byte SQLCipher key.
/// - `salt`: 16-byte page-1 salt of the encrypted main database (drives the
///   page-HMAC mac key); the caller reads it from the encrypted file.
/// - `plain_image`: output of [`sqlcipher4::decrypt_database`] — page 1 is
///   `header(16) | content(4000) | reserve(80)`, later pages
///   `content(4016) | reserve(80)`, total size a 4096 multiple.
/// - `wal`: raw `.db-wal` bytes. An empty WAL merges as a no-op.
///
/// Returns the merged standalone image plus the merge report.
pub fn merge(
    key: &[u8; 32],
    salt: &[u8],
    plain_image: &[u8],
    wal: &[u8],
) -> Result<(Vec<u8>, WalMergeReport), String> {
    if plain_image.len() < sqlcipher4::PAGE_SZ
        || !plain_image.len().is_multiple_of(sqlcipher4::PAGE_SZ)
        || !plain_image.starts_with(SQLITE_HEADER)
    {
        return Err(format!(
            "decrypted image ({} bytes) is not a page-aligned SQLite image",
            plain_image.len()
        ));
    }
    let mut report = WalMergeReport::default();
    let mut image = plain_image.to_vec();
    if wal.is_empty() {
        downgrade_journal_versions(&mut image);
        report.final_page_count = image.len() / sqlcipher4::PAGE_SZ;
        return Ok((image, report));
    }
    let (header, header_checksum_ok) = parse_header(wal)?;
    let (frames, _trailing) = parse_frames(wal);
    report.frames_seen = frames.len();
    if frames.is_empty() {
        downgrade_journal_versions(&mut image);
        report.final_page_count = image.len() / sqlcipher4::PAGE_SZ;
        return Ok((image, report));
    }

    // The mergeable generation is the contiguous run starting at frame 0.
    // Standard mode: its salt pair matches the on-disk header and the
    // checksum chain seeds from the header checksum. Forensic mode: the
    // header was rewritten by a checkpoint + restart and the frames carry
    // the previous generation's salts; frame 0 is then gated on its
    // page HMAC alone and the chain resumes from its stored checksum.
    let run_end = generation_runs(&frames)
        .first()
        .map(|(_, end)| *end)
        .unwrap_or(0);
    let run_salt = frames[0].salt;
    let standard_mode = run_salt == (header.salt1, header.salt2);
    if standard_mode && !header_checksum_ok {
        return Err("WAL header checksum mismatch".to_string());
    }
    let mut chain_seed = standard_mode.then_some(header.checksum);

    let mut valid: Vec<&FrameRef> = Vec::new();
    for frame in &frames[..run_end] {
        if frame.pgno == 0 {
            break;
        }
        if let Some(seed) = chain_seed {
            let header_end = frame.data_offset - FRAME_HDR_SZ;
            let (s1, s2) = wal_checksum(
                &wal[header_end..header_end + 8],
                seed.0,
                seed.1,
                header.little_endian,
            );
            let page_end = frame.data_offset + sqlcipher4::PAGE_SZ;
            let chained = wal_checksum(
                &wal[frame.data_offset..page_end],
                s1,
                s2,
                header.little_endian,
            );
            if chained != frame.checksum {
                break; // checksum chain broken: this frame and beyond are lost
            }
        }
        let page = &wal[frame.data_offset..frame.data_offset + sqlcipher4::PAGE_SZ];
        let Some(crypto_view) = frame_crypto_view(page, frame.pgno, salt) else {
            break; // page-1 salt prefix mismatch: truncate the run here
        };
        if !sqlcipher4::page_hmac_valid(key, salt, frame.pgno, crypto_view) {
            break; // page integrity failure: truncate the run here
        }
        chain_seed = Some(frame.checksum);
        valid.push(frame);
    }
    report.frames_valid = valid.len();

    let last_commit = valid.iter().rposition(|frame| frame.db_size != 0);
    let image_pages = image.len() / sqlcipher4::PAGE_SZ;
    let (applied, committed_pages) = match last_commit {
        Some(at) => (&valid[..=at], valid[at].db_size as usize),
        None => (&[][..], image_pages),
    };
    report.frames_applied = applied.len();
    report.frames_dropped_uncommitted = valid.len() - applied.len();
    // Sizing: when the generation is current (salts match the header) its
    // commit db-size is authoritative and the image is truncated/extended
    // to it. A stale generation predates the last checkpoint, so its
    // db-size is older than the main file's — never shrink on stale data,
    // only extend.
    let final_size_pages = if standard_mode {
        committed_pages
    } else {
        committed_pages.max(image_pages)
    };

    for frame in applied {
        let page = &wal[frame.data_offset..frame.data_offset + sqlcipher4::PAGE_SZ];
        let crypto_view = frame_crypto_view(page, frame.pgno, salt)
            .expect("applied frames passed the validation gate");
        let decrypted = sqlcipher4::decrypt_page(key, frame.pgno, crypto_view)?;
        let offset = (frame.pgno as usize - 1) * sqlcipher4::PAGE_SZ;
        if image.len() < offset + sqlcipher4::PAGE_SZ {
            image.resize(offset + sqlcipher4::PAGE_SZ, 0);
        }
        if frame.pgno == 1 {
            // Page-1 frames carry the salt prefix; the decrypted body maps
            // to image bytes 16..4096 and the SQLite header is restored
            // from the constant (mirroring decrypt_database).
            debug_assert_eq!(decrypted.len(), sqlcipher4::PAGE_SZ - sqlcipher4::SALT_SZ);
            image[offset..offset + sqlcipher4::SALT_SZ].copy_from_slice(SQLITE_HEADER);
            image[offset + sqlcipher4::SALT_SZ..offset + sqlcipher4::PAGE_SZ]
                .copy_from_slice(&decrypted);
        } else {
            debug_assert_eq!(decrypted.len(), sqlcipher4::PAGE_SZ);
            image[offset..offset + sqlcipher4::PAGE_SZ].copy_from_slice(&decrypted);
        }
        report.pages_written += 1;
    }
    image.resize(final_size_pages * sqlcipher4::PAGE_SZ, 0);
    downgrade_journal_versions(&mut image);
    report.final_page_count = final_size_pages;
    Ok((image, report))
}

/// Read-only WAL inventory for the offline `walinfo` tooling: header fields
/// plus per-generation frame/commit/chain statistics. No key is needed —
/// frame checksums and salts are plaintext.
pub fn inspect(wal: &[u8]) -> Result<WalInfo, String> {
    let (header, header_checksum_ok) = parse_header(wal)?;
    let (frames, trailing) = parse_frames(wal);
    let runs = generation_runs(&frames);
    let mut generations = Vec::with_capacity(runs.len());
    for (start, end) in runs {
        let run = &frames[start..end];
        let salt = run[0].salt;
        let matches_header = salt == (header.salt1, header.salt2);
        // Chain validation: seed from the header checksum when the run's
        // salts are current. A stale generation's first frame has no
        // verifiable seed on disk (its own header and predecessor frame
        // were overwritten by a newer generation), so it is skipped and
        // the chain resumes from its stored checksum; SQLCipher page HMACs
        // remain the integrity gate for that first frame.
        let mut seed = matches_header.then_some(header.checksum);
        let mut verified = 0;
        for frame in run {
            if let Some((s1, s2)) = seed {
                let header_end = frame.data_offset - FRAME_HDR_SZ;
                let (s1, s2) = wal_checksum(
                    &wal[header_end..header_end + 8],
                    s1,
                    s2,
                    header.little_endian,
                );
                let chained = wal_checksum(
                    &wal[frame.data_offset..frame.data_offset + sqlcipher4::PAGE_SZ],
                    s1,
                    s2,
                    header.little_endian,
                );
                if chained != frame.checksum {
                    break;
                }
                verified += 1;
            }
            seed = Some(frame.checksum);
        }
        generations.push(WalGenerationInfo {
            salt1: salt.0,
            salt2: salt.1,
            first_frame: start,
            frame_count: run.len(),
            commit_frames: run.iter().filter(|f| f.db_size != 0).count(),
            max_pgno: run.iter().map(|f| f.pgno).max().unwrap_or(0),
            matches_header_salt: matches_header,
            chain_verified_frames: verified,
        });
    }
    Ok(WalInfo {
        page_size: sqlcipher4::PAGE_SZ as u32,
        checkpoint_seq: header.checkpoint_seq,
        salt1: header.salt1,
        salt2: header.salt2,
        header_checksum_ok,
        frames: frames.len(),
        trailing_bytes: trailing,
        generations,
    })
}

/// Flip the file-format read/write version bytes to rollback mode (1) so
/// the merged image opens/deserializes without a sidecar WAL, mirroring
/// the downgrade `db.rs` applies on its private copy.
fn downgrade_journal_versions(image: &mut [u8]) {
    if image.len() >= 20 && image.starts_with(SQLITE_HEADER) {
        image[18] = 1;
        image[19] = 1;
    }
}

/// The HMAC/decryption view of a frame's page data. Page-1 frames keep the
/// main-database layout (`salt | ciphertext | IV | HMAC`); the salt prefix
/// must match the database salt and is stripped before crypto. Any other
/// page is used whole. Returns `None` on a page-1 salt mismatch.
fn frame_crypto_view<'a>(page: &'a [u8], pgno: u32, salt: &[u8]) -> Option<&'a [u8]> {
    if pgno == 1 {
        if page.len() != sqlcipher4::PAGE_SZ || !page[..sqlcipher4::SALT_SZ].eq(salt) {
            return None;
        }
        Some(&page[sqlcipher4::SALT_SZ..])
    } else {
        Some(page)
    }
}

/// Exposed for tests and the offline tooling sanity output.
pub fn is_btree_page_type(byte: u8) -> bool {
    BTREE_PAGE_TYPES.contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    use hmac::{Hmac, Mac};
    use sha2::Sha512;

    const SALT: [u8; sqlcipher4::SALT_SZ] = [0x5au8; sqlcipher4::SALT_SZ];

    fn key() -> [u8; 32] {
        [0x42u8; 32]
    }

    fn patterned_page(tag: u8) -> Vec<u8> {
        (0..sqlcipher4::PAGE_SZ)
            .map(|i| tag ^ (i % 251) as u8)
            .collect()
    }

    fn mac_tag(key: &[u8; 32], salt: &[u8], pgno: u32, body: &[u8], iv: &[u8; 16]) -> [u8; 64] {
        let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
        let mac_key = pbkdf2::pbkdf2_hmac_array::<Sha512, 32>(key, &mac_salt, 2);
        let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key[..]).expect("hmac");
        mac.update(body);
        mac.update(iv);
        mac.update(&pgno.to_le_bytes());
        mac.finalize().into_bytes().into()
    }

    /// SQLCipher-encrypt the first 4016 bytes of one plaintext page with a
    /// deterministic per-page IV; the tail 80 bytes become IV + HMAC,
    /// matching the on-disk WCDB frame layout.
    fn encrypt_frame_page_with_iv(
        key: &[u8; 32],
        salt: &[u8],
        pgno: u32,
        plain: &[u8],
        iv: [u8; 16],
    ) -> Vec<u8> {
        type Enc = cbc::Encryptor<aes::Aes256>;
        assert_eq!(plain.len(), sqlcipher4::PAGE_SZ);
        let mut body = plain[..sqlcipher4::PAGE_SZ - 80].to_vec();
        let body_len = body.len();
        Enc::new((&key[..]).into(), (&iv[..]).into())
            .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut body, body_len)
            .expect("encrypt");
        let tag = mac_tag(key, salt, pgno, &body, &iv);
        body.extend_from_slice(&iv);
        body.extend_from_slice(&tag);
        body
    }

    fn encrypt_frame_page(key: &[u8; 32], salt: &[u8], pgno: u32, plain: &[u8]) -> Vec<u8> {
        encrypt_frame_page_with_iv(key, salt, pgno, plain, [pgno as u8 ^ 0xA5; 16])
    }

    /// Encrypt one plaintext image page the way WCDB writes it into a WAL
    /// frame: page 1 keeps its salt prefix (ciphertext covers bytes
    /// 16..4016); other pages encrypt bytes 0..4016.
    fn encrypt_wal_page(key: &[u8; 32], salt: &[u8], pgno: u32, plain: &[u8]) -> Vec<u8> {
        if pgno == 1 {
            let mut body = plain[16..sqlcipher4::PAGE_SZ - 80].to_vec();
            type Enc = cbc::Encryptor<aes::Aes256>;
            let iv = [0x77u8; 16];
            let body_len = body.len();
            Enc::new((&key[..]).into(), (&iv[..]).into())
                .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut body, body_len)
                .expect("encrypt page 1 frame");
            let tag = mac_tag(key, salt, 1, &body, &iv);
            let mut out = salt.to_vec();
            out.extend_from_slice(&body);
            out.extend_from_slice(&iv);
            out.extend_from_slice(&tag);
            out
        } else {
            encrypt_frame_page(key, salt, pgno, plain)
        }
    }

    /// One frame of a synthetic WAL.
    struct FrameSpec {
        pgno: u32,
        /// Post-commit database size in pages; nonzero marks a commit frame.
        db_size: u32,
        salt: (u32, u32),
        page: Vec<u8>,
    }

    fn frame(pgno: u32, db_size: u32, salt: (u32, u32), tag: u8) -> FrameSpec {
        FrameSpec {
            pgno,
            db_size,
            salt,
            page: encrypt_wal_page(&key(), &SALT, pgno, &patterned_page(tag)),
        }
    }

    /// Build a WAL with a correct header checksum and a correct cumulative
    /// frame-checksum chain (computed over whatever frame bytes are given,
    /// so encrypted or corrupted content can be injected freely).
    fn build_wal(seq: u32, header_salt: (u32, u32), frames: &[FrameSpec]) -> Vec<u8> {
        let mut header = Vec::with_capacity(WAL_HDR_SZ);
        for value in [
            WAL_MAGIC_LE,
            WAL_FORMAT_VERSION,
            sqlcipher4::PAGE_SZ as u32,
            seq,
            header_salt.0,
            header_salt.1,
        ] {
            header.extend_from_slice(&value.to_be_bytes());
        }
        let hck = wal_checksum(&header, 0, 0, true);
        let mut wal = Vec::new();
        wal.extend_from_slice(&header);
        wal.extend_from_slice(&hck.0.to_be_bytes());
        wal.extend_from_slice(&hck.1.to_be_bytes());
        let mut chain = hck;
        for frame in frames {
            let mut fh = Vec::with_capacity(FRAME_HDR_SZ);
            fh.extend_from_slice(&frame.pgno.to_be_bytes());
            fh.extend_from_slice(&frame.db_size.to_be_bytes());
            fh.extend_from_slice(&frame.salt.0.to_be_bytes());
            fh.extend_from_slice(&frame.salt.1.to_be_bytes());
            chain = wal_checksum(&fh[..8], chain.0, chain.1, true);
            chain = wal_checksum(&frame.page, chain.0, chain.1, true);
            fh.extend_from_slice(&chain.0.to_be_bytes());
            fh.extend_from_slice(&chain.1.to_be_bytes());
            wal.extend_from_slice(&fh);
            wal.extend_from_slice(&frame.page);
        }
        wal
    }

    /// A synthetic "decrypted image": SQLite header plus patterned pages.
    fn plain_image(pages: usize) -> Vec<u8> {
        let mut image = patterned_page(0x00);
        image[..16].copy_from_slice(SQLITE_HEADER);
        image[18] = 2; // WAL journal versions, as decrypt_database emits them
        image[19] = 2;
        for tag in 1..pages {
            image.extend_from_slice(&patterned_page(tag as u8));
        }
        image
    }

    /// The merged page's first 4016 bytes match the frame plaintext; the
    /// reserve tail stays IV + HMAC bytes by design.
    fn assert_page_content(merged: &[u8], pgno: u32, tag: u8) {
        let offset = (pgno as usize - 1) * sqlcipher4::PAGE_SZ;
        let expected = patterned_page(tag);
        assert_eq!(
            &merged[offset..offset + sqlcipher4::PAGE_SZ - 80],
            &expected[..sqlcipher4::PAGE_SZ - 80],
            "page {pgno} content"
        );
    }

    #[test]
    fn committed_frames_apply_in_order_and_downgrade_versions() {
        let salt = (0xAAAA0001, 0xBBBB0002);
        let frames = vec![
            frame(2, 0, salt, 0x11),
            frame(1, 0, salt, 0x22), // page-1 frame: salt-prefixed layout
            frame(2, 3, salt, 0x33), // commit; rewrites page 2
        ];
        let wal = build_wal(7, salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(
            report,
            WalMergeReport {
                frames_seen: 3,
                frames_valid: 3,
                frames_applied: 3,
                frames_dropped_uncommitted: 0,
                pages_written: 3,
                final_page_count: 3,
            }
        );
        assert_eq!(merged.len(), 3 * sqlcipher4::PAGE_SZ);
        assert_page_content(&merged, 2, 0x33); // later write wins
        assert_page_content(&merged, 3, 2); // page 3 untouched (tag 2)
        assert_eq!(&merged[..16], SQLITE_HEADER, "page-1 header restored");
        assert_eq!(&merged[18..20], &[1, 1], "journal versions downgraded");
    }

    #[test]
    fn uncommitted_tail_frames_are_dropped() {
        let salt = (1, 2);
        let frames = vec![
            frame(2, 3, salt, 0x11), // commit
            frame(3, 0, salt, 0x22), // uncommitted
            frame(2, 0, salt, 0x33), // uncommitted rewrite of page 2
        ];
        let wal = build_wal(1, salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_seen, 3);
        assert_eq!(report.frames_valid, 3);
        assert_eq!(report.frames_applied, 1);
        assert_eq!(report.frames_dropped_uncommitted, 2);
        assert_eq!(report.pages_written, 1);
        assert_page_content(&merged, 2, 0x11);
    }

    #[test]
    fn wal_without_commit_frame_applies_nothing() {
        let salt = (1, 2);
        let frames = vec![frame(2, 0, salt, 0x11), frame(3, 0, salt, 0x22)];
        let wal = build_wal(1, salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_valid, 2);
        assert_eq!(report.frames_applied, 0);
        assert_eq!(report.frames_dropped_uncommitted, 2);
        assert_eq!(report.pages_written, 0);
        let mut expected = image;
        expected[18] = 1; // only the journal-version downgrade lands
        expected[19] = 1;
        assert_eq!(merged, expected);
    }

    #[test]
    fn salt_change_ends_the_mergeable_generation() {
        let salt = (1, 2);
        let older = (0, 9); // older generation remnant: never applied
        let frames = vec![
            frame(2, 0, salt, 0x11),
            frame(3, 3, salt, 0x22), // commit
            frame(2, 0, older, 0x33),
            frame(3, 3, older, 0x44),
        ];
        let wal = build_wal(1, salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_seen, 4);
        assert_eq!(report.frames_valid, 2);
        assert_eq!(report.frames_applied, 2);
        assert_page_content(&merged, 2, 0x11);
        assert_page_content(&merged, 3, 0x22);
    }

    #[test]
    fn checksum_chain_break_truncates_the_run() {
        let salt = (1, 2);
        let frames = vec![
            frame(2, 3, salt, 0x11), // commit
            frame(3, 0, salt, 0x22),
            frame(2, 3, salt, 0x33),
        ];
        let mut wal = build_wal(1, salt, &frames);
        // Corrupt one page byte of frame 1 (header + frame 0 + frame header).
        let at = WAL_HDR_SZ + FRAME_HDR_SZ + sqlcipher4::PAGE_SZ + FRAME_HDR_SZ;
        wal[at] ^= 0xFF;
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_seen, 3);
        assert_eq!(report.frames_valid, 1, "chain break stops validation");
        assert_eq!(report.frames_applied, 1);
        assert_page_content(&merged, 2, 0x11);
    }

    #[test]
    fn page_hmac_failure_truncates_the_run() {
        let salt = (1, 2);
        // Re-encrypt frame 1's page under a different key: the checksum
        // chain stays intact but the SQLCipher page HMAC fails.
        let mut bad = frame(3, 0, salt, 0x22);
        bad.page = encrypt_frame_page(&[0x43u8; 32], &SALT, 3, &patterned_page(0x22));
        let frames = vec![frame(2, 3, salt, 0x11), bad, frame(2, 3, salt, 0x33)];
        let wal = build_wal(1, salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_valid, 1);
        assert_eq!(report.frames_applied, 1);
        assert_page_content(&merged, 2, 0x11);
    }

    #[test]
    fn stale_salt_generation_recovers_via_consecutive_chain() {
        // Captured-WAL case (the real qianqian-image layout): the on-disk
        // header belongs to a later, empty generation; the frames carry
        // the previous generation's salts.
        let header_salt = (0xF6AE_AA83, 0x5194_F985);
        let frame_salt = (0xF6AE_AA82, 0x522F_EEC6);
        let frames = vec![
            frame(4, 0, frame_salt, 0x11),
            frame(5, 0, frame_salt, 0x22),
            frame(4, 5, frame_salt, 0x33), // commit, extends the db
        ];
        let wal = build_wal(1, header_salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_seen, 3);
        assert_eq!(report.frames_valid, 3);
        assert_eq!(report.frames_applied, 3);
        assert_eq!(report.final_page_count, 5);
        assert_eq!(merged.len(), 5 * sqlcipher4::PAGE_SZ);
        assert_page_content(&merged, 4, 0x33); // later write wins
        assert_page_content(&merged, 5, 0x22);
    }

    #[test]
    fn stale_generation_never_shrinks_the_image() {
        // A stale generation's commit db-size predates the main file's
        // size; the merge must not truncate the larger current image.
        let header_salt = (0xF6AE_AA83, 0x5194_F985);
        let frame_salt = (0xF6AE_AA82, 0x522F_EEC6);
        let frames = vec![frame(2, 2, frame_salt, 0x11)]; // commit dbsize 2 < 3
        let wal = build_wal(1, header_salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_applied, 1);
        assert_eq!(report.final_page_count, 3, "stale db-size cannot shrink");
        assert_eq!(merged.len(), 3 * sqlcipher4::PAGE_SZ);
        assert_page_content(&merged, 2, 0x11);
    }

    #[test]
    fn stale_generation_first_frame_still_needs_valid_hmac() {
        let header_salt = (0xF6AE_AA83, 0x5194_F985);
        let frame_salt = (0xF6AE_AA82, 0x522F_EEC6);
        let frames = vec![
            FrameSpec {
                pgno: 2,
                db_size: 0,
                salt: frame_salt,
                page: encrypt_frame_page(&[0x43u8; 32], &SALT, 2, &patterned_page(0x11)),
            },
            frame(3, 3, frame_salt, 0x22),
        ];
        let wal = build_wal(1, header_salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_valid, 0, "frame 0 fails its HMAC gate");
        assert_eq!(report.frames_applied, 0);
        let mut expected = image;
        expected[18] = 1; // only the journal-version downgrade lands
        expected[19] = 1;
        assert_eq!(merged, expected);
    }

    #[test]
    fn page1_frame_with_wrong_salt_prefix_is_rejected() {
        let salt = (1, 2);
        let mut bad_page1 = frame(1, 0, salt, 0x55);
        // Corrupt the salt prefix: the page-1 frame no longer belongs to
        // this database and must truncate the run.
        bad_page1.page[0] ^= 0xFF;
        // Rebuild so the checksum chain matches the corrupted bytes; only
        // the salt-prefix/HMAC gate may catch it.
        let frames = vec![frame(2, 2, salt, 0x11), bad_page1, frame(3, 3, salt, 0x33)];
        let wal = build_wal(1, salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.frames_valid, 1, "corrupt page-1 frame stops the run");
        assert_eq!(report.frames_applied, 1);
        assert_page_content(&merged, 2, 0x11);
    }

    #[test]
    fn final_size_truncates_to_last_commit() {
        let salt = (1, 2);
        let frames = vec![frame(2, 2, salt, 0x11)]; // shrinks a 3-page db to 2
        let wal = build_wal(1, salt, &frames);
        let image = plain_image(3);
        let (merged, report) = merge(&key(), &SALT, &image, &wal).expect("merge");
        assert_eq!(report.final_page_count, 2);
        assert_eq!(merged.len(), 2 * sqlcipher4::PAGE_SZ);
    }

    #[test]
    fn malformed_inputs_error_and_empty_wal_is_noop() {
        let image = plain_image(3);
        let key = key();
        assert!(merge(&key, &SALT, &image, b"tiny").is_err());
        let mut bad_magic = build_wal(1, (1, 2), &[]);
        bad_magic[3] = 0x99;
        assert!(merge(&key, &SALT, &image, &bad_magic).is_err());
        let header_only = build_wal(1, (1, 2), &[]);
        let (merged, report) = merge(&key, &SALT, &image, &header_only).expect("merge");
        assert_eq!(report.frames_seen, 0);
        assert_eq!(report.final_page_count, 3);
        assert_eq!(&merged[18..20], &[1, 1]);
        let (merged, report) = merge(&key, &SALT, &image, &[]).expect("merge");
        assert_eq!(
            report,
            WalMergeReport {
                final_page_count: 3,
                ..WalMergeReport::default()
            }
        );
        assert_eq!(&merged[18..20], &[1, 1]);
        // A corrupt decrypted image is rejected up front.
        assert!(merge(&key, &SALT, &image[..100], &[]).is_err());
    }

    #[test]
    fn inspect_reports_generations_and_chain_coverage() {
        let salt = (9, 8);
        let older = (7, 6);
        let frames = vec![
            frame(2, 0, salt, 0x11),
            frame(3, 3, salt, 0x22),
            frame(2, 3, older, 0x33),
        ];
        let wal = build_wal(3, salt, &frames);
        let info = inspect(&wal).expect("inspect");
        assert!(info.header_checksum_ok);
        assert_eq!(info.frames, 3);
        assert_eq!(info.generations.len(), 2);
        assert!(info.generations[0].matches_header_salt);
        assert_eq!(info.generations[0].chain_verified_frames, 2);
        assert!(!info.generations[1].matches_header_salt);
        assert_eq!(info.generations[1].commit_frames, 1);

        // Stale-header variant: frame salts lag the header salts, so the
        // first frame of the generation is not chain-verifiable.
        let stale = build_wal(3, (10, 11), &frames[..2]);
        let info = inspect(&stale).expect("inspect");
        assert!(!info.generations[0].matches_header_salt);
        assert_eq!(
            info.generations[0].chain_verified_frames, 1,
            "first frame unverifiable, the rest chain off it"
        );
    }

    #[test]
    fn btree_page_type_check() {
        for byte in [0x02, 0x05, 0x0a, 0x0d] {
            assert!(is_btree_page_type(byte));
        }
        assert!(!is_btree_page_type(0x00));
        assert!(!is_btree_page_type(0x53)); // 'S' of the SQLite header
    }

    // ------------------------------------------------------------------
    // End-to-end: real SQLite database (4096-byte pages, 80 reserve bytes,
    // WAL journal mode) → hand-built encrypted db + encrypted WAL →
    // decrypt + merge → rusqlite row count and integrity check.
    // ------------------------------------------------------------------

    /// Build a pair of plaintext database images that share a schema:
    /// the base image holds `base_rows` rows, the extended image
    /// `extra_rows` more. Both carry 80 reserve bytes per page, matching
    /// the decrypted-WCDB layout that `merge` consumes.
    fn real_db_pair(base_rows: u32, extra_rows: u32) -> (Vec<u8>, Vec<u8>) {
        use rusqlite::ffi;
        let dir = std::env::temp_dir().join(format!(
            "wechat-walmerge-{}-{}",
            std::process::id(),
            base_rows
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("fixture.db");
        let (base, extended);
        {
            let conn = rusqlite::Connection::open(&path).expect("open fixture db");
            // 80 reserve bytes per page must be set before the first write.
            let mut reserve: std::ffi::c_int = 80;
            // SAFETY: `conn.handle()` is a live sqlite3*, the database name
            // is a valid NUL-terminated literal, and `reserve` is a valid
            // in/out int pointer for SQLITE_FCNTL_RESERVE_BYTES.
            let rc = unsafe {
                ffi::sqlite3_file_control(
                    conn.handle(),
                    c"main".as_ptr(),
                    ffi::SQLITE_FCNTL_RESERVE_BYTES,
                    &mut reserve as *mut std::ffi::c_int as *mut std::ffi::c_void,
                )
            };
            assert_eq!(rc, ffi::SQLITE_OK, "reserve-bytes file control");
            conn.execute_batch(
                "PRAGMA page_size=4096;
                 PRAGMA journal_mode=WAL;
                 CREATE TABLE m (id INTEGER PRIMARY KEY, body TEXT);",
            )
            .expect("fixture schema");
            {
                let mut stmt = conn
                    .prepare("INSERT INTO m (id, body) VALUES (?1, ?2)")
                    .expect("insert stmt");
                for id in 1..=base_rows {
                    stmt.execute(rusqlite::params![id, format!("row-{id}")])
                        .expect("insert row");
                }
            }
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .expect("base checkpoint");
            base = std::fs::read(&path).expect("read base image");
            {
                let mut stmt = conn
                    .prepare("INSERT INTO m (id, body) VALUES (?1, ?2)")
                    .expect("insert stmt");
                for id in base_rows + 1..=base_rows + extra_rows {
                    stmt.execute(rusqlite::params![id, format!("row-{id}")])
                        .expect("insert row");
                }
            }
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .expect("extended checkpoint");
            extended = std::fs::read(&path).expect("read extended image");
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(base[20], 80, "reserve byte recorded in the header");
        assert_eq!(&base[18..20], &[2, 2], "WAL journal versions");
        (base, extended)
    }

    /// SQLCipher-encrypt a whole decrypted-layout image, reversing
    /// `sqlcipher4::decrypt_database`; page 1's WAL/main-db salt-prefixed
    /// layout is exactly what `encrypt_wal_page` produces for pgno 1.
    fn encrypt_image(key: &[u8; 32], salt: &[u8], image: &[u8]) -> Vec<u8> {
        let pages = image.len() / sqlcipher4::PAGE_SZ;
        let mut out = Vec::with_capacity(image.len());
        for index in 0..pages {
            let at = index * sqlcipher4::PAGE_SZ;
            out.extend_from_slice(&encrypt_wal_page(
                key,
                salt,
                (index + 1) as u32,
                &image[at..at + sqlcipher4::PAGE_SZ],
            ));
        }
        out
    }

    #[test]
    fn encrypted_db_plus_encrypted_wal_merges_into_extended_rows() {
        let key = key();
        let (base, extended) = real_db_pair(4, 3);
        assert_eq!(base.len(), extended.len(), "small fixture grows in place");

        // The synthetic WAL carries exactly the pages that differ between
        // the checkpointed base image and the extended image; the last
        // frame commits with the extended image's page count.
        let salt = (0xDEAD_0001, 0xBEEF_0002);
        let pages = extended.len() / sqlcipher4::PAGE_SZ;
        let mut frames = Vec::new();
        for index in 0..pages {
            let at = index * sqlcipher4::PAGE_SZ;
            if base[at..at + sqlcipher4::PAGE_SZ] != extended[at..at + sqlcipher4::PAGE_SZ] {
                let pgno = (index + 1) as u32;
                frames.push(FrameSpec {
                    pgno,
                    db_size: 0,
                    salt,
                    page: encrypt_wal_page(
                        &key,
                        &SALT,
                        pgno,
                        &extended[at..at + sqlcipher4::PAGE_SZ],
                    ),
                });
            }
        }
        assert!(!frames.is_empty(), "fixture pages must differ");
        frames.last_mut().expect("last frame").db_size = pages as u32; // commit
        let wal = build_wal(1, salt, &frames);

        // Full chain: encrypted db → decrypt_database → merge → rusqlite.
        let encrypted = encrypt_image(&key, &SALT, &base);
        let decrypted = sqlcipher4::decrypt_database(&key, &encrypted).expect("decrypt db");
        // The usable bytes round-trip exactly; the per-page reserve tail
        // holds IV + HMAC by design, so compare usable regions only.
        for index in 0..base.len() / sqlcipher4::PAGE_SZ {
            let at = index * sqlcipher4::PAGE_SZ;
            assert_eq!(
                &decrypted[at..at + sqlcipher4::PAGE_SZ - 80],
                &base[at..at + sqlcipher4::PAGE_SZ - 80],
                "page {} usable bytes",
                index + 1
            );
        }
        let (merged, report) = merge(&key, &SALT, &decrypted, &wal).expect("merge");
        assert_eq!(report.frames_seen, frames.len());
        assert_eq!(report.frames_applied, frames.len());
        assert_eq!(report.frames_dropped_uncommitted, 0);
        assert_eq!(report.final_page_count, pages);
        for index in 0..pages {
            let at = index * sqlcipher4::PAGE_SZ;
            // merge() downgrades the journal version bytes (18/19) to make
            // the image standalone; mask them out of the comparison.
            let merged_page = &merged[at..at + sqlcipher4::PAGE_SZ - 80];
            let expected_page = &extended[at..at + sqlcipher4::PAGE_SZ - 80];
            if index == 0 {
                assert_eq!(&merged_page[..18], &expected_page[..18], "page 1 prefix");
                assert_eq!(&merged_page[20..], &expected_page[20..], "page 1 body");
            } else {
                assert_eq!(
                    merged_page,
                    expected_page,
                    "page {} usable bytes",
                    index + 1
                );
            }
        }

        let db = crate::WeChatDb::from_bytes(&merged).expect("merged image opens");
        assert_eq!(db.row_count("m").expect("row count"), 7);
        let integrity: String = db
            .conn()
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity_check");
        assert_eq!(integrity, "ok");
    }
}
