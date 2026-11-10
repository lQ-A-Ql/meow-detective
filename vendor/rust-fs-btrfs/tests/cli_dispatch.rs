//! The multi-call binary: every name answers as itself, the
//! repository-named form reaches the same tool, `--version` identifies
//! the crate, errors are structured, the verbs this library cannot do
//! answer so by name, and `doctor` tells our program from whatever else
//! PATH finds under the same name.
//!
//! No fixture, no VM: the unit tier.

mod cli_support;

use cli_support::*;
use std::path::Path;

const CRATE: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[test]
fn the_binary_installs_exactly_its_dotted_names() {
    // Written here, not read from the binary: a binary that grew or lost a
    // name would otherwise agree with itself. fsck.btrfs checks a volume
    // as btrfs check --readonly does (#260).
    assert_eq!(dotted_names(), ["fs.btrfs", "mkfs.btrfs", "fsck.btrfs"]);
}

#[test]
fn every_name_answers_version_with_itself_the_crate_and_the_version() {
    let mut names = dotted_names();
    names.push("rust-fs-btrfs".to_string());
    for name in names {
        for flag in ["--version", "-V"] {
            let out = ok(tool(&name).arg(flag));
            assert_eq!(
                stdout(&out).trim_end(),
                format!("{name} ({CRATE}) {VERSION}"),
                "{name} {flag}"
            );
        }
    }
    // Through the repository name, the tool still names itself.
    for word in ["fs", "fs.btrfs"] {
        let out = ok(tool("rust-fs-btrfs").args([word, "--version"]));
        assert_eq!(
            stdout(&out).trim_end(),
            format!("fs.btrfs ({CRATE}) {VERSION}")
        );
    }
}

#[test]
fn the_repository_name_reaches_a_tool_by_verb_and_by_full_name() {
    let args = ["image.img", "resize", "1G"];
    let dotted = tool("fs.btrfs").args(args).output().unwrap();
    assert_eq!(dotted.status.code(), Some(3), "{}", stderr(&dotted));
    for word in ["fs", "fs.btrfs"] {
        let repo = tool("rust-fs-btrfs").arg(word).args(args).output().unwrap();
        assert_eq!(
            repo.status.code(),
            dotted.status.code(),
            "rust-fs-btrfs {word}"
        );
        assert_eq!(stdout(&repo), stdout(&dotted), "rust-fs-btrfs {word}");
        assert_eq!(stderr(&repo), stderr(&dotted), "rust-fs-btrfs {word}");
    }
    // cargo's own build, under cargo's name, is the same entry point.
    let cargo = entry().arg("fs").args(args).output().unwrap();
    assert_eq!(stderr(&cargo), stderr(&dotted));
}

#[test]
fn every_tool_help_carries_an_example_for_every_verb() {
    for name in dotted_names() {
        let out = ok(tool(&name).arg("--help"));
        assert!(
            stdout(&out).contains("Examples:"),
            "{name} --help has no example:\n{}",
            stdout(&out)
        );
        // fs.btrfs is a set of verbs; mkfs.btrfs is one command.
        if name != "fs.btrfs" {
            continue;
        }
        for verb in [
            "ls", "read", "write", "mkdir", "get", "info", "set", "resize",
        ] {
            let out = ok(tool(&name).args(["image.img", verb, "--help"]));
            assert!(
                stdout(&out).contains(&format!("Examples:\n  {name} ")),
                "{name} {verb} --help has no example:\n{}",
                stdout(&out)
            );
        }
    }
    let out = ok(tool("rust-fs-btrfs").arg("--help"));
    assert!(
        stdout(&out).contains("rust-fs-btrfs fs"),
        "rust-fs-btrfs --help does not show `rust-fs-btrfs fs`:\n{}",
        stdout(&out)
    );
}

#[test]
fn a_bare_entry_point_shows_its_help_and_says_nothing_was_done() {
    let out = tool("rust-fs-btrfs").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let all = stdout(&out) + &stderr(&out);
    assert!(all.contains("doctor"), "{all}");
    assert!(all.contains("fs"), "{all}");
}

#[test]
fn a_wrong_command_line_is_a_structured_error_on_stderr_with_status_2() {
    let message = refused(tool("fs.btrfs").args(["x.img", "--no-such-flag", "ls"]), 2);
    assert!(message.contains("--no-such-flag"), "{message}");
    let message = refused(tool("fs.btrfs").args(["x.img", "no-such-verb"]), 2);
    assert!(message.contains("no-such-verb"), "{message}");

    // --text: clap's own message, for a person.
    let out = tool("fs.btrfs")
        .args(["--text", "x.img", "--no-such-flag", "ls"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).starts_with("error: "), "{}", stderr(&out));
}

#[test]
fn the_verbs_this_library_cannot_do_answer_not_implemented_with_status_3() {
    // None of these opens the image: the answer does not depend on it.
    for (args, why) in [
        (vec!["mkdir", "/d"], "rust-fs-btrfs#262"),
        (vec!["resize", "1G"], "resize"),
    ] {
        let message = refused(tool("fs.btrfs").arg("never-opened.img").args(&args), 3);
        assert!(
            message.starts_with("not implemented: ") && message.contains(why),
            "{args:?}: {message}"
        );
    }
    // A key that is not settable is refused as read-only, still status 3;
    // one that does not exist is a wrong command line.
    let message = refused(
        tool("fs.btrfs").args(["never-opened.img", "set", "total_bytes", "1"]),
        3,
    );
    assert!(message.contains("read-only"), "{message}");
    refused(
        tool("fs.btrfs").args(["never-opened.img", "set", "colour", "blue"]),
        2,
    );
    // --text: `<tool>: <message>`, for a person.
    let out = tool("fs.btrfs")
        .args(["--text", "never-opened.img", "resize", "1G"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(
        stderr(&out).starts_with("fs.btrfs: not implemented: "),
        "{}",
        stderr(&out)
    );
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

/// A PATH made of `dirs`, and doctor's JSON and status against it.
fn doctor(dirs: &[&Path]) -> (Option<i32>, String) {
    let path = std::env::join_paths(dirs).unwrap();
    let out = entry().arg("doctor").env("PATH", path).output().unwrap();
    (out.status.code(), stdout(&out))
}

/// An executable script at `path` that prints `line` for `--version`.
fn impostor(path: &Path, line: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\necho '{line}'\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn doctor_passes_when_every_name_on_path_is_ours() {
    let (code, json) = doctor(&[names_dir()]);
    assert_eq!(code, Some(0), "{json}");
    assert!(json.contains("\"ok\": true"), "{json}");
    for name in dotted_names() {
        assert!(json.contains(&format!("\"name\": \"{name}\"")), "{json}");
    }
    assert!(json.contains("\"status\": \"ours\""), "{json}");
}

#[test]
fn doctor_names_a_shadowing_program_and_says_which_path_entry_to_move() {
    let theirs = scratch_dir("doctor-foreign");
    impostor(&theirs.join("fs.btrfs"), "fs.btrfs 1.0 (someone else)");
    let (code, json) = doctor(&[&theirs, names_dir()]);
    assert_eq!(code, Some(1), "{json}");
    assert!(json.contains("\"ok\": false"), "{json}");
    assert!(json.contains("\"status\": \"foreign\""), "{json}");
    assert!(
        json.contains(&format!(
            "\"path\": \"{}\"",
            theirs.join("fs.btrfs").display()
        )),
        "{json}"
    );
    assert!(
        json.contains(&format!(
            "put {} before {} on PATH",
            names_dir().display(),
            theirs.display()
        )),
        "{json}"
    );
    // Ours is still found, later, and listed as not run.
    assert!(
        json.contains(&names_dir().join("fs.btrfs").display().to_string()),
        "{json}"
    );
}

#[test]
fn doctor_names_the_homebrew_formula_to_unlink() {
    let prefix = scratch_dir("doctor-brew");
    let real = prefix.join("Cellar/some-formula/1.0/bin/fs.btrfs");
    impostor(&real, "fs.btrfs version 1.0");
    let bin_dir = prefix.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::os::unix::fs::symlink(&real, bin_dir.join("fs.btrfs")).unwrap();
    let (code, json) = doctor(&[&bin_dir, names_dir()]);
    assert_eq!(code, Some(1), "{json}");
    assert!(json.contains("\"formula\": \"some-formula\""), "{json}");
    assert!(json.contains("`brew unlink some-formula`"), "{json}");
}

#[test]
fn doctor_reports_a_missing_name_with_how_to_install_it() {
    let empty = scratch_dir("doctor-empty");
    let (code, json) = doctor(&[&empty]);
    assert_eq!(code, Some(1), "{json}");
    assert!(json.contains("\"status\": \"missing\""), "{json}");
    assert!(json.contains("chore cli:install"), "{json}");
    assert!(
        json.contains("brew install antimatter-studios/tap/rust-fs-btrfs"),
        "{json}"
    );
}

#[test]
fn doctor_reports_our_program_at_another_version_as_stale() {
    let old = scratch_dir("doctor-stale");
    impostor(&old.join("fs.btrfs"), &format!("fs.btrfs ({CRATE}) 0.0.1"));
    let (code, json) = doctor(&[&old, names_dir()]);
    assert_eq!(code, Some(1), "{json}");
    assert!(json.contains("\"status\": \"stale\""), "{json}");
    assert!(
        json.contains(&format!("{CRATE} 0.0.1, not {VERSION}")),
        "{json}"
    );
}

#[test]
fn doctor_text_is_for_a_person_and_keeps_the_fix() {
    let theirs = scratch_dir("doctor-text");
    impostor(&theirs.join("fs.btrfs"), "something else entirely");
    let path = std::env::join_paths([theirs.as_path(), names_dir()]).unwrap();
    let out = entry()
        .args(["doctor", "--text"])
        .env("PATH", path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("fs.btrfs: foreign ("), "{text}");
    assert!(text.contains("  fix: "), "{text}");
    assert!(!text.contains('{'), "{text}");
}
