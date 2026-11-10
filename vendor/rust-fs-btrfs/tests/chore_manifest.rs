//! `chore_min_version` must cover the keys `chores.yml` actually uses.
//!
//! # The failure this exists to stop
//!
//! `chore_min_version` is enforced as a floor on the running binary,
//! and chore's YAML decoder uses `KnownFields(true)` -- an unknown key
//! is a hard parse error, not a field it ignores. So a declared floor
//! that is LOWER than the keys in the file produces the worst of both:
//! a binary between the declared floor and the real one passes the
//! version check, then fails to load the file at all with a generic
//! `unknown field "timeout" in a Task`, which never mentions
//! `chore_min_version` and sends the reader looking at their YAML
//! instead of at their chore build.
//!
//! `timeout:` and `on_timeout:` were adopted here while the floor still
//! said `0.6.0`, and the identical drift happened in a sibling
//! repository in the same week. It is not the kind of mistake that gets
//! noticed by the person who makes it, because their own chore is new
//! enough.
//!
//! # This parses the file rather than scanning it
//!
//! The key check used to be `line.starts_with("timeout:")` over the
//! file's text, and that is the shape it was returned for. Two
//! ordinary spellings defeat it SILENTLY -- the guard stays green
//! while the floor is wrong:
//!
//! ```text
//! floor 0.9.0, keys left plain            EXIT=101   <- the control
//! floor 0.9.0, "timeout": / "on_timeout": EXIT=0     <- DEFEAT
//! floor 0.9.0, net: { timeout: 45m }      EXIT=0     <- DEFEAT
//! ```
//!
//! A quoted key is the same key; a flow mapping puts it on a line that
//! starts with something else. Parsed, neither is an edge case: the
//! quoting is resolved before this file sees anything, and a mapping
//! is a mapping wherever it is written.
//!
//! THE FLOOR LOOKUP WAS NOT THE SAME RISK, and lumping the two together
//! would misdescribe both. It was the same raw-scan shape, but a quoted
//! floor is simply not FOUND, `violations` then reports "declares no
//! chore_min_version at all", and the test goes red. It failed closed.
//! It is parsed here too, because the document is already parsed and
//! two techniques in one file is how the next defect arrives -- not
//! because it was dangerous.
//!
//! # What this guard does NOT do
//!
//! It is not a model of chore's schema and cannot be: nothing available
//! here knows which release introduced a given key. It is a table of
//! the keys THIS repository uses whose introducing version is known,
//! and a row is added when a newer key is adopted. So it catches the
//! mistake that has already been made twice rather than every mistake
//! of the shape. A guard that under-claims and says so is worth more
//! than one that implies coverage it has not got.

use saphyr::{LoadableYamlNode, Yaml};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Keys whose presence requires a chore at least this new.
///
/// Names, not line prefixes. The parser hands back the key itself, so
/// there is no colon to remember and no quoting to strip.
const KEYS_REQUIRING: &[(&str, Version)] = &[
    // chore ab59c04, "Add timeout:/on_timeout:, the net for a task
    // that hangs", released in 0.10.0.
    ("timeout", (0, 10, 0)),
    ("on_timeout", (0, 10, 0)),
];

type Version = (u64, u64, u64);

fn chores_yml() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("chores.yml")
}

/// The document, or a loud failure.
///
/// A file this cannot parse is a FAILURE and never a pass: chore would
/// not load it either, so "I could not read it" is a real answer about
/// the manifest rather than a reason to stay quiet.
fn document(text: &str) -> Yaml<'_> {
    let documents = Yaml::load_from_str(text).unwrap_or_else(|e| {
        panic!(
            "chores.yml is not valid YAML: {e}. This guard reads the manifest rather \
             than scanning its text, so a file it cannot parse is a failure."
        )
    });
    documents
        .into_iter()
        .next()
        .expect("chores.yml must contain a document")
}

/// Maps whose keys the user names, not chore.
///
/// A `vars:` or `env:` entry called `timeout` is a variable the task's
/// own commands read, and says nothing about chore's `timeout:` field.
/// The key `vars` itself is still collected; only the names inside it
/// are not.
///
/// Only where chore puts such a map: at the top level or in a task. Under
/// `tasks:` the keys are task names, which the user also picks, so a task
/// NAMED `vars` or `env` is a task, and its body is walked like any other.
const FREE_FORM_MAPS: &[&str] = &["vars", "env"];

/// Every mapping key in the document, except the names inside a
/// [`FREE_FORM_MAPS`] map.
///
/// Recursive, because the keys this asks about are nested inside tasks
/// rather than at the top level, and a `timeout:` is the same adoption
/// wherever it appears. Sequences are walked too: a task's `cmds:` is a
/// list of mappings.
fn all_keys(node: &Yaml, out: &mut BTreeSet<String>) {
    collect_keys(node, None, out);
}

/// [`all_keys`], knowing the key whose value `node` is.
fn collect_keys(node: &Yaml, parent: Option<&str>, out: &mut BTreeSet<String>) {
    if let Some(mapping) = node.as_mapping() {
        for (key, value) in mapping.iter() {
            let name = key.as_str();
            if let Some(name) = name {
                out.insert(name.to_string());
                if FREE_FORM_MAPS.contains(&name) && parent != Some("tasks") {
                    continue;
                }
            }
            collect_keys(value, name, out);
        }
    } else if let Some(sequence) = node.as_sequence() {
        for item in sequence {
            collect_keys(item, parent, out);
        }
    }
}

fn parse_version(raw: &str) -> Option<Version> {
    let v = raw.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    let mut parts = v.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// The declared floor, or `None` if there is not one this can read.
///
/// `None` is the safe direction and is reported as a violation: a
/// manifest whose floor cannot be read is a manifest with no effective
/// floor. That is also why the previous raw-scan version of this was
/// not the dangerous half -- a quoted floor was simply not found, and
/// not-found is red.
fn declared_floor(doc: &Yaml) -> Option<Version> {
    doc.as_mapping()?
        .iter()
        .find(|(key, _)| key.as_str() == Some("chore_min_version"))
        .and_then(|(_, value)| value.as_str())
        .and_then(parse_version)
}

/// Every key present in `text` that the declared floor does not cover,
/// as a sentence naming the key, what it needs and what is declared.
fn violations(text: &str) -> Vec<String> {
    let doc = document(text);
    let Some(declared) = declared_floor(&doc) else {
        return vec!["chores.yml declares no readable chore_min_version at all".to_string()];
    };
    let mut keys = BTreeSet::new();
    all_keys(&doc, &mut keys);

    let mut out = Vec::new();
    for (key, required) in KEYS_REQUIRING {
        if keys.contains(*key) && declared < *required {
            out.push(format!(
                "`{key}` needs chore {}.{}.{}, chore_min_version declares {}.{}.{}",
                required.0, required.1, required.2, declared.0, declared.1, declared.2
            ));
        }
    }
    out
}

/// THE ONE THAT READS THE REAL FILE.
#[test]
fn the_declared_floor_covers_the_keys_this_repository_uses() {
    let path = chores_yml();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    assert!(
        declared_floor(&document(&text)).is_some(),
        "control: chores.yml must declare a chore_min_version, or every assertion \
         below passes for the wrong reason"
    );
    let found = violations(&text);
    assert!(
        found.is_empty(),
        "chores.yml uses keys its declared floor does not cover, so a chore between \
         the two passes the version check and then cannot parse the file at all: {found:?}"
    );
}

mod parser {
    use super::{declared_floor, document, violations};

    const OLD_FLOOR: &str = "\
version: \"3\"
chore_min_version: 0.6.0

tasks:
  test:
    cmds:
      - cmd: 'cargo test'
";

    #[test]
    fn a_key_newer_than_the_floor_is_reported() {
        let yaml = OLD_FLOOR.replace("      - cmd: 'cargo test'\n", "    timeout: 45m\n");
        let found = violations(&yaml);
        assert_eq!(
            found.len(),
            1,
            "expected exactly the timeout violation: {found:?}"
        );
        assert!(
            found[0].contains("0.10.0"),
            "the message must name the version needed"
        );
        assert!(
            found[0].contains("0.6.0"),
            "and the one declared, or it cannot be acted on"
        );
    }

    /// LEGAL INPUT THE SCAN USED TO REJECT, which is the other half of
    /// the witness and the half that is easy to skip. The old lookup
    /// was `line.strip_prefix("chore_min_version:")`, so a QUOTED KEY
    /// -- `"chore_min_version": 0.10.0`, which chore reads identically
    /// -- was not found at all, and `violations` then reported "no
    /// floor declared". Red, so it failed closed rather than letting
    /// anything through; but red on a correct manifest is still a
    /// guard refusing something legal, and only the parser makes it
    /// pass.
    #[test]
    fn a_quoted_floor_key_is_still_the_floor() {
        let yaml = OLD_FLOOR
            .replace("chore_min_version: 0.6.0", "\"chore_min_version\": 0.10.0")
            .replace("      - cmd: 'cargo test'\n", "    timeout: 45m\n");
        assert_eq!(
            declared_floor(&document(&yaml)),
            Some((0, 10, 0)),
            "a quoted key is the same key"
        );
        assert!(
            violations(&yaml).is_empty(),
            "0.10.0 covers `timeout`, and the manifest is correct"
        );
    }

    /// THE FIRST OF THE TWO SPELLINGS THIS WAS RETURNED FOR. A quoted
    /// key is the same key -- chore reads it identically -- and
    /// `line.starts_with("timeout:")` matched neither `"timeout":` nor
    /// `'timeout':`. Measured before the conversion: floor 0.9.0 with
    /// the keys quoted gave EXIT=0, 8 passed, with the floor wrong.
    #[test]
    fn a_quoted_key_is_the_same_key() {
        for spelling in ["\"timeout\"", "'timeout'"] {
            let yaml = OLD_FLOOR.replace(
                "      - cmd: 'cargo test'\n",
                &format!("    {spelling}: 45m\n"),
            );
            let found = violations(&yaml);
            assert_eq!(
                found.len(),
                1,
                "{spelling} is the key `timeout`, and the floor does not cover it: \
                 {found:?}"
            );
        }
    }

    /// THE SECOND. A flow mapping puts the key on a line that starts
    /// with something else entirely, so a line-prefix scan never sees
    /// it. Measured before the conversion: floor 0.9.0 with
    /// `net: { timeout: 45m }` gave EXIT=0, 8 passed.
    #[test]
    fn a_flow_mapping_hides_nothing() {
        let yaml = OLD_FLOOR.replace(
            "      - cmd: 'cargo test'\n",
            "    net: { timeout: 45m, on_timeout: [] }\n",
        );
        let found = violations(&yaml);
        assert_eq!(
            found.len(),
            2,
            "both keys are declared inside the flow mapping: {found:?}"
        );
    }

    /// A key nested deeper than the task is still an adoption of that
    /// key. The walk is recursive for this reason, and a scan that only
    /// looked at one indent would be a third spelling to miss.
    #[test]
    fn a_key_nested_deeper_is_still_used() {
        let yaml = OLD_FLOOR.replace(
            "      - cmd: 'cargo test'\n",
            "      - cmd: 'cargo test'\n        on_timeout:\n          - cmd: 'echo late'\n",
        );
        assert_eq!(
            violations(&yaml).len(),
            1,
            "on_timeout is used, wherever it sits"
        );
    }

    /// A key that merely CONTAINS the name is a different key, which a
    /// prefix scan could not tell apart either.
    #[test]
    fn a_similarly_named_key_is_a_different_key() {
        let yaml = OLD_FLOOR.replace("      - cmd: 'cargo test'\n", "    timeout_secs: 45\n");
        assert!(
            violations(&yaml).is_empty(),
            "`timeout_secs` is not `timeout`, and this table says nothing about it"
        );
    }

    /// A key NAMED IN A COMMENT is not a key. `chores.yml` here
    /// discusses `timeout:` at length before using it, and a guard that
    /// counted the prose would raise the floor of a file that does not
    /// need it -- and could never be trusted to mean what it says.
    #[test]
    fn a_key_only_mentioned_in_a_comment_is_not_used() {
        let yaml = OLD_FLOOR.replace(
            "      - cmd: 'cargo test'\n",
            "    # `timeout:` is the third layer, and `on_timeout:` runs after it\n",
        );
        assert!(
            violations(&yaml).is_empty(),
            "the prose above a key is not the key"
        );
    }

    /// A VARIABLE NAMED LIKE A FEATURE IS NOT THE FEATURE. `vars:` and
    /// `env:` hold names the user picks, so a `timeout` there is read by
    /// the task's commands and not by chore; reporting it would reject a
    /// manifest that needs no newer chore. The control beside each
    /// spelling puts the real key back and must still be reported, so
    /// the skip cannot have swallowed the task it sits in.
    #[test]
    fn a_variable_named_like_a_key_is_not_the_key() {
        for spelling in [
            "    vars:\n      timeout: '30'\n",
            "    env:\n      on_timeout: 'kill'\n",
            "    vars: { timeout: '30', on_timeout: x }\n",
        ] {
            let yaml = OLD_FLOOR.replace("      - cmd: 'cargo test'\n", spelling);
            assert_ne!(yaml, OLD_FLOOR, "the mutation must actually apply");
            assert!(
                violations(&yaml).is_empty(),
                "a variable is not chore's key: {spelling:?} gave {:?}",
                violations(&yaml)
            );
            let control = yaml.replace("tasks:\n  test:\n", "tasks:\n  test:\n    timeout: 45m\n");
            assert_eq!(
                violations(&control).len(),
                1,
                "control: the real key beside {spelling:?} is still reported"
            );
        }
    }

    /// A TASK NAMED LIKE A FREE-FORM MAP IS A TASK (Greptile on
    /// rust-fs-xfs#173). Under `tasks:` the keys are task names, so a task
    /// called `vars` or `env` has its body walked, and a `timeout:` in it
    /// is reported.
    #[test]
    fn a_task_named_vars_or_env_is_still_walked() {
        for task in ["vars", "env"] {
            let yaml = OLD_FLOOR.replace(
                "  test:\n    cmds:\n      - cmd: 'cargo test'\n",
                &format!("  {task}:\n    timeout: 45m\n    cmds:\n      - cmd: 'cargo test'\n"),
            );
            assert_ne!(yaml, OLD_FLOOR, "the mutation must actually apply");
            assert_eq!(
                violations(&yaml).len(),
                1,
                "the task `{task}` uses timeout: {:?}",
                violations(&yaml)
            );
        }
    }

    #[test]
    fn a_floor_that_covers_the_key_is_not_reported() {
        let yaml = OLD_FLOOR
            .replace("chore_min_version: 0.6.0", "chore_min_version: 0.10.0")
            .replace("      - cmd: 'cargo test'\n", "    timeout: 45m\n");
        assert!(violations(&yaml).is_empty(), "0.10.0 covers a 0.10.0 key");
    }

    #[test]
    fn a_newer_floor_than_needed_is_not_reported() {
        let yaml = OLD_FLOOR
            .replace("chore_min_version: 0.6.0", "chore_min_version: 0.11.0")
            .replace("      - cmd: 'cargo test'\n", "    on_timeout:\n");
        assert!(
            violations(&yaml).is_empty(),
            "the floor is a minimum, not an equality"
        );
    }

    #[test]
    fn a_quoted_floor_is_the_same_floor() {
        let yaml = OLD_FLOOR.replace("chore_min_version: 0.6.0", "chore_min_version: \"0.6.0\"");
        assert_eq!(
            declared_floor(&document(&yaml)),
            Some((0, 6, 0)),
            "a quoted version string is the same version -- the spelling that has \
             defeated three other comparisons in this repository"
        );
    }

    #[test]
    fn a_trailing_comment_does_not_hide_the_floor() {
        let yaml = OLD_FLOOR.replace(
            "chore_min_version: 0.6.0",
            "chore_min_version: 0.6.0 # bump when adopting a newer key",
        );
        assert_eq!(declared_floor(&document(&yaml)), Some((0, 6, 0)));
    }

    #[test]
    fn a_missing_floor_is_itself_a_violation() {
        let yaml = OLD_FLOOR.replace("chore_min_version: 0.6.0\n", "");
        assert_eq!(
            violations(&yaml).len(),
            1,
            "a file with no floor declares no floor"
        );
    }
}
