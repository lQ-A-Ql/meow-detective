//! Offline WeChat tooling harness (developer/forensic-analyst use).
//!
//! Subcommands:
//!   scan <dump.dmp> <db...>              recover and print matching db keys
//!   decrypt <key-hex> <in.db> <out>      decrypt one WCDB/SQLCipher-4 database
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
use meow_plugin_wechat::{content, keyscan, sqlcipher4, WeChatDb};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: wechat_offline scan <dump> <db...> | decrypt <key-hex> <in.db> <out.db> | parse <decrypted.db> <category> [owner-wxid]"
        );
        std::process::exit(2);
    };
    let code = match command {
        "scan" => scan(&args[1..]),
        "decrypt" => decrypt(&args[1..]),
        "parse" => parse(&args[1..]),
        _ => {
            eprintln!("unknown command: {command}");
            2
        }
    };
    std::process::exit(code);
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
    if args.len() != 3 {
        eprintln!("usage: decrypt <key-hex> <in.db> <out.db>");
        return 2;
    }
    let key_hex = &args[0];
    let mut key = [0u8; 32];
    for (idx, pair) in key_hex.as_bytes().chunks(2).enumerate() {
        let Ok(byte) = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("zz"), 16) else {
            eprintln!("invalid key hex");
            return 2;
        };
        if idx >= 32 {
            eprintln!("key too long");
            return 2;
        }
        key[idx] = byte;
    }
    let data = match std::fs::read(&args[1]) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read {}: {e}", args[1]);
            return 1;
        }
    };
    match sqlcipher4::decrypt_database(&key, &data) {
        Ok(plain) => {
            if let Err(e) = std::fs::write(&args[2], &plain) {
                eprintln!("write {}: {e}", args[2]);
                return 1;
            }
            println!("{}: decrypted {} bytes", args[2], plain.len());
            0
        }
        Err(e) => {
            eprintln!("decrypt {}: {e}", args[1]);
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
