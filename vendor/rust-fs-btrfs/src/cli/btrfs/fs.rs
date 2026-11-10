//! `fs.btrfs <target> <verb>`: an errand inside a Btrfs image or device,
//! without mounting it.
//!
//! The verbs are the shared set: `ls`, `read`, `write`, `mkdir`,
//! `get`/`info`, `set`, `resize`. Metadata is JSON (or `--text`); file
//! content is raw bytes. A verb the library cannot do yet still exists and
//! answers `not implemented` with exit status 3, so a script moved between
//! filesystems fails loudly instead of meaning something else.
//!
//! What this library can write is narrow, and `write` says so rather than
//! pretending otherwise: the bytes of an existing NODATACOW file, in
//! place, at the same length. Everything that needs a copy-on-write
//! transaction -- a new file, a directory, a different length, an ordinary
//! file -- is refused, by the library's own reason where it has one.

use std::ffi::OsString;
use std::io::Write;

use clap::{value_parser, Arg, ArgAction, ArgMatches, Command as Cmd};

use super::device;
use fs_btrfs::inode::{FileType, Inode};
use fs_btrfs::superblock::{compat_ro, incompat};
use fs_btrfs::{ChecksumType, Error, Filesystem, Superblock};
use fs_core::cli::{CliError, Json, Outcome, Tool};

pub const TOOL: Tool = Tool {
    name: "fs.btrfs",
    verb: "fs",
    section: 1,
    usage_exit: fs_core::cli::output::EXIT_USAGE,
    about: "List, read and inspect a Btrfs image or device without mounting it",
    command,
    run,
};

/// The canonical keys every `fs.<fs>` answers, in the shared order.
/// Filesystem specifics are nested under `btrfs`.
pub const KEYS: &[&str] = &[
    "fs",
    "label",
    "total_bytes",
    "free_bytes",
    "block_size",
    "dirty",
    "btrfs",
];

/// Why creating, removing or resizing anything is refused: the library
/// changes the bytes of files that exist, and nothing else yet.
const COW_BLOCKED: &str = "not something this library does yet (rust-fs-btrfs#262)";

fn command() -> Cmd {
    Cmd::new("fs.btrfs")
        .about("List, read and inspect a Btrfs image or device without mounting it")
        .long_about(
            "Work inside a Btrfs image or device directly: no mount, no kernel driver.\n\n\
             An escape hatch for an errand (get a file out, read the label, check whether \
             it is dirty), not a place to do real filesystem work: for that, mount it.\n\n\
             Metadata is JSON on stdout (--text for people); `read` writes the file's raw \
             bytes. A failure is {\"error\": \"...\", \"code\": N} on stderr, N being the \
             exit status: 1 failed, 2 wrong command line, 3 not implemented or refused.\n\n\
             Writing is limited to what this library can do safely: `write` overwrites \
             an existing NODATACOW file in place, at the same length. Everything that \
             needs a copy-on-write transaction answers with exit status 3.",
        )
        .arg(
            Arg::new("target")
                .value_name("TARGET")
                .help("The image file or device")
                .value_parser(value_parser!(OsString))
                .required(true),
        )
        .arg(
            Arg::new("offset")
                .long("offset")
                .value_name("BYTES")
                .help(
                    "Where the filesystem starts in TARGET, for a partition in a whole-disk image",
                )
                .value_parser(value_parser!(u64))
                .global(true),
        )
        .args(fs_core::cli::format_args().map(|a| a.global(true)))
        .subcommand_required(true)
        .subcommand(
            Cmd::new("ls")
                .about(
                    "List a directory: name, type, size, mode, mtime (a symlink's target, \
                     and which entries are subvolumes)",
                )
                .arg(
                    Arg::new("path")
                        .value_name("PATH")
                        .default_value("/")
                        .value_parser(value_parser!(OsString)),
                )
                .after_help(
                    "Examples:\n  fs.btrfs disk.img ls /home\n  \
                     fs.btrfs disk.img ls / | jq -r '.[] | select(.subvolume) | .name'\n  \
                     fs.btrfs disk.img ls --text /",
                ),
        )
        .subcommand(
            Cmd::new("read")
                .about("Write a file's bytes to stdout, or to a file with -o")
                .arg(
                    Arg::new("path")
                        .value_name("PATH")
                        .required(true)
                        .value_parser(value_parser!(OsString)),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("FILE")
                        .value_parser(value_parser!(OsString))
                        .help("Write here instead of stdout"),
                )
                .after_help(
                    "Examples:\n  fs.btrfs disk.img read /etc/hostname\n  \
                     fs.btrfs disk.img read /snapshots/home/notes.txt | less\n  \
                     fs.btrfs disk.img read /backup.tar -o backup.tar",
                ),
        )
        .subcommand(
            Cmd::new("write")
                .about(
                    "Overwrite an existing NODATACOW file in place with the same number of \
                     bytes from stdin",
                )
                .arg(
                    Arg::new("path")
                        .value_name("PATH")
                        .required(true)
                        .value_parser(value_parser!(OsString)),
                )
                .after_help(
                    "Examples:\n  fs.btrfs disk.img write /vm/disk.raw < disk.raw\n  \
                     fs.btrfs disk.img read /db/data | fix-up | fs.btrfs disk.img write /db/data\n\n\
                     Only a file marked NODATACOW (chattr +C) can be written, and only with \
                     exactly as many bytes as it already holds: this library cannot allocate. \
                     A new file, an ordinary copy-on-write file, a snapshotted or compressed \
                     extent, or a different length is refused with exit status 3 and the \
                     reason.",
                ),
        )
        .subcommand(
            Cmd::new("mkdir")
                .about("Create a directory (not implemented: needs directory edits)")
                .arg(
                    Arg::new("path")
                        .value_name("PATH")
                        .required(true)
                        .value_parser(value_parser!(OsString)),
                )
                .after_help(
                    "Examples:\n  fs.btrfs disk.img mkdir /backup\n\n\
                     Answers `not implemented` (exit 3) until this library can add a \
                     directory entry (rust-fs-btrfs#262).",
                ),
        )
        .subcommand(key_command(
            "get",
            "Report the filesystem's properties, or one of them",
        ))
        .subcommand(key_command(
            "info",
            "The same as get: every property, or one of them",
        ))
        .subcommand(
            Cmd::new("set")
                .about("Change a property: the label")
                .arg(Arg::new("key").value_name("KEY").required(true))
                .arg(Arg::new("value").value_name("VALUE").required(true))
                .after_help(
                    "Examples:\n  fs.btrfs disk.img set label BACKUP\n\n\
                     The label is written to every superblock copy; at most 255 bytes.",
                ),
        )
        .subcommand(
            Cmd::new("resize")
                .about("Grow or shrink the filesystem (not implemented)")
                .arg(Arg::new("size").value_name("SIZE").required(true))
                .arg(
                    Arg::new("force")
                        .long("force")
                        .action(ArgAction::SetTrue)
                        .help("Do it without asking"),
                )
                .after_help(
                    "Examples:\n  fs.btrfs disk.img resize 20G --force\n\n\
                     Answers `not implemented` (exit 3): this library has no resize.",
                ),
        )
        .after_help(
            "Examples:\n  fs.btrfs disk.img ls /\n  \
             fs.btrfs disk.img read /etc/fstab > fstab\n  \
             fs.btrfs disk.img get label --text\n  \
             fs.btrfs --offset 1048576 whole-disk.img info",
        )
}

fn key_command(name: &'static str, about: &'static str) -> Cmd {
    Cmd::new(name)
        .about(about)
        .arg(
            Arg::new("key")
                .value_name("KEY")
                .help(format!("One of: {} (or btrfs.<field>)", KEYS.join(", "))),
        )
        .after_help(format!(
            "Examples:\n  fs.btrfs disk.img {name}\n  \
             fs.btrfs disk.img {name} label --text\n  \
             fs.btrfs disk.img {name} btrfs.csum_type"
        ))
}

fn run(matches: &ArgMatches) -> Result<Outcome, CliError> {
    let target = matches
        .get_one::<OsString>("target")
        .expect("clap requires the target");
    let (verb, sub) = matches.subcommand().expect("clap requires a verb");
    let offset = sub
        .get_one::<u64>("offset")
        .or_else(|| matches.get_one::<u64>("offset"))
        .copied()
        .unwrap_or(0);
    match verb {
        "ls" => ls(&device::mount(target, offset)?, path_arg(sub)),
        "read" => read(
            &device::mount(target, offset)?,
            path_arg(sub),
            sub.get_one("output"),
        ),
        "write" => write(target, offset, path_arg(sub)),
        "mkdir" => Err(CliError::not_implemented(format!("mkdir: {COW_BLOCKED}"))),
        "get" | "info" => get(
            target,
            offset,
            sub.get_one::<String>("key").map(String::as_str),
        ),
        "set" => set(target, offset, sub),
        "resize" => Err(CliError::not_implemented(
            "resize: this library cannot resize a Btrfs filesystem",
        )),
        other => unreachable!("clap knows no verb {other}"),
    }
}

fn path_arg(sub: &ArgMatches) -> &[u8] {
    let path = sub
        .get_one::<OsString>("path")
        .expect("clap requires or defaults the path");
    os_bytes(path)
}

#[cfg(unix)]
fn os_bytes(s: &OsString) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    s.as_bytes()
}

#[cfg(not(unix))]
fn os_bytes(s: &OsString) -> &[u8] {
    s.to_str().map(str::as_bytes).unwrap_or_default()
}

/// A byte string for a person to read. Never fed back into a lookup:
/// two distinct names can show the same.
fn show(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A library error about `what`: exit 3 when the library refused (it
/// cannot do this, or will not), exit 1 when something failed.
fn btrfs_error(what: &[u8], e: Error) -> CliError {
    let message = format!("{}: {e}", show(what));
    match e {
        Error::UnsupportedFeature(_) | Error::ReadOnly => CliError::refused(message),
        _ => CliError::failed(message),
    }
}

fn type_name(t: Option<FileType>) -> &'static str {
    match t {
        Some(FileType::Regular) => "file",
        Some(FileType::Directory) => "dir",
        Some(FileType::Symlink) => "symlink",
        Some(FileType::CharDevice) => "char",
        Some(FileType::BlockDevice) => "block",
        Some(FileType::Fifo) => "fifo",
        Some(FileType::Socket) => "socket",
        None => "unknown",
    }
}

fn type_char(name: &str) -> char {
    match name {
        "file" => '-',
        "dir" => 'd',
        "symlink" => 'l',
        "char" => 'c',
        "block" => 'b',
        "fifo" => 'p',
        "socket" => 's',
        _ => '?',
    }
}

/// `path` joined with one more component.
fn child_path(path: &[u8], name: &[u8]) -> Vec<u8> {
    let mut full = path.to_vec();
    if !full.ends_with(b"/") {
        full.push(b'/');
    }
    full.extend_from_slice(name);
    full
}

/// One `ls` entry: the fields every `fs.<fs>` reports, typed the same way
/// everywhere -- name (string), type (string), size (number), mode (octal
/// string), mtime (seconds since the epoch, number), inode (number) and
/// target (string) for a symlink -- plus `subvolume` (boolean), which is
/// Btrfs's own: the entry is the top directory of another subvolume or a
/// snapshot. A name that is not UTF-8 is shown lossily, with its exact
/// bytes in `name_hex`.
fn entry(fs: &Filesystem, name: &[u8], inode: &Inode, subvolume: bool) -> Json {
    let kind = type_name(inode.file_type());
    let mut fields = vec![("name", Json::from(show(name)))];
    if std::str::from_utf8(name).is_err() {
        fields.push((
            "name_hex",
            Json::from(name.iter().map(|b| format!("{b:02x}")).collect::<String>()),
        ));
    }
    fields.extend([
        ("type", Json::from(kind)),
        ("size", Json::from(inode.size)),
        ("mode", Json::from(format!("{:04o}", inode.permissions()))),
        ("mtime", Json::from(inode.mtime.sec)),
        ("inode", Json::from(inode.ino)),
        ("subvolume", Json::from(subvolume)),
    ]);
    if inode.is_symlink() {
        fields.push((
            "target",
            Json::from(fs.read_link(inode.ino).map(|t| show(&t)).ok()),
        ));
    }
    Json::object(fields)
}

fn entry_text(e: &Json) -> String {
    let field = |k: &str| e.get(k).map(Json::to_text).unwrap_or_default();
    let mut line = format!(
        "{}{} {:>12} {}",
        type_char(&field("type")),
        field("mode"),
        field("size"),
        field("name")
    );
    if e.get("subvolume") == Some(&Json::Bool(true)) {
        line.push_str(" (subvolume)");
    }
    if let Some(target) = e.get("target") {
        line.push_str(&format!(" -> {}", target.to_text()));
    }
    line
}

/// List `path`. A path crossing into a subvolume or a snapshot is
/// followed into it, as a mount would show it.
fn ls(root: &Filesystem, path: &[u8]) -> Result<Outcome, CliError> {
    let found = root
        .resolve_path_bytes(path)
        .map_err(|e| btrfs_error(path, e))?;
    let fs = found.fs(root);
    let entries = if found.inode.is_dir() {
        let mut listed = Vec::new();
        for d in fs
            .read_dir(found.inode.ino)
            .map_err(|e| btrfs_error(path, e))?
        {
            let full = child_path(path, &d.name);
            if d.is_inode() {
                let child = fs.read_inode(d.ino).map_err(|e| btrfs_error(&full, e))?;
                listed.push(entry(fs, &d.name, &child, false));
            } else {
                // A subvolume's entry names a tree, not an inode; its top
                // directory is what a mount shows at this name.
                let inner = root
                    .resolve_path_bytes(&full)
                    .map_err(|e| btrfs_error(&full, e))?;
                listed.push(entry(inner.fs(root), &d.name, &inner.inode, true));
            }
        }
        listed.sort_by(|a, b| {
            a.get("name")
                .map(Json::to_text)
                .cmp(&b.get("name").map(Json::to_text))
        });
        listed
    } else {
        let name = path.rsplit(|b| *b == b'/').next().unwrap_or(path);
        vec![entry(fs, name, &found.inode, false)]
    };
    let text = entries
        .iter()
        .map(entry_text)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Outcome::report(Json::Arr(entries)).with_text(text))
}

/// Stream a regular file's bytes. Each chunk is read before it is
/// written, so a file whose blocks turn out unreadable part-way stops with
/// status 1 and what came before stays on stdout; everything that can be
/// refused up front (no such path, a directory, a symlink) is refused
/// before a byte is written. `-o FILE` writes `FILE.partial` and renames
/// it, so FILE is never left half written.
fn read(root: &Filesystem, path: &[u8], output: Option<&OsString>) -> Result<Outcome, CliError> {
    let found = root
        .resolve_path_bytes(path)
        .map_err(|e| btrfs_error(path, e))?;
    let fs = found.fs(root);
    let inode = &found.inode;
    if inode.is_dir() {
        return Err(CliError::failed(format!("{}: is a directory", show(path))));
    }
    if inode.is_symlink() {
        let target = fs
            .read_link(inode.ino)
            .map(|t| show(&t))
            .unwrap_or_default();
        return Err(CliError::failed(format!(
            "{}: is a symlink to {target}; read the target instead",
            show(path)
        )));
    }
    if !inode.is_regular_file() {
        return Err(CliError::failed(format!(
            "{}: not a regular file",
            show(path)
        )));
    }
    const CHUNK: usize = 1 << 20;
    let mut buf = vec![0u8; CHUNK];
    let mut copy = |sink: &mut dyn Write| -> Result<(), CliError> {
        let mut offset = 0u64;
        while offset < inode.size {
            let want = CHUNK.min((inode.size - offset) as usize);
            let got = fs
                .read_at(inode.ino, offset, &mut buf[..want])
                .map_err(|e| btrfs_error(path, e))?;
            if got == 0 {
                return Err(CliError::failed(format!(
                    "{}: short read at byte {offset} of {}",
                    show(path),
                    inode.size
                )));
            }
            sink.write_all(&buf[..got])
                .map_err(|e| CliError::failed(format!("write: {e}")))?;
            offset += got as u64;
        }
        sink.flush()
            .map_err(|e| CliError::failed(format!("write: {e}")))
    };
    match output {
        None => copy(&mut std::io::stdout().lock())?,
        Some(file) => {
            let dest = std::path::Path::new(file);
            let mut partial = dest.as_os_str().to_owned();
            partial.push(".partial");
            let partial = std::path::PathBuf::from(partial);
            let mut f = std::fs::File::create(&partial)
                .map_err(|e| CliError::failed(format!("create {}: {e}", partial.display())))?;
            if let Err(e) = copy(&mut f) {
                drop(f);
                let _ = std::fs::remove_file(&partial);
                return Err(e);
            }
            std::fs::rename(&partial, dest)
                .map_err(|e| CliError::failed(format!("rename to {}: {e}", dest.display())))?;
        }
    }
    Ok(Outcome::done())
}

/// A UUID in its standard 8-4-4-4-12 form.
fn uuid_text(u: &[u8; 16]) -> String {
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

/// The checksum algorithm, by the name btrfs-progs gives it.
pub fn csum_name(t: ChecksumType) -> &'static str {
    match t {
        ChecksumType::Crc32c => "crc32c",
        ChecksumType::XxHash64 => "xxhash64",
        ChecksumType::Sha256 => "sha256",
        ChecksumType::Blake2b256 => "blake2",
    }
}

/// The names of the bits set in `bits`, lowercase, as btrfs-progs spells
/// them; a bit this table does not know is given in hex.
fn flag_names(bits: u64, table: &[(u64, &str)]) -> Json {
    let mut names = Vec::new();
    let mut known = 0u64;
    for &(bit, name) in table {
        known |= bit;
        if bits & bit != 0 {
            names.push(Json::from(name));
        }
    }
    let unknown = bits & !known;
    if unknown != 0 {
        names.push(Json::from(format!("{unknown:#x}")));
    }
    Json::Arr(names)
}

const INCOMPAT: &[(u64, &str)] = &[
    (incompat::MIXED_BACKREF, "mixed_backref"),
    (incompat::DEFAULT_SUBVOL, "default_subvol"),
    (incompat::MIXED_GROUPS, "mixed_groups"),
    (incompat::COMPRESS_LZO, "compress_lzo"),
    (incompat::COMPRESS_ZSTD, "compress_zstd"),
    (incompat::BIG_METADATA, "big_metadata"),
    (incompat::EXTENDED_IREF, "extended_iref"),
    (incompat::RAID56, "raid56"),
    (incompat::SKINNY_METADATA, "skinny_metadata"),
    (incompat::NO_HOLES, "no_holes"),
    (incompat::METADATA_UUID, "metadata_uuid"),
    (incompat::RAID1C34, "raid1c34"),
    (incompat::ZONED, "zoned"),
    (incompat::EXTENT_TREE_V2, "extent_tree_v2"),
    (incompat::RAID_STRIPE_TREE, "raid_stripe_tree"),
    (incompat::SIMPLE_QUOTA, "simple_quota"),
    (incompat::REMAP_TREE, "remap_tree"),
];

const COMPAT_RO: &[(u64, &str)] = &[
    (compat_ro::FREE_SPACE_TREE, "free_space_tree"),
    (compat_ro::FREE_SPACE_TREE_VALID, "free_space_tree_valid"),
    (compat_ro::VERITY, "verity"),
    (compat_ro::BLOCK_GROUP_TREE, "block_group_tree"),
];

/// Whether the volume needs attention before it can be trusted: a log
/// tree waiting to be replayed, or the error flag the kernel sets when it
/// forced the volume read-only.
pub fn is_dirty(sb: &Superblock) -> bool {
    sb.has_dirty_log() || sb.has_error_flag()
}

/// The envelope: the shared keys first, Btrfs's own under `btrfs`.
///
/// From the superblock alone, so a volume that will not mount -- a log
/// waiting for replay -- still answers, and says it is dirty.
pub fn envelope(sb: &Superblock) -> Json {
    Json::object([
        ("fs", Json::from("btrfs")),
        (
            "label",
            if sb.label.is_empty() {
                Json::Null
            } else {
                Json::from(sb.label.as_str())
            },
        ),
        ("total_bytes", Json::from(sb.total_bytes)),
        (
            "free_bytes",
            Json::from(sb.total_bytes.saturating_sub(sb.bytes_used)),
        ),
        ("block_size", Json::from(sb.sectorsize)),
        ("dirty", Json::from(is_dirty(sb))),
        (
            "btrfs",
            Json::object([
                ("fsid", Json::from(uuid_text(&sb.fsid))),
                ("metadata_uuid", Json::from(uuid_text(&sb.node_uuid()))),
                ("node_size", Json::from(sb.nodesize)),
                ("sector_size", Json::from(sb.sectorsize)),
                ("csum_type", Json::from(csum_name(sb.csum_type))),
                ("device_count", Json::from(sb.num_devices)),
                ("bytes_used", Json::from(sb.bytes_used)),
                ("generation", Json::from(sb.generation)),
                ("log_root", Json::from(sb.log_root)),
                (
                    "features",
                    Json::object([
                        ("compat", Json::from(format!("{:#x}", sb.compat_flags))),
                        ("compat_ro", flag_names(sb.compat_ro_flags, COMPAT_RO)),
                        ("incompat", flag_names(sb.incompat_flags, INCOMPAT)),
                    ]),
                ),
            ]),
        ),
    ])
}

fn get(target: &OsString, offset: u64, key: Option<&str>) -> Result<Outcome, CliError> {
    let sb = device::superblock(target, offset)?;
    let all = envelope(&sb);
    let Some(key) = key else {
        return Ok(Outcome::report(all));
    };
    let mut value = Some(&all);
    for part in key.split('.') {
        value = value.and_then(|v| v.get(part));
    }
    let Some(value) = value else {
        return Err(CliError::usage(format!(
            "no key {key:?}; the keys are {} (and btrfs.<field>)",
            KEYS.join(", ")
        )));
    };
    let text = value.to_text();
    Ok(Outcome::report(Json::object([(key, value.clone())])).with_text(text))
}

fn set(target: &OsString, offset: u64, sub: &ArgMatches) -> Result<Outcome, CliError> {
    let key = sub.get_one::<String>("key").expect("clap requires the key");
    match key.as_str() {
        "label" => {
            let value = sub
                .get_one::<String>("value")
                .expect("clap requires the value");
            if value.len() > fs_btrfs::super_write::MAX_LABEL_BYTES {
                return Err(CliError::failed(format!(
                    "set label: {} bytes, and a Btrfs label holds at most {}",
                    value.len(),
                    fs_btrfs::super_write::MAX_LABEL_BYTES
                )));
            }
            let dev = device::open_rw(target, offset)?;
            fs_btrfs::super_write::set_label(&*dev, value)
                .map_err(|e| btrfs_error(target.to_string_lossy().as_bytes(), e))?;
            Ok(
                Outcome::report(Json::object([("label", Json::from(value.as_str()))]))
                    .with_text(format!("label set to {value:?}")),
            )
        }
        k if KEYS.contains(&k) || k.starts_with("btrfs.") => {
            Err(CliError::refused(format!("{k} is read-only")))
        }
        other => Err(CliError::usage(format!(
            "no key {other:?}; the settable key is label"
        ))),
    }
}

/// Overwrite an existing NODATACOW file with everything on stdin, which
/// must be exactly as long as the file. The whole input is read before the
/// image is opened, so a failing producer (`false | fs.btrfs img write /f`)
/// leaves the image as it was; the library writes the whole range or none
/// of it.
fn write(target: &OsString, offset: u64, path: &[u8]) -> Result<Outcome, CliError> {
    let mut data = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin().lock(), &mut data)
        .map_err(|e| CliError::failed(format!("read stdin: {e}")))?;
    let dev = device::open_rw(target, offset)?;
    let fs = Filesystem::mount_rw(dev).map_err(|e| {
        btrfs_error(
            format!("{} (read-write)", target.to_string_lossy()).as_bytes(),
            e,
        )
    })?;
    let found = match fs.resolve_path_bytes(path) {
        Ok(found) => found,
        Err(Error::NotFound) => {
            return Err(CliError::not_implemented(format!(
                "write {}: creating a file is {COW_BLOCKED}",
                show(path)
            )))
        }
        Err(e) => return Err(btrfs_error(path, e)),
    };
    if found.tree.is_some() {
        return Err(CliError::refused(format!(
            "{}: is inside a subvolume or snapshot, which this library opens read-only",
            show(path)
        )));
    }
    let inode = &found.inode;
    if inode.is_dir() {
        return Err(CliError::failed(format!("{}: is a directory", show(path))));
    }
    if !inode.is_regular_file() {
        return Err(CliError::failed(format!(
            "{}: not a regular file",
            show(path)
        )));
    }
    if (data.len() as u64) < inode.size {
        return Err(CliError::refused(format!(
            "{}: {} bytes on stdin for a {}-byte file; making it shorter is {COW_BLOCKED}",
            show(path),
            data.len(),
            inode.size
        )));
    }
    let written = fs
        .write_at(inode.ino, 0, &data)
        .map_err(|e| btrfs_error(path, e))?;
    let report = Json::object([
        ("path", Json::from(show(path))),
        ("bytes", Json::from(written as u64)),
        ("created", Json::from(false)),
    ]);
    let text = format!("overwrote {} ({written} bytes, in place)", show(path));
    Ok(Outcome::report(report).with_text(text))
}
