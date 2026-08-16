//! Offline WeChat tooling harness (developer/forensic-analyst use).
//!
//! Subcommands:
//!   scan <dump.dmp> <db...>              recover and print matching db keys
//!   decrypt <key-hex> <in.db> <out> [in.db-wal]
//!                                        decrypt one WCDB/SQLCipher-4 database,
//!                                        merging the WAL when given
//!   walinfo <in.db-wal>                  print WAL header/frame statistics
//!   parse <decrypted.db> <category> [owner-wxid]
//!                                        run a content parser, print stats
//!
//! `category` is one of `contact`, `session`, `message`, `sns`,
//! `favorite`. `owner-wxid` (the path wxid segment, `_<hash>` suffix
//! tolerated) enables message direction resolution.
//!
//! This binary ships nowhere near the host: it is a plugins-src developer
//! tool that drives the same library code the DLL uses.

use meow_plugin_wechat::payload::Payload;
use meow_plugin_wechat::{content, keyscan, sqlcipher4, walmerge, WeChatDb};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: wechat_offline scan <dump> <db...> | decrypt <key-hex> <in.db> <out.db> [in.db-wal] | walinfo <in.db-wal> | parse <decrypted.db> <category> [owner-wxid]"
        );
        std::process::exit(2);
    };
    let code = match command {
        "scan" => scan(&args[1..]),
        "decrypt" => decrypt(&args[1..]),
        "walinfo" => walinfo(&args[1..]),
        "parse" => parse(&args[1..]),
        _ => {
            eprintln!("unknown command: {command}");
            2
        }
    };
    std::process::exit(code);
}

fn parse_key(key_hex: &str) -> Option<[u8; 32]> {
    if key_hex.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (idx, pair) in key_hex.as_bytes().chunks(2).enumerate() {
        let byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("zz"), 16).ok()?;
        key[idx] = byte;
    }
    Some(key)
}

fn scan(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("usage: scan <dump.dmp> <db...>");
        return 2;
    }
    let dump = Path::new(&args[0]);
    let dbs: Vec<&Path> = args[1..].iter().map(|p| Path::new(p.as_str())).collect();
    let candidates = match keyscan::scan_dump_for_keys(dump) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("scan failed: {e}");
            return 1;
        }
    };
    println!("{} unique key candidates", candidates.len());
    let mut recovered = 0;
    for db in dbs {
        match keyscan::recover_key_for_db(&candidates, db) {
            Ok(Some(key)) => {
                recovered += 1;
                println!("{}: {}", db.display(), keyscan::key_to_hex(&key));
            }
            Ok(None) => println!("{}: no matching key", db.display()),
            Err(e) => println!("{}: read error {e}", db.display()),
        }
    }
    if recovered == 0 {
        1
    } else {
        0
    }
}

fn decrypt(args: &[String]) -> i32 {
    if args.len() != 3 && args.len() != 4 {
        eprintln!("usage: decrypt <key-hex> <in.db> <out.db> [in.db-wal]");
        return 2;
    }
    let Some(key) = parse_key(&args[0]) else {
        eprintln!("invalid key hex (expected 64 hex chars)");
        return 2;
    };
    let data = match std::fs::read(&args[1]) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read {}: {e}", args[1]);
            return 1;
        }
    };
    let plain = match sqlcipher4::decrypt_database(&key, &data) {
        Ok(plain) => plain,
        Err(e) => {
            eprintln!("decrypt {}: {e}", args[1]);
            return 1;
        }
    };
    // Optional WAL merge: the page-1 salt (first 16 bytes of the encrypted
    // main database) drives the frame HMAC verification.
    let (plain, report) = match args.get(3) {
        Some(wal_path) => {
            let wal = match std::fs::read(wal_path) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("read {wal_path}: {e}");
                    return 1;
                }
            };
            let salt = &data[..sqlcipher4::SALT_SZ];
            match walmerge::merge(&key, salt, &plain, &wal) {
                Ok((merged, report)) => (merged, Some(report)),
                Err(e) => {
                    eprintln!("wal merge {wal_path}: {e}");
                    return 1;
                }
            }
        }
        None => (plain, None),
    };
    if let Err(e) = std::fs::write(&args[2], &plain) {
        eprintln!("write {}: {e}", args[2]);
        return 1;
    }
    println!("{}: decrypted {} bytes", args[2], plain.len());
    if let Some(report) = report {
        println!(
            "wal merge: frames seen={} valid={} applied={} dropped_uncommitted={} pages_written={} final_pages={}",
            report.frames_seen,
            report.frames_valid,
            report.frames_applied,
            report.frames_dropped_uncommitted,
            report.pages_written,
            report.final_page_count
        );
    }
    0
}

fn walinfo(args: &[String]) -> i32 {
    if args.len() != 1 {
        eprintln!("usage: walinfo <in.db-wal>");
        return 2;
    }
    let wal = match std::fs::read(&args[0]) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("read {}: {e}", args[0]);
            return 1;
        }
    };
    if wal.is_empty() {
        println!("{}: empty WAL (0 bytes)", args[0]);
        return 0;
    }
    match walmerge::inspect(&wal) {
        Ok(info) => {
            println!(
                "page_size={} checkpoint_seq={} salt=({:08x},{:08x}) header_checksum_ok={}",
                info.page_size,
                info.checkpoint_seq,
                info.salt1,
                info.salt2,
                info.header_checksum_ok
            );
            println!(
                "frames={} trailing_bytes={} generations={}",
                info.frames,
                info.trailing_bytes,
                info.generations.len()
            );
            for (index, gen) in info.generations.iter().enumerate() {
                println!(
                    "  gen{index}: salt=({:08x},{:08x}) frames[{}..{}] n={} commits={} max_pgno={} current_header_salt={} chain_verified={}",
                    gen.salt1,
                    gen.salt2,
                    gen.first_frame,
                    gen.first_frame + gen.frame_count - 1,
                    gen.frame_count,
                    gen.commit_frames,
                    gen.max_pgno,
                    gen.matches_header_salt,
                    gen.chain_verified_frames
                );
            }
            0
        }
        Err(e) => {
            eprintln!("walinfo {}: {e}", args[0]);
            1
        }
    }
}

fn parse(args: &[String]) -> i32 {
    if args.len() < 2 || args.len() > 3 {
        eprintln!(
            "usage: parse <decrypted.db> <contact|session|message|sns|favorite> [owner-wxid]"
        );
        return 2;
    }
    let data = match std::fs::read(&args[0]) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read {}: {e}", args[0]);
            return 1;
        }
    };
    let db = match WeChatDb::from_bytes(&data) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("open {}: {e}", args[0]);
            return 1;
        }
    };
    let owner = args.get(2).map(String::as_str).unwrap_or("");
    let mut payload = Payload::empty();
    let result = match args[1].as_str() {
        "contact" => content::contacts::parse(&db, &mut payload),
        "session" => content::sessions::parse(&db, &mut payload),
        "message" => content::messages::parse(&db, owner, &mut payload),
        "sns" => content::sns::parse(&db, &mut payload),
        "favorite" => content::favorites::parse(&db, &mut payload),
        other => {
            eprintln!("unknown category: {other}");
            return 2;
        }
    };
    let emitted = match result {
        Ok(n) => n,
        Err(e) => {
            eprintln!("parse failed: {e}");
            return 1;
        }
    };
    let mut families: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for artifact in &payload.artifacts {
        *families.entry(artifact.family.as_str()).or_default() += 1;
    }
    println!("content artifacts emitted: {emitted}");
    for (family, count) in families {
        println!("  {family}: {count}");
    }
    println!("timeline events: {}", payload.timeline_events.len());
    for warning in &payload.warnings {
        println!("warning: {warning}");
    }
    if let Some(first) = payload.artifacts.first() {
        let json = serde_json::to_string_pretty(first).unwrap_or_default();
        println!("first artifact sample:\n{json}");
    }
    0
}
