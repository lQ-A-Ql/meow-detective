//! `fsck.btrfs`: check a Btrfs filesystem without changing it.
//!
//! What it checks is this crate's checker (`fs_btrfs::check`): the
//! superblock copies, every tree block of every tree, the extent tree
//! against the blocks and data the trees reach, block groups, chunks
//! against device extents, the free-space tree, every subvolume's inodes
//! and directories, and the checksum tree. It is a subset of what
//! `btrfs check --readonly` checks, and says so: a volume this calls clean
//! is one on which none of THESE invariants is broken.
//!
//! It never writes. `-n` is accepted because scripts pass it; `-y` and
//! `-p` (repair) are refused, because there is no repair.
//!
//! EXIT STATUS IS fsck(8)'s, because scripts and the `fsck` front-end read
//! it: 0 clean, 4 errors left uncorrected, 8 an operational error (the
//! target could not be opened, or is not Btrfs), 16 a wrong command line.

use std::ffi::OsString;
use std::sync::Arc;

use clap::{value_parser, Arg, ArgAction, ArgMatches, Command as Cmd};

use fs_core::cli::{CliError, Json, Outcome, Tool};
use fs_core::{BlockRead, FileDevice, OwnedSlice};

/// fsck(8): no errors.
pub const CLEAN: u8 = 0;
/// fsck(8): filesystem errors left uncorrected.
pub const UNCORRECTED: u8 = 4;
/// fsck(8): operational error.
pub const OPERATIONAL: u8 = 8;
/// fsck(8): usage or syntax error.
pub const USAGE: u8 = 16;

pub const TOOL: Tool = Tool {
    name: "fsck.btrfs",
    verb: "fsck",
    section: 8,
    usage_exit: USAGE,
    about: "Check a Btrfs filesystem without changing it",
    command,
    run,
};

fn command() -> Cmd {
    Cmd::new("fsck.btrfs")
        .about("Check a Btrfs filesystem without changing it")
        .long_about(
            "Check a Btrfs image or device and report what is wrong with it. Nothing is \
             written, and nothing is repaired.\n\n\
             Checked: the superblock copies against the primary; every tree block of every \
             tree (checksum, level, generation, key order, parent key); the extent tree against \
             the blocks and data the trees reach; block groups against their chunks and \
             extents; chunks against device extents; the free-space tree against the extent \
             tree; every subvolume's inodes, directory entries, back-references, link counts \
             and directory sizes; and the checksum tree against the data extents. A volume \
             whose log tree holds unreplayed changes is not checked: mount it with Linux once.\n\n\
             Exit status is fsck(8)'s: 0 clean, 4 errors found (and left), 8 the target could \
             not be checked, 16 a wrong command line.",
        )
        .arg(
            Arg::new("target")
                .value_name("TARGET")
                .help("Block device or image file to check")
                .required(true)
                .value_parser(value_parser!(OsString)),
        )
        .arg(
            Arg::new("offset")
                .long("offset")
                .value_name("BYTES")
                .help(
                    "Where the filesystem starts inside TARGET (a partition in a whole-disk image)",
                )
                .default_value("0")
                .value_parser(value_parser!(u64)),
        )
        .arg(
            Arg::new("no-change")
                .short('n')
                .help("Check only (the default, and the only mode)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("repair")
                .short('y')
                .help("Refused: this checker does not repair")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("preen")
                .short('p')
                .help("Refused: this checker does not repair")
                .action(ArgAction::SetTrue),
        )
        .args(fs_core::cli::format_args())
        .after_help(
            "Examples:\n  \
             fsck.btrfs disk.img                     the report, as JSON\n  \
             fsck.btrfs --text disk.img              the findings, one per line\n  \
             fsck.btrfs --offset 1048576 whole.img   a partition inside a disk image",
        )
}

fn run(matches: &ArgMatches) -> Result<Outcome, CliError> {
    if matches.get_flag("repair") || matches.get_flag("preen") {
        return Err(CliError::usage(
            "fsck.btrfs checks and does not repair: -y and -p are refused rather than \
             ignored, so a script that asked for a repair is not told one happened",
        )
        .with_code(USAGE));
    }
    let target = matches
        .get_one::<OsString>("target")
        .expect("clap requires the target");
    let name = target.to_string_lossy().into_owned();
    let offset = *matches.get_one::<u64>("offset").expect("defaulted");

    let dev: Arc<dyn BlockRead> = Arc::new(
        FileDevice::open(&*name)
            .map_err(|e| CliError::failed(format!("open {name}: {e}")).with_code(OPERATIONAL))?,
    );
    let dev: Arc<dyn BlockRead> = if offset == 0 {
        dev
    } else {
        let size = dev.size_bytes();
        if offset >= size {
            return Err(CliError::failed(format!(
                "--offset {offset} is past the end of {name} ({size} bytes)"
            ))
            .with_code(OPERATIONAL));
        }
        Arc::new(OwnedSlice::new(dev, offset, size - offset))
    };

    let base = |clean: bool, dirty: bool, code: u8| -> Vec<(&'static str, Json)> {
        vec![
            ("fs", Json::from("btrfs")),
            ("device", Json::from(name.as_str())),
            ("clean", Json::from(clean)),
            ("dirty", Json::from(dirty)),
            ("exit", Json::from(u64::from(code))),
        ]
    };

    let fs = match fs_btrfs::Filesystem::mount(dev) {
        Ok(fs) => fs,
        // Not Btrfs at all, a device that cannot be read, or a filesystem
        // this driver cannot open (another device of a pool missing, a
        // feature it does not read): there is nothing it can check, which
        // is not the same as finding damage.
        Err(
            e @ (fs_btrfs::Error::NotBtrfs { .. }
            | fs_btrfs::Error::Io(_)
            | fs_btrfs::Error::UnsupportedFeature(_)
            | fs_btrfs::Error::UnsupportedProfile(_)
            | fs_btrfs::Error::UnsupportedChecksum(_)),
        ) => return Err(CliError::failed(format!("{name}: {e}")).with_code(OPERATIONAL)),
        // A log tree to replay is not damage, and the trees it would
        // change cannot be judged before it is replayed.
        Err(fs_btrfs::Error::DirtyLog) => {
            return Err(CliError::failed(format!(
                "{name}: the log tree holds changes nothing has replayed, so the trees are not \
                 yet what they will be. Mount it with Linux once (which replays the log) and \
                 check it again"
            ))
            .with_code(OPERATIONAL))
        }
        // Btrfs, and too damaged to mount: that is a finding.
        Err(e) => {
            let what = format!("the filesystem cannot be mounted: {e}");
            let mut report = base(false, false, UNCORRECTED);
            report.push((
                "findings",
                Json::Arr(vec![Json::object([("what", Json::from(what.as_str()))])]),
            ));
            return Ok(Outcome::report(Json::object(report))
                .with_text(format!("{name}: {what}"))
                .with_code(UNCORRECTED));
        }
    };

    let checked = fs_btrfs::check::check(&fs);
    let code = if checked.is_clean() {
        CLEAN
    } else {
        UNCORRECTED
    };
    let mut report = base(checked.is_clean(), false, code);
    report.push(("tree_blocks", Json::from(checked.tree_blocks)));
    report.push(("inodes", Json::from(checked.inodes)));
    report.push(("subvolumes", Json::from(checked.subvolumes)));
    report.push((
        "findings",
        Json::Arr(
            checked
                .findings
                .iter()
                .map(|f| {
                    Json::object([
                        ("tree", f.tree.map(Json::from).unwrap_or(Json::Null)),
                        ("what", Json::from(f.what.as_str())),
                    ])
                })
                .collect(),
        ),
    ));
    let mut text: Vec<String> = checked
        .findings
        .iter()
        .map(|f| format!("{name}: {}", f.what))
        .collect();
    if checked.is_clean() {
        text.push(format!(
            "{name}: clean, {} tree blocks, {} inodes in {} subvolumes",
            checked.tree_blocks, checked.inodes, checked.subvolumes
        ));
    }
    Ok(Outcome::report(Json::object(report))
        .with_text(text.join("\n"))
        .with_code(code))
}
