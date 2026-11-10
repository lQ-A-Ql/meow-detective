//! `mkfs.btrfs`: create a single-device Btrfs filesystem.
//!
//! The options are the standard formatter's spelling for what this one can
//! choose: `-L`, `-n`, `--csum`, `-U`, `-f`, `-q`, plus `-s 4096`, `-m dup`,
//! `-d single` and `-K`, which name the defaults this formatter makes and
//! so are accepted. Any other sector size or profile is REFUSED by name
//! rather than ignored: a filesystem made with a profile other than the
//! one asked for is a surprise found only when a disk fails.
//!
//! The device or image must already exist at its size, unless `--size` is
//! given for an image file that does not exist yet. A device that already
//! holds a filesystem or a partition table is refused without `-f`.

use std::sync::Arc;

use clap::{Arg, ArgAction, ArgMatches, Command as Cmd};

use fs_btrfs::mkfs::{self, Options, MAX_LABEL_BYTES};
use fs_btrfs::superblock::ChecksumType;
use fs_core::cli::{CliError, Json, Outcome, Tool};
use fs_core::{BlockRead, FileDevice};

pub const TOOL: Tool = Tool {
    name: "mkfs.btrfs",
    verb: "mkfs",
    section: 8,
    usage_exit: fs_core::cli::output::EXIT_USAGE,
    about: "Create a Btrfs filesystem on a device or an image file",
    command,
    run,
};

fn command() -> Cmd {
    Cmd::new("mkfs.btrfs")
        .about("Create a Btrfs filesystem on a device or an image file")
        .long_about(
            "Create a single-device Btrfs filesystem on a device or a pre-sized image file.\n\n\
             The device or file must already exist at the target size (`truncate -s 1G \
             disk.img`), unless --size is given for an image file that does not exist yet. \
             A device that already holds a filesystem is refused unless -f is given.\n\n\
             The filesystem has the standard formatter's single-device defaults: metadata \
             and system DUP, data single, the free-space tree, skinny metadata and no-holes.\n\n\
             A JSON report of what was written, read back from the new superblock, goes to \
             stdout; progress goes to stderr.",
        )
        .arg(
            Arg::new("device")
                .value_name("TARGET")
                .help("Block device or image file to format")
                .required(true),
        )
        .arg(
            Arg::new("label")
                .short('L')
                .long("label")
                .value_name("LABEL")
                .help(format!("Volume label, at most {MAX_LABEL_BYTES} bytes")),
        )
        .arg(
            Arg::new("nodesize")
                .short('n')
                .long("nodesize")
                .value_name("BYTES")
                .help(format!(
                    "Tree block size: a power of two, {}..={}. Default: {}.",
                    mkfs::MIN_NODESIZE,
                    mkfs::MAX_NODESIZE,
                    mkfs::DEFAULT_NODESIZE
                )),
        )
        .arg(
            Arg::new("sectorsize")
                .short('s')
                .long("sectorsize")
                .value_name("BYTES")
                .help("Sector size. Only 4096 is made."),
        )
        .arg(
            Arg::new("csum")
                .long("csum")
                .visible_alias("checksum")
                .value_name("TYPE")
                .help("Checksum: crc32c (default), xxhash, sha256 or blake2"),
        )
        .arg(
            Arg::new("uuid")
                .short('U')
                .long("uuid")
                .value_name("UUID")
                .help("Filesystem UUID. Default: random."),
        )
        .arg(
            Arg::new("metadata")
                .short('m')
                .long("metadata")
                .value_name("PROFILE")
                .help("Metadata profile. Only dup, the default, is made."),
        )
        .arg(
            Arg::new("data")
                .short('d')
                .long("data")
                .value_name("PROFILE")
                .help("Data profile. Only single, the default, is made."),
        )
        .arg(
            Arg::new("nodiscard")
                .short('K')
                .long("nodiscard")
                .help("Accepted: this formatter never discards")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("force")
                .short('f')
                .long("force")
                .help("Format even if the device already holds a filesystem")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("No progress on stderr")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("size")
                .long("size")
                .value_name("SIZE")
                .help(
                    "Create TARGET as an image file of SIZE bytes first, if it does not exist \
                     (K/M/G/T suffixes, 1024-based)",
                )
                .value_parser(parse_size),
        )
        .args(fs_core::cli::format_args())
        .after_help(
            "Examples:\n  \
             mkfs.btrfs --size 1G -L BACKUP disk.img\n  \
             truncate -s 4G disk.img && mkfs.btrfs -n 32768 --csum xxhash disk.img\n  \
             mkfs.btrfs -f /dev/sdb1                replace whatever is there",
        )
}

fn options(matches: &ArgMatches) -> Result<Options, CliError> {
    let mut opts = Options::default();
    if let Some(v) = matches.get_one::<String>("nodesize") {
        opts.nodesize = parse_bytes(v)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| CliError::usage(format!("-n {v}: not a size")))?;
    }
    if let Some(v) = matches.get_one::<String>("sectorsize") {
        if parse_bytes(v) != Some(4096) {
            return Err(CliError::usage(format!(
                "-s {v}: this formatter makes 4096-byte sectors only. Refused rather than \
                 ignored, so the filesystem made is the one asked for"
            )));
        }
    }
    if let Some(v) = matches.get_one::<String>("csum") {
        opts.csum = match v.to_ascii_lowercase().as_str() {
            "crc32c" => ChecksumType::Crc32c,
            "xxhash" | "xxhash64" => ChecksumType::XxHash64,
            "sha256" => ChecksumType::Sha256,
            "blake2" | "blake2b" => ChecksumType::Blake2b256,
            other => {
                return Err(CliError::usage(format!(
                    "--csum {other}: not a checksum this formatter knows (crc32c, xxhash, \
                     sha256, blake2)"
                )))
            }
        };
    }
    for (id, flag, only) in [("metadata", 'm', "dup"), ("data", 'd', "single")] {
        if let Some(v) = matches.get_one::<String>(id) {
            if !v.eq_ignore_ascii_case(only) {
                return Err(CliError::usage(format!(
                    "-{flag} {v}: this formatter makes the {id} profile {only} only, on one \
                     device. Refused rather than ignored, so the filesystem made is the one \
                     asked for"
                )));
            }
        }
    }
    if let Some(v) = matches.get_one::<String>("uuid") {
        opts.uuid =
            Some(parse_uuid(v).ok_or_else(|| CliError::usage(format!("-U {v}: not a UUID")))?);
    }
    opts.label = matches.get_one::<String>("label").cloned();
    Ok(opts)
}

fn run(matches: &ArgMatches) -> Result<Outcome, CliError> {
    let opts = options(matches)?;
    let quiet = matches.get_flag("quiet");
    let say = |line: String| {
        if !quiet {
            eprintln!("mkfs.btrfs: {line}");
        }
    };
    let device = matches
        .get_one::<String>("device")
        .expect("clap requires the device")
        .as_str();

    if let Some(n) = matches.get_one::<u64>("size").copied() {
        if std::fs::metadata(device).is_err() {
            let f = std::fs::File::create(device)
                .map_err(|e| CliError::failed(format!("--size: create {device}: {e}")))?;
            f.set_len(n)
                .map_err(|e| CliError::failed(format!("--size: set_len({n}) on {device}: {e}")))?;
            say(format!("--size: created {device} ({n} bytes)"));
        }
    }

    let dev = FileDevice::open_rw(device)
        .map_err(|e| CliError::failed(format!("open {device} read-write: {e}")))?;
    let size = dev.size_bytes();
    let plan = mkfs::plan(size, &opts).map_err(|e| CliError::failed(format!("{device}: {e}")))?;

    if !matches.get_flag("force") {
        if let Some(what) = mkfs::existing_signature(&dev) {
            return Err(CliError::refused(format!(
                "{device} already holds {what}. Use -f to replace it"
            )));
        }
    }

    say(format!(
        "formatting {device} ({size} bytes, nodes of {} bytes, a {}-byte metadata chunk)",
        plan.nodesize(),
        plan.metadata_chunk_bytes()
    ));
    mkfs::write(&dev, &plan).map_err(|e| CliError::failed(format!("{device}: {e}")))?;

    // The report is what the superblock now SAYS, read back through the
    // ordinary mount, not what was asked for.
    let dev: Arc<dyn BlockRead> = Arc::new(dev);
    let fs = fs_btrfs::Filesystem::mount(dev)
        .map_err(|e| CliError::failed(format!("read back {device} after formatting: {e}")))?;
    let sb = fs.superblock();
    let report = vec![
        ("fs", Json::from("btrfs")),
        ("device", Json::from(device)),
        ("device_bytes", Json::from(size)),
        ("formatted", Json::from(true)),
        (
            "label",
            if sb.label.is_empty() {
                Json::Null
            } else {
                Json::from(sb.label.as_str())
            },
        ),
        ("uuid", Json::from(format_uuid(&sb.fsid))),
        ("total_bytes", Json::from(sb.total_bytes)),
        ("node_size", Json::from(sb.nodesize)),
        ("sector_size", Json::from(sb.sectorsize)),
    ];
    say(format!("{device} formatted"));
    Ok(Outcome::report(Json::object(report)).with_text(String::new()))
}

/// The UUID in its standard 8-4-4-4-12 form.
fn format_uuid(u: &[u8; 16]) -> String {
    let hex: String = u.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// 32 hex digits, dashes anywhere.
fn parse_uuid(v: &str) -> Option<[u8; 16]> {
    let hex: String = v.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// A byte count with an optional 1024-based K/M/G/T suffix.
fn parse_bytes(v: &str) -> Option<u64> {
    let (digits, shift) = match v.chars().last()?.to_ascii_uppercase() {
        'K' => (&v[..v.len() - 1], 10),
        'M' => (&v[..v.len() - 1], 20),
        'G' => (&v[..v.len() - 1], 30),
        'T' => (&v[..v.len() - 1], 40),
        _ => (v, 0),
    };
    digits.parse::<u64>().ok()?.checked_mul(1u64 << shift)
}

fn parse_size(v: &str) -> Result<u64, String> {
    parse_bytes(v)
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("not a size: {v}"))
}
