//! The debug run that lets the PR gate see an overflow guards itself.
//!
//! `overflow-checks` is on in the debug profile and off in release, so a
//! defect whose only symptom is an arithmetic overflow panic cannot be
//! observed by a release-only test run. Everything below exists to keep
//! one sentence true:
//!
//! > the pull-request gate still builds the library's own unit tests in
//! > the debug profile, with `EXPECT_OVERFLOW_CHECKS=1` set, on a job
//! > that actually gates the merge.
//!
//! # The sentence now has three links, and each breaks on its own
//!
//! `ci.yml` used to run `cargo test` itself. It does not any more:
//! every job in it runs chore tasks and nothing else, and the cargo
//! invocations live in `chores.yml`, wrapped in `../rust-fs-core/scripts/tier.sh` (the
//! output budget) around `scripts/test.sh` (the scratch directory),
//! with the target list produced by `scripts/test-targets.sh`. So the
//! one step this guard used to read has become a chain, and a chain is
//! only as good as the link nobody checked:
//!
//! | link | what it rules out | pinned by |
//! |---|---|---|
//! | `ci.yml` still runs `chore test:unit` on a gating job | the task being perfect and nothing calling it | [`the_pr_gate_still_tests_the_library_in_a_profile_that_can_see_an_overflow`] |
//! | `chores.yml`'s `test:unit` is the debug run it says it is | the job running a task that has quietly gained `--release`, or lost the handshake | [`the_debug_run_asks_the_build_to_prove_it_traps_overflows`] |
//! | `scripts/test-targets.sh unit` still names `--lib` | the workflow and the task both being right while the list they agree on stopped containing the library | [`the_unit_tier_target_list_still_names_the_library`] |
//!
//! The third link is the one a comment cannot replace. The task body
//! says `$(scripts/test-targets.sh unit)`: read as text it proves
//! nothing about what that command prints, so the script is RUN. It is
//! cheap, it needs no fixture, no tool and no VM -- which is what lets
//! this file sit in the `unit` tier it is guarding.
//!
//! # This repository is not in the same position as its siblings
//!
//! Stated plainly, because the difference is the whole reason this file
//! is shaped the way it is, and a guard ported from a sibling would be
//! satisfied here while the defect was still live.
//!
//! `ci.yml` **does** run tests in the debug profile on a pull request,
//! and always did. What matters is which *targets* those runs build.
//! Three of the four tiers -- `test:images`, `test:oracle`,
//! `test:kernel` -- take their targets from
//! `scripts/test-targets.sh <tier>`, which emits nothing but
//! `--test <name>` arguments, and `cargo test --test <name>` selects one
//! integration target and never builds the library's own unit tests.
//! The whole-suite run in `test:native` does build them, and is
//! `--release`. So "does any debug `cargo test` run on this trigger" is
//! the wrong question here: the answer is yes, and it was yes on the
//! tree where the library's arithmetic -- the largest body of it in the
//! crate -- was compiled with the checks on nowhere but a version tag.
//!
//! That is why the scan refuses a run whose targets come from a tier
//! other than `unit`, and why it refuses `--test <name>` however the
//! line is spelled. Drop either and the guard passes on the tree as it
//! stood before this change, which is the precise definition of a check
//! that cannot fail for the reason it exists.
//!
//! The file scoping is load-bearing for the same reason it always was.
//! `release.yml` runs `chore test` -- the whole gate, `test:unit`
//! included -- on a version tag, after the change has merged, detached
//! from the change and from the person who could have caught it. A scan
//! widened across every workflow would find the unit task running there
//! and report this repository as covered while the pull-request gate had
//! lost it, so the guard opens `ci.yml` and only `ci.yml`
//! ([`a_debug_run_outside_ci_yml_does_not_satisfy_this_guard`]).
//!
//! # Two halves, neither redundant
//!
//! | half | asks | cannot answer |
//! |---|---|---|
//! | the scans here | is the task still run by a gating job, still covering the library, still asked to check, and not disabled from the manifest | whether the build it produces actually traps |
//! | `overflow_checks` in `src/lib.rs` | does this build trap a real `u64::MAX + 1` | whether it was supposed to; it cannot notice its own absence |
//!
//! Delete the step and the runtime probe never runs at all. Keep the
//! step but drop the variable and the probe runs, finds nothing to
//! check, and passes doing nothing. Keep both and put
//! `overflow-checks = false` under `[profile.test]` and the step is
//! present, running, green and blind. Each needs its own guard.
//!
//! # Why this is an integration test and not a module under `src/`
//!
//! Cargo discovers `tests/*.rs` on its own, so there is no declaration
//! anywhere that can be deleted to switch this off, and `Cargo.toml`
//! sets no `autotests = false`. A guard living as a file under `src/`
//! behind a `#[cfg(test)] mod` line has no such protection: lose the
//! one line and the file stays, compiles into nothing, and asserts
//! nothing, with no lint to say so. That happened once already on a
//! sibling repository's version of this fix -- a `git reset --hard`
//! took the `mod` line, the suite went green, and seven assertions
//! silently ceased to exist.
//!
//! The runtime probe in `src/lib.rs` is the deliberate exception, and
//! is inline in `lib.rs` for the same reason: it must be part of the
//! library target the `unit` tier builds, and inline there is no
//! declaration to lose.

use saphyr::{LoadableYamlNode, Yaml};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The chore task whose body is the debug run this file is about.
const UNIT_TASK: &str = "test:unit";

/// The tier of `scripts/test-targets.sh` whose target list contains the
/// library. The other three emit nothing but `--test <name>`.
const TIER_COVERING_THE_LIBRARY: &str = "unit";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ci_yml() -> PathBuf {
    manifest_dir()
        .join(".github")
        .join("workflows")
        .join("ci.yml")
}

fn chores_yml() -> PathBuf {
    manifest_dir().join("chores.yml")
}

/// Read a file the guards depend on, or fail.
///
/// It panics rather than returning `None` on purpose. An
/// `if !path.exists() { return }` anywhere in this module would
/// reproduce the exact class of blindness the module exists to prevent:
/// an assertion that is present, runs, and cannot report the thing it
/// was written for. A missing workflow is a finding, not a skip.
fn read_or_panic(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}. This guard must fail rather than skip: a \
             version of it that returned early here would be the same \
             blindness it exists to prevent.",
            path.display()
        )
    })
}

/// Every test run in `script` that would compile the **library unit
/// tests** with overflow checks on.
///
/// Five things disqualify a line, and each one is a way the guard could
/// otherwise be satisfied by something that does not actually build the
/// library in debug:
///
/// - it is a comment. This is not defensive here, it is load bearing:
///   `chores.yml` and `ci.yml` both explain this tier in comment blocks
///   that quote what it does, so a scan that ignored comments would
///   still find it after the command itself had been deleted, and would
///   pass;
/// - it is an inline trailing comment on an otherwise-`--release` line;
/// - it passes `--release`, or names a profile explicitly;
/// - it sets a `CARGO_PROFILE_*` variable, which can turn overflow
///   checks off for the dev or test profile from outside the manifest;
/// - **its target selection does not include the library.** This is the
///   rule this repository needs and its siblings do not, and it has two
///   spellings now. `cargo test --test csum_oracle` is a genuine debug
///   run that builds no library unit test whatsoever; so is
///   `scripts/test.sh $(scripts/test-targets.sh oracle)`, because that
///   tier prints one `--test <name>` per file and nothing else.
///   Counting either would report the defect as already fixed. See
///   [`selects_the_library_unit_tests`].
///
/// `cargo build` lines are not test runs and are not considered.
///
/// And before any of those: **the line must BE a test run**, not
/// mention one (#118). See [`begins_with_a_test_run`].
fn runs_covering_the_library_unit_tests(script: &str) -> Vec<String> {
    script
        .lines()
        .filter_map(|raw| {
            if !begins_with_a_test_run(raw) {
                return None;
            }
            let command = raw.split(" #").next().unwrap_or(raw).trim();
            if command.contains("CARGO_PROFILE_") {
                return None;
            }
            // THE RUN'S OWN WORDS, NOT THE LINE'S (#136). `cargo test -r` is
            // `--release`, and a text scan does not see it. The profile flags
            // are read from the arguments that reach `cargo test`, up to the
            // first control operator outside quotes: in `cargo test --lib &&
            // cargo test --release` the debug run still counts, and in
            // `--target-dir "build;" -r` the `-r` is still this run's.
            let words = cargo_test_arguments(command)?;
            let arguments: Vec<&str> = words.iter().map(String::as_str).collect();
            // Everything past a bare `--` belongs to libtest rather than
            // to cargo, and the unit tier really does end `-- --skip
            // needs_host::`. A `--test` there is a name filter and says
            // nothing about which targets are built.
            let selection = before_the_double_dash(&arguments);
            if selection
                .iter()
                .any(|a| *a == "--release" || *a == "--profile" || a.starts_with("--profile="))
                || release_in(&arguments)
            {
                return None;
            }
            if !selects_the_library_unit_tests(selection) {
                return None;
            }
            Some(command.to_string())
        })
        .collect()
}

/// Flags that make cargo build the library target's own unit tests
/// whatever else is selected alongside them.
const LIBRARY_COVERING_FLAGS: [&str; 3] = ["--lib", "--tests", "--all-targets"];

/// Does this run's target selection build the library's own unit tests?
///
/// Three questions in order, because a later one would give the wrong
/// answer about a run an earlier one has already settled:
///
/// 1. an explicit library-covering flag settles it. `--lib`, `--tests`
///    and `--all-targets` all build the library unit tests, and they do
///    so even beside a `--test <name>`;
/// 2. otherwise, a run taking its targets from
///    `$(scripts/test-targets.sh <tier>)` is covered exactly when that
///    tier is `unit` -- the only one whose output begins `--lib`, which
///    [`the_unit_tier_target_list_still_names_the_library`] is what
///    proves. The others print `--test <name>` per file, so a run using
///    one builds no library unit test at all, and the substitution hides
///    from this scan what the flags would not;
/// 3. otherwise a `--test <name>` restricts the build to one integration
///    target, and anything else (a bare `cargo test`) builds everything.
///
/// WHOLE ARGUMENTS, NOT SUBSTRINGS. The version this replaces searched
/// for the text `--test ` -- with a trailing space, because `--tests`
/// DOES build the library unit tests and contains `--test`, and
/// swallowing it would have made the guard refuse a run that genuinely
/// satisfies it. That space was a fact about the spelling which had to
/// be remembered and could be lost in an edit; comparing whole
/// arguments is the same rule with nothing to remember, and `--test`,
/// `--tests` and `--all-targets` are simply three different arguments.
fn selects_the_library_unit_tests(selection: &[&str]) -> bool {
    if selection.iter().any(|a| LIBRARY_COVERING_FLAGS.contains(a)) {
        return true;
    }
    let tiers = target_list_tiers(selection);
    if !tiers.is_empty() {
        return tiers.iter().any(|tier| tier == TIER_COVERING_THE_LIBRARY);
    }
    !selection
        .iter()
        .any(|a| *a == "--test" || a.starts_with("--test="))
}

/// The tiers a run takes its target list from: `unit` in
/// `$(scripts/test-targets.sh unit)`.
///
/// The words are searched rather than the substitution parsed, so that
/// `$(...)`, `"$(...)"` and a backticked spelling all answer the same.
/// Nothing here evaluates the substitution -- the guard treats the
/// script as opaque on purpose and asks it directly instead, which is
/// the third link of the chain.
fn target_list_tiers(selection: &[&str]) -> Vec<String> {
    const SCRIPT: &str = "test-targets.sh";
    let joined = selection.join(" ");
    joined
        .match_indices(SCRIPT)
        .filter_map(|(at, _)| {
            let tier = joined[at + SCRIPT.len()..].split_whitespace().next()?;
            let tier = tier.trim_matches(|c| c == ')' || c == '"' || c == '\'' || c == '`');
            (!tier.is_empty()).then(|| tier.to_string())
        })
        .collect()
}

/// The arguments before a bare `--`, which is where cargo's own
/// arguments stop and libtest's begin.
fn before_the_double_dash<'a>(arguments: &'a [&'a str]) -> &'a [&'a str] {
    match arguments.iter().position(|a| *a == "--") {
        Some(at) => &arguments[..at],
        None => arguments,
    }
}

/// The arguments that reach `cargo test` when `command` runs, or `None`
/// when it runs no test suite at all.
///
/// THE WRAPPERS ARE PART OF THE COMMAND NOW. A tier in `chores.yml` is
/// spelled
///
/// ```text
///   EXPECT_OVERFLOW_CHECKS=1 ../rust-fs-core/scripts/tier.sh test:unit unit 400 21000 \
///       -- scripts/test.sh --locked $(scripts/test-targets.sh unit)
/// ```
///
/// and every word that decides the profile is on the far side of two
/// scripts. `tier.sh` runs what follows its `--` under an output budget;
/// `test.sh` makes a scratch directory inside the repository and ends in
/// `cargo test --features cli "$@"`. Both pass their arguments through unchanged, so
/// what reaches cargo is recoverable by walking the chain -- and a scan
/// that knew only the words `cargo test` would read that line and find
/// nothing at all.
///
/// Anything else -- `cargo build`, `vm.sh guest-test`, a bare script --
/// is not a cargo test run *here*, and says so by answering `None`.
/// `test:vm`'s in-guest run is the notable one: its cargo invocation
/// lives in the harness sibling's own guest script, which this
/// repository does not read.
fn cargo_test_arguments(command: &str) -> Option<Vec<String>> {
    fn walk(words: &[String]) -> Option<Vec<String>> {
        let at = words.iter().position(|w| !is_an_assignment(w))?;
        let rest = &words[at..];
        let program = rest[0].rsplit('/').next().unwrap_or(&rest[0]);
        match program {
            "cargo" => (rest.get(1).map(String::as_str) == Some("test"))
                .then(|| rest.get(2..).unwrap_or_default().to_vec()),
            // tier.sh LABEL LOG MAX-LINES MAX-BYTES -- COMMAND...
            "tier.sh" => {
                let at = rest.iter().position(|w| w == "--")?;
                walk(rest.get(at + 1..)?)
            }
            // test.sh's last line is `cargo test --features cli "$@"`.
            "test.sh" => Some(rest.get(1..).unwrap_or_default().to_vec()),
            _ => None,
        }
    }
    walk(&first_command_words(command))
}

/// Whether `word` is a `NAME=value` assignment, which the shell applies
/// to one command's environment rather than treating as the command.
fn is_an_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
            && !name.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// The words of the first command on a line, quotes removed as the
/// shell removes them.
fn first_command_words(command: &str) -> Vec<String> {
    shell_commands(command)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// `command` split into the commands the shell's control operators --
/// `&&`, `||`, `;`, `|` and `&` -- separate, each as its words, with
/// quotes and backslashes removed as the shell removes them.
///
/// Operators need no spaces: `true&&cargo test -r` is a `cargo test` run,
/// and in `cargo test --lib&&rm -rf build` the `-rf` is `rm`'s. An `&` or
/// `|` straight after `>` or `<` is part of a redirection (`2>&1`, `>|`).
/// Inside quotes, or after a backslash, nothing is an operator or a word
/// break, and what the quotes held is the word: `--features 'a;b' -r` is
/// one command, and `'-r'` is `-r`.
fn shell_commands(command: &str) -> Vec<Vec<String>> {
    let mut commands = vec![Vec::new()];
    let mut word: Option<String> = None;
    let mut chars = command.chars().peekable();
    let mut previous = None;
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                let word = word.get_or_insert_with(String::new);
                word.extend(chars.by_ref().take_while(|&q| q != '\''));
            }
            '"' => {
                let word = word.get_or_insert_with(String::new);
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.extend(chars.next());
                        }
                        _ => word.push(q),
                    }
                }
            }
            '\\' => word.get_or_insert_with(String::new).extend(chars.next()),
            ';' | '&' | '|' if !matches!(previous, Some('>' | '<')) => {
                if c != ';' && chars.peek() == Some(&c) {
                    chars.next();
                }
                commands.last_mut().unwrap().extend(word.take());
                commands.push(Vec::new());
            }
            c if c.is_whitespace() => commands.last_mut().unwrap().extend(word.take()),
            c => word.get_or_insert_with(String::new).push(c),
        }
        previous = Some(c);
    }
    commands.last_mut().unwrap().extend(word.take());
    commands
}

/// Whether `cargo test`'s `arguments`, up to the end of its own command,
/// carry `-r`. See [`runs_covering_the_library_unit_tests`].
///
/// `arguments` are one command's words ([`shell_commands`]), so in
/// `cargo test --lib && rm -rf build` the `r` in `-rf` is not among them.
fn release_in(arguments: &[&str]) -> bool {
    const LONG_OPTIONS_TAKING_A_VALUE: [&str; 15] = [
        "--package",
        "--exclude",
        "--features",
        "--target",
        "--target-dir",
        "--manifest-path",
        "--profile",
        "--test",
        "--bin",
        "--example",
        "--bench",
        "--jobs",
        "--message-format",
        "--color",
        "--config",
    ];
    let mut next_is_a_value = false;
    for &argument in arguments {
        if std::mem::take(&mut next_is_a_value) {
            continue;
        }
        if argument == "--" {
            return false;
        }
        if argument.starts_with("--") {
            next_is_a_value =
                !argument.contains('=') && LONG_OPTIONS_TAKING_A_VALUE.contains(&argument);
        } else if let Some(cluster) = argument.strip_prefix('-') {
            for (at, flag) in cluster.char_indices() {
                match flag {
                    'r' => return true,
                    'p' | 'j' | 'F' | 'Z' => {
                        next_is_a_value = at + 1 == cluster.len();
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    false
}

/// Whether `line` starts a test run the shell performs unconditionally:
/// at the block's own left margin, with nothing before the command but
/// `NAME=value` assignments.
///
/// This does not interpret the shell, and says so. The text used to
/// count wherever `cargo test` appeared in it, so `echo "cargo test
/// --locked --lib"`, or the real command indented inside an `if false;
/// then` branch, satisfied the guard with no debug run at all (#118).
/// Requiring the command to begin an unindented line rejects both, and
/// admits every real invocation in this repository's tasks. A command
/// in a conditional or loop written at the left margin would still
/// count; a guard that parsed bash would acquire a new defeat for every
/// way a block can be written, and this one only has to recognise the
/// one way the gate's tier is written.
fn begins_with_a_test_run(line: &str) -> bool {
    !line.starts_with(char::is_whitespace) && cargo_test_arguments(line).is_some()
}

/// Whether this command arms the handshake for the run it performs.
///
/// A leading `NAME=value`, which is how the shell puts a variable into
/// one command's environment and is how the tier sets it. Matched on
/// the name with a non-empty value, exactly as `src/lib.rs` reads it:
/// `EXPECT_OVERFLOW_CHECKS=` sets the variable to nothing and the probe
/// returns without asserting, so a tier spelled that way would look
/// armed here and prove nothing there.
fn sets_the_handshake(command: &str) -> bool {
    first_command_words(command)
        .iter()
        .take_while(|word| is_an_assignment(word))
        .any(|word| {
            word.split_once('=')
                .is_some_and(|(name, value)| name == "EXPECT_OVERFLOW_CHECKS" && !value.is_empty())
        })
}

/// The runs that additionally ask the build to prove it traps.
///
/// A subset of [`runs_covering_the_library_unit_tests`]: those which
/// also set the `EXPECT_OVERFLOW_CHECKS` handshake, so that
/// `overflow_checks::the_build_the_gate_asked_to_check_does_check`
/// performs an overflow and fails if the build let it through.
///
/// A run carrying the handshake but also `--release` is not counted,
/// because the function above has already excluded it. Such a tier is a
/// misconfiguration and it fails loudly rather than quietly: the checks
/// are legitimately off in release, so the assertion the handshake arms
/// would fire there every time.
fn debug_runs_that_prove_the_build_traps(script: &str) -> Vec<String> {
    runs_covering_the_library_unit_tests(script)
        .into_iter()
        .filter(|command| sets_the_handshake(command))
        .collect()
}

/// Every line of `script` that runs `chore <task>` unconditionally.
///
/// The link between the workflow and the tasks, and deliberately a
/// literal one: a job that reaches `test:unit` indirectly, the way
/// `chore test` does through `test:native`, is not accepted. Following
/// that would mean modelling chore's own task graph -- a second parser,
/// with a second set of defeats -- to establish something the workflow
/// can state in three words. `ci.yml` names the task, and this reads
/// the name.
fn runs_of_the_chore_task(script: &str, task: &str) -> Vec<String> {
    script
        .lines()
        .filter_map(|raw| {
            if raw.starts_with(char::is_whitespace) {
                return None;
            }
            let command = raw.split(" #").next().unwrap_or(raw).trim();
            let words = first_command_words(command);
            let at = words.iter().position(|w| !is_an_assignment(w))?;
            (words.get(at).map(String::as_str) == Some("chore")
                && words.get(at + 1).map(String::as_str) == Some(task))
            .then(|| command.to_string())
        })
        .collect()
}

/// WHAT ELSE DECIDES WHETHER A STEP GATES.
///
/// The first version of this guard matched the text of a `- run:` line
/// and never looked at anything else in the step. That is enough to
/// find the command and useless for deciding whether the command's
/// result is read. Measured against this repository's own workflow:
/// adding `if: false` to the step, or `continue-on-error: true`, left
/// every one of the guard's 31 tests green while the gate went blind.
/// A step that runs and whose result nothing reads is this
/// constellation's own named defect, reproduced inside the guard
/// written to prevent it.
///
/// So the list is enumerated first, rather than discovered one defeat
/// at a time. A `run:` step gates a pull request only if ALL of these
/// hold:
///
/// 1. the step carries no `if:` -- a false condition skips it;
/// 2. the step carries no `continue-on-error:` -- its failure is
///    discarded;
/// 3. its JOB carries no `if:` -- same reasoning, one level up;
/// 4. its JOB carries no `continue-on-error:`;
/// 5. the workflow's `on:` still includes `pull_request` -- a scan
///    scoped to `ci.yml` assumes `ci.yml` is what runs on a pull
///    request, and that is a fact about the file, not a given.
///
/// OVER-STRICT IS THE SAFE DIRECTION HERE, so 1 and 2 reject on the
/// key's PRESENCE rather than trying to evaluate it. `if: false`,
/// `if: ${{ false }}`, and an `if:` on an expression that happens to
/// evaluate false are distinct spellings, and this crate has already
/// been caught by four spellings of one manifest key -- enumerating
/// them is the losing game. A step that genuinely needs a condition
/// can be split out; a guard that tries to interpret conditions is a
/// guard with a new defeat every time GitHub adds syntax.
///
/// `ci-ok` carries `if: always()` and is therefore not counted. That
/// costs nothing and is right: it aggregates the other jobs' results
/// and runs no test. The jobs that run tests carry no condition at all.
///
/// # Why this is parsed and no longer scanned
///
/// The version this replaces hand-rolled the YAML: `.lines()`, an
/// indent count, `split_once(':')` for the key, and `after != "|"` for
/// a block scalar. It was defeated three more times after the five
/// spellings above, and each defeat was the same shape -- ordinary
/// YAML the scanner had not been taught:
///
/// ```text
///   "if": false               quoted key -- matched no NON_GATING_KEYS
///                             entry, so the step counted as gating
///                             while Actions skipped it. SILENT.
///   "continue-on-error": true same.
///   # pull_request:           a substring match over the `on:` block's
///                             raw text, comments included, so
///                             commenting the trigger out left the
///                             guard green. SILENT.
///   run: |-  / run: >         only a bare `|` opened a block, so every
///                             other legal style was read as the
///                             command itself and the block's contents
///                             never parsed. LOUD -- it failed a
///                             correct workflow.
/// ```
///
/// Quoted keys, block scalar styles, comments and nested mappings are
/// not edge cases; they are the grammar. A parser handles all of them
/// by construction, and does not need to be taught the next one. The
/// sibling `rust-fs-xfs` copy patched each hole individually and its
/// own comments record the cost: the identical quote-normalisation was
/// added to its TOML key scan, and then had to be added again, a few
/// dozen lines away, to its YAML key scan. The same lesson twice in one
/// file is the argument against learning it a third time.
///
/// `saphyr` is a dev-dependency, so nothing here reaches a consumer of
/// the crate.
#[derive(Debug)]
struct Step {
    keys: Vec<String>,
    run: String,
}

#[derive(Debug)]
struct Job {
    /// The job's key under `jobs:`, such as `unit`.
    id: String,
    keys: Vec<String>,
    steps: Vec<Step>,
}

#[derive(Debug)]
struct Workflow {
    triggers: Vec<String>,
    jobs: Vec<Job>,
}

/// The value of `name` in a YAML mapping, or `None`.
///
/// By name rather than by constructing a key, because `saphyr`'s `Yaml`
/// borrows the source text and building one to hand to `get` is more
/// ceremony than the lookup is worth here.
fn field<'a, 'b>(node: &'a Yaml<'b>, name: &str) -> Option<&'a Yaml<'b>> {
    node.as_mapping()?
        .iter()
        .find(|(key, _)| key.as_str() == Some(name))
        .map(|(_, value)| value)
}

/// The keys of a YAML mapping, as plain strings.
///
/// The parser has already resolved the quoting, so `"if"`, `'if'` and
/// `if` all arrive here as `if`. That is the whole of the quoted-key
/// fix: there is no un-quoting step to forget.
fn keys_of(node: &Yaml) -> Vec<String> {
    node.as_mapping()
        .map(|mapping| {
            mapping
                .iter()
                .filter_map(|(key, _)| key.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Structure a workflow far enough to answer the five questions above.
///
/// Panics on a workflow it cannot parse, deliberately. A guard that
/// returned an empty `Workflow` for a file it did not understand would
/// report "no gating job runs this task" -- which is a failure, so that
/// direction is safe -- but a guard that returned early with a PASS
/// would be the blindness this module exists to prevent. Failing on the
/// parse error names the real problem instead of a consequence of it.
fn parse_workflow(text: &str) -> Workflow {
    let documents = Yaml::load_from_str(text).unwrap_or_else(|e| {
        panic!(
            "workflow is not valid YAML: {e}. This guard reads the workflow \
             rather than scanning its text, so a file it cannot parse is a \
             failure and never a pass."
        )
    });
    let Some(document) = documents.first() else {
        return Workflow {
            triggers: Vec::new(),
            jobs: Vec::new(),
        };
    };

    // `on:` takes three legal shapes: a mapping of trigger names, a
    // sequence of them, or a single scalar. All three are names.
    //
    // Note that `on` survives as the string key `on` and is not folded
    // into the boolean `true` -- saphyr implements the YAML 1.2 core
    // schema, where only `true`/`false` are booleans. The YAML 1.1
    // reading that would break every GitHub workflow ever written does
    // not apply.
    let triggers = match field(document, "on") {
        Some(on) if on.as_mapping().is_some() => keys_of(on),
        Some(on) if on.as_sequence().is_some() => on
            .as_sequence()
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Some(on) => on.as_str().map(str::to_string).into_iter().collect(),
        None => Vec::new(),
    };

    let mut jobs = Vec::new();
    if let Some(mapping) = field(document, "jobs").and_then(Yaml::as_mapping) {
        for (id, body) in mapping.iter() {
            let steps = field(body, "steps")
                .and_then(Yaml::as_sequence)
                .into_iter()
                .flatten()
                .map(|step| Step {
                    keys: keys_of(step),
                    // A `run:` block of any style -- `|`, `|-`, `|+`,
                    // `>`, `>-`, `|2` -- arrives as one string with the
                    // block folded per its own rules, so a command
                    // inside a shell loop is seen whole rather than as
                    // fragments, and no style is mistaken for the
                    // command itself.
                    run: field(step, "run")
                        .and_then(Yaml::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect();
            jobs.push(Job {
                id: id.as_str().unwrap_or_default().to_string(),
                keys: keys_of(body),
                steps,
            });
        }
    }

    Workflow { triggers, jobs }
}

/// Does this workflow still run on a pull request at all?
///
/// A whole-name comparison against the parsed trigger keys. The version
/// this replaces asked `wf.triggers.contains("pull_request")` of the
/// `on:` block's raw text -- comments and blank lines included -- so
/// the word appearing anywhere in it satisfied the guard. Commenting
/// the real key out, or deleting it and leaving a comment naming it,
/// left `ci.yml` no longer running on pull requests at all with the
/// guard still green.
///
/// `pull_request_target` DELIBERATELY DOES NOT COUNT, and the omission
/// is the point rather than an oversight. It runs against the base
/// repository with a write token and the repository's secrets, and it
/// checks out the base ref by default -- so a workflow triggered only
/// that way may never build the contributor's code at all, and
/// accepting it as proof the merge is gated is permissive in the worst
/// direction. `rust-fs-xfs#146` and `rust-fs-ext4#149` record it as a
/// live gap in the hand-rolled guard this file replaces, where the
/// clause was written by hand and then copied between repositories.
///
/// A parser has no opinion about `pull_request_target` unless someone
/// writes one. So it is not written. If this repository ever needs it
/// accepted, that is a decision with its own justification, and it
/// comes with a check that the checkout selects the pull request head.
fn runs_on_pull_request(wf: &Workflow) -> bool {
    wf.triggers.iter().any(|t| t == "pull_request")
}

/// Why `workflow` gates no pull request at all, or `None` if it does.
///
/// The real-file assertions below ask this FIRST. Without it, a
/// workflow whose `on:` block moved reported that no gating job runs
/// the task, which sends the reader to a job that is fine (#124). This
/// names the triggers that were found instead, and says why
/// `pull_request_target` alone does not count.
fn not_a_pull_request_gate(workflow: &str) -> Option<String> {
    let wf = parse_workflow(workflow);
    if runs_on_pull_request(&wf) {
        return None;
    }
    let mut why = format!(
        "the workflow does not trigger on `pull_request` at all (its triggers: {:?}), so \
         none of its steps gates a pull request however they are written. The steps are \
         not the problem; the `on:` block is.",
        wf.triggers
    );
    if wf.triggers.iter().any(|t| t == "pull_request_target") {
        why.push_str(
            " `pull_request_target` alone is refused on purpose: it runs against the base \
             repository and may never build the contributor's code. See \
             `runs_on_pull_request`; carry `pull_request:` beside it.",
        );
    }
    Some(why)
}

/// Keys whose presence on a step or job means its result does not gate.
const NON_GATING_KEYS: [&str; 2] = ["if", "continue-on-error"];

/// Walk a workflow's steps and collect what `select` finds in each
/// `run:`.
///
/// `gating` restricts the walk to steps whose result the pull-request
/// gate actually reads: the workflow must still trigger on a pull
/// request, and neither the job nor the step may carry a key from
/// [`NON_GATING_KEYS`].
///
/// One walk rather than two. The headline assertion used the line-based
/// scan while only the handshake assertion was step-aware, so under
/// `if: false` the headline PASSED and its failure message would have
/// claimed the pull-request gate could see an overflow when the step it
/// names does not run. Every defeat spelling still turned the suite red
/// through the other assertion, so this was a precision defect rather
/// than a hole -- but it left the "runs without --release" property
/// verified line-based, and defeatable if the handshake assertion were
/// ever weakened. Both halves share this walk now and cannot drift
/// apart again.
fn scan_steps(workflow: &str, gating: bool, select: fn(&str) -> Vec<String>) -> Vec<String> {
    let wf = parse_workflow(workflow);
    if gating && !runs_on_pull_request(&wf) {
        return Vec::new();
    }
    let carries_a_non_gating_key =
        |keys: &[String]| keys.iter().any(|k| NON_GATING_KEYS.contains(&k.as_str()));

    let mut out = Vec::new();
    for job in &wf.jobs {
        if gating && carries_a_non_gating_key(&job.keys) {
            continue;
        }
        for step in &job.steps {
            if gating && carries_a_non_gating_key(&step.keys) {
                continue;
            }
            out.extend(select(&step.run));
        }
    }
    out
}

/// The `chore test:unit` invocations whose result the pull-request gate
/// actually reads.
fn gating_runs_of_the_unit_task(workflow: &str) -> Vec<String> {
    scan_steps(workflow, true, |script| {
        runs_of_the_chore_task(script, UNIT_TASK)
    })
}

/// The ids of the jobs whose results the pull-request gate reads.
///
/// For the failure message of the first link: "none of these jobs runs
/// the task" is a diagnosis, while "the task is not run" leaves the
/// reader to work out where to look.
fn gating_jobs(workflow: &str) -> Vec<String> {
    let wf = parse_workflow(workflow);
    if !runs_on_pull_request(&wf) {
        return Vec::new();
    }
    wf.jobs
        .iter()
        .filter(|job| {
            !job.keys
                .iter()
                .any(|k| NON_GATING_KEYS.contains(&k.as_str()))
        })
        .map(|job| job.id.clone())
        .collect()
}

/// Every task in `chores.yml`, as its id and the commands of its
/// `cmds:`.
///
/// A `cmds:` entry comes in three shapes and all three are ordinary
/// chore: a plain string, a mapping with a `cmd:` key (which is how
/// `on_timeout:` writes one), and a mapping with a `task:` key naming
/// another task. The third carries no command of its own and is
/// skipped -- the task it names is walked in its own right, so nothing
/// is missed by not following it.
///
/// Panics on a manifest it cannot parse, for the reason
/// [`parse_workflow`] does.
fn task_bodies(manifest: &str) -> Vec<(String, Vec<String>)> {
    let documents = Yaml::load_from_str(manifest).unwrap_or_else(|e| {
        panic!(
            "chores.yml is not valid YAML: {e}. This guard reads the manifest \
             rather than scanning its text, so a file it cannot parse is a \
             failure and never a pass."
        )
    });
    let Some(document) = documents.first() else {
        return Vec::new();
    };
    let Some(tasks) = field(document, "tasks").and_then(Yaml::as_mapping) else {
        return Vec::new();
    };
    tasks
        .iter()
        .map(|(id, body)| {
            let commands = match field(body, "cmds") {
                Some(cmds) if cmds.as_str().is_some() => {
                    vec![cmds.as_str().unwrap_or_default().to_string()]
                }
                Some(cmds) => cmds
                    .as_sequence()
                    .into_iter()
                    .flatten()
                    .filter_map(|item| {
                        item.as_str()
                            .or_else(|| field(item, "cmd").and_then(Yaml::as_str))
                            .map(str::to_string)
                    })
                    .collect(),
                None => Vec::new(),
            };
            (id.as_str().unwrap_or_default().to_string(), commands)
        })
        .collect()
}

/// One task's commands as a single script, or a failure naming the
/// tasks that do exist.
///
/// A missing task is a finding, not a skip: the workflow runs it by
/// name, so a `chores.yml` without it is a gate that dies at the runner
/// rather than a gate that is fine.
fn task_script_or_panic(manifest: &str, task: &str) -> String {
    let bodies = task_bodies(manifest);
    match bodies.iter().find(|(id, _)| id == task) {
        Some((_, commands)) => commands.join("\n"),
        None => panic!(
            "chores.yml has no `{task}` task, and ci.yml runs it by name. The tasks it \
             does have: {:?}",
            bodies.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
        ),
    }
}

/// THE FIRST TWO LINKS. Reads the workflow this repository's pull
/// requests are gated by, and the task that workflow runs, and refuses
/// if the chain from one to the other stops compiling the library unit
/// tests with the overflow checks on.
#[test]
fn the_pr_gate_still_tests_the_library_in_a_profile_that_can_see_an_overflow() {
    let path = ci_yml();
    let workflow = read_or_panic(&path);

    if let Some(why) = not_a_pull_request_gate(&workflow) {
        panic!("{}: {why}", path.display());
    }
    assert!(
        !gating_runs_of_the_unit_task(&workflow).is_empty(),
        "no job in {} that gates a pull request runs `chore {UNIT_TASK}`, so the debug \
         profile -- the only one with overflow checks on -- is built nowhere the gate can \
         see. The jobs whose results the gate reads are {:?}. Every job in this workflow \
         runs chore tasks and nothing else, so this is the link between the workflow and \
         the run: the task can be perfect and buy nothing while no job calls it. `chore \
         test` reaches it through `test:native` and is deliberately not accepted here -- \
         see `runs_of_the_chore_task`.",
        path.display(),
        gating_jobs(&workflow),
    );

    let chores = chores_yml();
    let script = task_script_or_panic(&read_or_panic(&chores), UNIT_TASK);
    assert!(
        !runs_covering_the_library_unit_tests(&script).is_empty(),
        "the `{UNIT_TASK}` task in {} runs no test that builds the library unit tests \
         without `--release`, so a defect whose only symptom is an arithmetic overflow \
         panic can merge without the PR gate ever seeing it. A tier taking its targets \
         from `scripts/test-targets.sh` with any tier but `{TIER_COVERING_THE_LIBRARY}` \
         does not count: those lists are `--test <name>` per file, which builds one \
         integration target and no library unit tests at all. The whole-suite run does \
         cover them, and does not help either -- it is `--release`, where the checks are \
         off by design.",
        chores.display(),
    );
}

/// The other half of the task scan: the tier covers the library, but
/// does it ask the build anything?
///
/// # Why a handshake rather than more spellings
///
/// The manifest scan below reads `Cargo.toml` and asks whether a known
/// spelling of "overflow checks are off" is present. Several spellings
/// of the key were needed before it was right, and then routes turned
/// up that are not in that file at all: a
/// `CARGO_PROFILE_TEST_OVERFLOW_CHECKS` variable set anywhere along the
/// chain, and a `.cargo/config.toml`, which nothing here reads. All of
/// them leave the debug tier present, running, green and blind.
///
/// They are all the same shape: a scanner enumerating the ways a thing
/// can be disabled, in the places it happens to look. Another pass buys
/// the next one. So the question is put to the build instead -- perform
/// an overflow, see whether you are stopped -- and this test's job
/// shrinks to making sure the gate still asks it.
#[test]
fn the_debug_run_asks_the_build_to_prove_it_traps_overflows() {
    let path = chores_yml();
    let script = task_script_or_panic(&read_or_panic(&path), UNIT_TASK);

    assert!(
        !debug_runs_that_prove_the_build_traps(&script).is_empty(),
        "no command in the `{UNIT_TASK}` task of {} covers the library unit tests without \
         `--release` while setting EXPECT_OVERFLOW_CHECKS to a non-empty value, so \
         nothing checks whether the profile the gate builds actually traps an arithmetic \
         overflow. Reading Cargo.toml is not enough: the checks can also be turned off by \
         a CARGO_PROFILE_TEST_OVERFLOW_CHECKS variable anywhere along the chain, or by a \
         .cargo/config.toml, neither of which is in any file this test reads. The \
         handshake is what arms the one check that cannot be fooled by where the setting \
         lives -- see `overflow_checks` in src/lib.rs.",
        path.display(),
    );
}

/// THE THIRD LINK, AND THE ONE NO COMMENT CAN REPLACE.
///
/// The task body says `$(scripts/test-targets.sh unit)`. Read as text
/// that is a promise; the scans above treat it as opaque, and this test
/// is what makes the script keep it. Workflow and task could both be
/// perfect while the script had stopped emitting `--lib`, and the
/// failure would be silent in the worst way: the tier would still run,
/// still pass, and still build not one line of the library's arithmetic
/// with the checks on.
///
/// So the script is executed. It is cheap, it reads nothing but
/// `tests/*.rs`, and it needs no tool, no fixture and no VM -- which is
/// what lets this file live in the `unit` tier it is guarding.
///
/// `--skip needs_host::` in that output is expected and is not a
/// reduction of this run: library tests that need a fixture or the VM
/// live in a module of that name, and the whole-suite release run is
/// what executes them, with the VM up.
#[test]
fn the_unit_tier_target_list_still_names_the_library() {
    let targets = |tier: &str| -> Vec<String> {
        let script = manifest_dir().join("scripts").join("test-targets.sh");
        let output = Command::new("bash")
            .arg(&script)
            .arg(tier)
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "cannot run {} {tier}: {e}. This guard must fail rather than skip: the \
                     script is what decides whether the gate builds the library at all.",
                    script.display()
                )
            });
        assert!(
            output.status.success(),
            "{} {tier} exited {:?}: {}",
            script.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .map(str::to_string)
            .collect()
    };

    let unit = targets(TIER_COVERING_THE_LIBRARY);
    assert_eq!(
        unit.first().map(String::as_str),
        Some("--lib"),
        "scripts/test-targets.sh {TIER_COVERING_THE_LIBRARY} no longer begins its target \
         list with `--lib`, so `chore {UNIT_TASK}` builds no library unit test and the \
         debug profile is compiled for the integration targets alone. The task body and \
         the workflow can both be correct while this is false -- which is why it is run \
         rather than read. It printed: {unit:?}",
    );

    // AND THE TIERS THAT DO NOT, which is what makes the tier NAME
    // load-bearing in `selects_the_library_unit_tests` rather than a
    // detail of it. A tier swapped for one of these in the task body is
    // the same defect as a `--test <name>` written by hand, one
    // substitution further from the reader.
    for tier in ["images", "oracle", "kernel"] {
        let list = targets(tier);
        assert!(
            !list
                .iter()
                .any(|a| LIBRARY_COVERING_FLAGS.contains(&a.as_str())),
            "scripts/test-targets.sh {tier} now names the library, so `unit` is no longer \
             the only tier that covers it and `selects_the_library_unit_tests` is refusing \
             a run that would satisfy this guard. It printed: {list:?}",
        );
    }
}

/// THE DISTINCTION THIS REPOSITORY NEEDS AND ITS SIBLINGS DO NOT.
///
/// A run that selects integration targets by name must not satisfy the
/// guard. Such runs are real `cargo test` invocations, they carry no
/// `--release`, they run on every pull request, and they build not one
/// library unit test between them.
///
/// The `kernel-gate` job that made this urgent is gone -- it was fifty
/// steps of `cargo test --test <name>` on the runner -- and its shape is
/// pinned below anyway, because the parser must go on refusing it. The
/// shape did not leave with the job: `chore test:images`, `test:oracle`
/// and `test:kernel` are the same runs one substitution away, since
/// `scripts/test-targets.sh <tier>` prints `--test <name>` per file for
/// all three. Both spellings are asserted here, so a future edit
/// widening the parser to count either fails rather than quietly
/// reporting this repository's defect as fixed.
#[test]
fn a_run_that_names_its_integration_targets_does_not_satisfy_this_guard() {
    let the_old_kernel_gate = "\
jobs:
  kernel-gate:
    steps:
      - run: cargo test --test csum_oracle -- --nocapture
      - run: cargo test --test super_write_oracle -- --nocapture
      - run: cargo test --test transaction_oracle -- --nocapture
      - run: |
          for t in bootstrap_chain btree_oracle capi compression_oracle \\
                   fs_oracle fstree_oracle write_oracle xattr_oracle; do
            cargo test --test \"$t\" -- --nocapture
          done
";
    assert!(
        scan_steps(
            the_old_kernel_gate,
            false,
            runs_covering_the_library_unit_tests
        )
        .is_empty(),
        "the old kernel-gate's `--test <name>` runs were debug runs that built no library \
         unit tests -- counting them would have reported this repository's defect as \
         already fixed, and counting them now would do it again"
    );

    // THE SAME DEFECT IN THE SHAPE IT WOULD ARRIVE IN TODAY: a tier
    // whose targets come from a list that names integration targets and
    // nothing else. No `--release`, the handshake set, and still not one
    // library unit test built.
    let another_tiers_target_list = "\
EXPECT_OVERFLOW_CHECKS=1 ../rust-fs-core/scripts/tier.sh test:unit unit 400 21000 -- \
scripts/test.sh --locked $(scripts/test-targets.sh oracle)
";
    assert!(
        runs_covering_the_library_unit_tests(another_tiers_target_list).is_empty(),
        "`$(scripts/test-targets.sh oracle)` expands to `--test <name>` per file, so this \
         builds one integration target per suite and no library unit test; the \
         substitution must not hide from the guard what the flags would not"
    );

    // And the control, so the exclusion is about the target list rather
    // than about the wrapper chain the tier is written in.
    let the_real_tier = "\
EXPECT_OVERFLOW_CHECKS=1 ../rust-fs-core/scripts/tier.sh test:unit unit 400 21000 -- \
scripts/test.sh --locked $(scripts/test-targets.sh unit)
";
    assert_eq!(
        debug_runs_that_prove_the_build_traps(the_real_tier).len(),
        1,
        "the unit tier's own shape must be counted, or every assertion above passes for \
         the wrong reason"
    );

    // And the real guards must be reading the real files' content.
    let workflow = read_or_panic(&ci_yml());
    if let Some(why) = not_a_pull_request_gate(&workflow) {
        panic!("{}: {why}", ci_yml().display());
    }
    assert!(
        !gating_runs_of_the_unit_task(&workflow).is_empty(),
        "the guards above must be satisfied by ci.yml's own content, not by any of the \
         strings in this test"
    );
    assert!(
        !debug_runs_that_prove_the_build_traps(&task_script_or_panic(
            &read_or_panic(&chores_yml()),
            UNIT_TASK
        ))
        .is_empty(),
        "and by chores.yml's own content"
    );
}

/// The file scoping, which matters here for the same reason it always
/// did, with a different file on the other side of it.
///
/// `release.yml` runs `chore test` on a version tag. That reaches
/// `test:unit` through `test:native`, so the library's unit tests ARE
/// compiled in debug with the handshake set -- after the change has
/// merged, detached from the change and from the person who could have
/// caught it. A scan widened across every workflow would find it and
/// report this repository as covered.
///
/// Two things keep that out, and this pins both: the guard opens
/// `ci.yml` and nothing else, and a workflow that does not trigger on
/// `pull_request` gates nothing whatever its steps say.
#[test]
fn a_debug_run_outside_ci_yml_does_not_satisfy_this_guard() {
    let release_yml_as_it_is = "\
on:
  push:
    tags:
      - 'v*.*.*'
jobs:
  test:
    steps:
      - run: chore lint
      - run: chore test
      - run: chore test:unit
";
    assert_eq!(
        scan_steps(release_yml_as_it_is, false, |script| {
            runs_of_the_chore_task(script, UNIT_TASK)
        }),
        vec![format!("chore {UNIT_TASK}")],
        "release.yml's tag-triggered run IS the same task -- the parser counts it, and one \
         of the two reasons it does not satisfy the guard is that the guard never opens \
         that file"
    );
    assert!(
        gating_runs_of_the_unit_task(release_yml_as_it_is).is_empty(),
        "and the other: a workflow triggered by a tag push gates no pull request, so even \
         read from the right file it would establish nothing about a merge"
    );
}

/// Every cargo test run in `chores.yml` pins `--locked`.
///
/// `--locked` makes the build fail rather than silently resolve a
/// drifting `Cargo.lock` past, which is what keeps the gate and a
/// developer's machine checking the same dependency versions -- and it
/// is what the release process relies on. `ci.yml`'s release run was
/// the one `cargo test` in this constellation that did not pin it, and
/// this guard exists so that does not come back.
///
/// IT READS THE MANIFEST NOW, because that is where the runs went. The
/// workflow calls `chore <task>` and nothing else, so a scan of
/// `ci.yml` would find no cargo invocation at all and pass while
/// asserting nothing -- the failure mode this file is named for.
/// Reading `chores.yml` instead gains something the old scoping could
/// not have: these are the same commands a developer runs locally, so
/// pinning them here pins both.
///
/// The runs are read from the parsed manifest, every command of every
/// task, through the same wrapper chain the rest of this file walks
/// ([`cargo_test_arguments`]) -- so `scripts/test.sh --locked` is
/// recognised as the cargo invocation it ends in, while `cargo build`
/// and `vm.sh guest-test` are not test runs and are not asked.
#[test]
fn the_gates_own_cargo_test_runs_pin_locked() {
    let path = chores_yml();
    let manifest = read_or_panic(&path);

    let runs = cargo_test_runs_outside(&manifest, LOCKED_EXEMPT_TASKS);

    // Non-emptiness first: `all()` over nothing is true, and a rewritten
    // manifest with no cargo test runs outside the exempt tasks would
    // satisfy the loop below while establishing nothing at all.
    assert!(
        !runs.is_empty(),
        "{} has no cargo test run outside {LOCKED_EXEMPT_TASKS:?}, so this guard is \
         asserting nothing. Re-read it before changing how the tiers invoke cargo.",
        path.display()
    );

    for run in &runs {
        let arguments = cargo_test_arguments(run).unwrap_or_default();
        assert!(
            arguments.iter().any(|a| a == "--locked"),
            "{}: `{run}` does not pin `--locked`, so a drifting Cargo.lock is resolved \
             past instead of failing the gate.",
            path.display()
        );
    }
}

/// Tasks whose cargo test runs need not pin `--locked`. EMPTY, and the
/// migration is what emptied it; see
/// [`the_gates_own_cargo_test_runs_pin_locked`].
///
/// It named `kernel-gate`, the workflow job that invoked thirty-odd
/// suites by hand from shell bodies, several of them inside loops:
/// pinning those was a larger change than the one that introduced this
/// guard, so they were exempted by job. That job no longer exists, and
/// every test run in this repository now reaches cargo through
/// `scripts/test.sh`, which every tier calls with `--locked`. There is
/// nothing left to exempt.
///
/// The one test run this guard does not reach is `test:vm`'s, and it is
/// deliberately NOT listed: its command is `vm.sh guest-test`, the
/// harness sibling's, whose cargo invocation lives in that repository's
/// guest script and not in this file at all. Naming it here would make
/// the list look load-bearing when nothing would ever consult it.
const LOCKED_EXEMPT_TASKS: &[&str] = &[];

/// Every cargo test command in `manifest`'s tasks, except in the tasks
/// named in `exempt`: each non-comment line of each command, with any
/// trailing ` #` comment cut off.
fn cargo_test_runs_outside(manifest: &str, exempt: &[&str]) -> Vec<String> {
    task_bodies(manifest)
        .into_iter()
        .filter(|(id, _)| !exempt.contains(&id.as_str()))
        .flat_map(|(_, commands)| commands)
        .flat_map(|command| logical_lines(&command))
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(|line| {
            let line = line.split(" #").next().unwrap_or(&line).to_string();
            shell_commands_of(&line)
        })
        .filter(|command| cargo_test_arguments(command).is_some())
        .collect()
}

/// A command's lines with backslash continuations joined, so
/// `../rust-fs-core/scripts/tier.sh ... \` followed by what it wraps is one command
/// (Greptile on #159). `chores.yml` writes the `test:vm` tier that way.
fn logical_lines(run: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = String::new();
    for line in run.lines() {
        if let Some(head) = line.trim_end().strip_suffix('\\') {
            pending.push_str(head);
            pending.push(' ');
        } else {
            pending.push_str(line);
            out.push(std::mem::take(&mut pending));
        }
    }
    if !pending.is_empty() {
        out.push(pending);
    }
    out
}

/// The commands on one line, split at `&&`, `||`, `;` and `|` outside
/// quotes, so `cargo test --lib && cargo test --locked --release` is two
/// commands and the first one's missing `--locked` is seen (Greptile on
/// #159).
fn shell_commands_of(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), _) if c == q => {
                quote = None;
                current.push(c);
            }
            (Some(_), _) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                current.push(c);
            }
            (None, ';') => out.push(std::mem::take(&mut current)),
            (None, '&' | '|') if chars.peek() == Some(&c) => {
                chars.next();
                out.push(std::mem::take(&mut current));
            }
            (None, '|') => out.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    out.push(current);
    out.into_iter()
        .map(|command| command.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|command| !command.is_empty())
        .collect()
}

/// The pieces of the `--locked` guard, against manifests whose answers
/// are known.
mod locked {
    use super::{cargo_test_runs_outside, task_bodies, LOCKED_EXEMPT_TASKS};

    const MANIFEST: &str = "\
tasks:
  test:unit:
    cmds:
      - '../rust-fs-core/scripts/tier.sh test:unit unit 400 21000 -- scripts/test.sh --locked --lib'
  test:images:
    cmds:
      - cmd: 'scripts/test.sh --locked --release --test fixtures_present'
";

    const UNIT_CMD: &str =
        "      - '../rust-fs-core/scripts/tier.sh test:unit unit 400 21000 -- scripts/test.sh --locked --lib'\n";

    #[test]
    fn the_control_finds_both_runs_through_their_wrappers() {
        assert_eq!(
            cargo_test_runs_outside(MANIFEST, LOCKED_EXEMPT_TASKS),
            vec![
                "../rust-fs-core/scripts/tier.sh test:unit unit 400 21000 -- scripts/test.sh --locked --lib"
                    .to_string(),
                "scripts/test.sh --locked --release --test fixtures_present".to_string(),
            ],
            "a `cmds:` entry is a string or a `cmd:` mapping, and both are commands"
        );
    }

    /// THE HOLE (#123), in its new home. A cargo invocation inside a
    /// block scalar is on a line of its own, which a scan of the
    /// single-line `cmds:` entries would never collect.
    ///
    /// The folding style gets a one-line body on purpose. `>` joins the
    /// block's lines into one, so a two-line script cannot be written in
    /// it at all -- `set -e` and the command become a single command
    /// named `set`, which no shell runs as a test. Asserting that such a
    /// thing is collected would be asserting about a command that cannot
    /// exist; what is worth pinning for `>` is that the entry's value is
    /// still read from the line below it.
    #[test]
    fn a_run_inside_a_block_scalar_is_checked() {
        for (style, body) in [
            (
                "|",
                "          set -e\n          scripts/test.sh --lib  # unpinned\n",
            ),
            (
                "|-",
                "          set -e\n          scripts/test.sh --lib  # unpinned\n",
            ),
            (">", "          scripts/test.sh --lib  # unpinned\n"),
        ] {
            let yaml = MANIFEST.replace(UNIT_CMD, &format!("      - {style}\n{body}"));
            assert_ne!(yaml, MANIFEST, "the mutation must actually apply");
            let runs = cargo_test_runs_outside(&yaml, LOCKED_EXEMPT_TASKS);
            assert!(
                runs.iter()
                    .any(|r| r.contains("scripts/test.sh --lib") && !r.contains("--locked")),
                "cmds: {style}: the unpinned run inside the block was not collected: {runs:?}"
            );
        }
    }

    /// The exemption is by task, so the same unpinned run in any other
    /// task is still checked, whatever its entry looks like.
    #[test]
    fn the_exemption_covers_only_the_named_task() {
        let yaml = MANIFEST.replace(UNIT_CMD, "      - 'scripts/test.sh --lib'\n");
        assert_ne!(yaml, MANIFEST, "the mutation must actually apply");
        assert!(
            cargo_test_runs_outside(&yaml, &["test:unit"])
                .iter()
                .all(|r| r != "scripts/test.sh --lib"),
            "the exempt task's unpinned run is not collected"
        );
        assert!(
            cargo_test_runs_outside(&yaml, &["test:images"])
                .iter()
                .any(|r| r == "scripts/test.sh --lib"),
            "exempting a different task leaves it collected"
        );
    }

    /// A LINE IS NOT A COMMAND (Greptile on #159). Two runs on one line
    /// are checked one at a time, so the unpinned one is found; and a
    /// run continued onto the next line with a backslash is one run, so
    /// its `--locked` counts. `chores.yml` writes the `test:vm` tier
    /// with exactly that continuation.
    #[test]
    fn runs_are_split_at_separators_and_joined_across_continuations() {
        let yaml = MANIFEST.replace(
            UNIT_CMD,
            "      - |\n          scripts/test.sh --lib && cargo test --locked --release\n          ../rust-fs-core/scripts/tier.sh t u 1 1 -- \\\n            scripts/test.sh --locked --lib\n",
        );
        assert_ne!(yaml, MANIFEST, "the mutation must actually apply");
        let runs = cargo_test_runs_outside(&yaml, LOCKED_EXEMPT_TASKS);
        assert_eq!(
            runs,
            vec![
                "scripts/test.sh --lib".to_string(),
                "cargo test --locked --release".to_string(),
                "../rust-fs-core/scripts/tier.sh t u 1 1 -- scripts/test.sh --locked --lib"
                    .to_string(),
                "scripts/test.sh --locked --release --test fixtures_present".to_string(),
            ],
            "each run on its own, the continued one whole"
        );
        assert!(
            runs.iter().any(|r| !cargo_test_arguments_contain_locked(r)),
            "the unpinned first run must be visible to the guard"
        );
    }

    fn cargo_test_arguments_contain_locked(run: &str) -> bool {
        super::cargo_test_arguments(run)
            .unwrap_or_default()
            .iter()
            .any(|a| a == "--locked")
    }

    /// A commented-out command is not a run.
    #[test]
    fn a_comment_inside_a_cmds_block_is_not_a_run() {
        let yaml = MANIFEST.replace(
            UNIT_CMD,
            "      - |\n          # scripts/test.sh --lib\n          scripts/test.sh --locked --lib\n",
        );
        assert_eq!(
            cargo_test_runs_outside(&yaml, LOCKED_EXEMPT_TASKS),
            vec![
                "scripts/test.sh --locked --lib".to_string(),
                "scripts/test.sh --locked --release --test fixtures_present".to_string(),
            ]
        );
    }

    /// A manifest the parser cannot read is a failure, never a pass --
    /// the same direction as the workflow parser's, in the other file
    /// this module reads.
    #[test]
    #[should_panic(expected = "not valid YAML")]
    fn a_manifest_that_does_not_parse_is_a_failure() {
        task_bodies("tasks:\n  test:\n   - broken: [unclosed\n");
    }

    /// THE LIST IS NOT STALE, in both directions it can be.
    ///
    /// A renamed task would leave an exemption naming nothing, and that
    /// task's unpinned runs would then fail the guard with no pointer to
    /// why. And an exemption is a hole in the `--locked` guard, so the
    /// list being empty is asserted rather than assumed: with nothing in
    /// it the loop below examines nothing, and a test that passes by
    /// looking at nothing is this repository's own named defect.
    #[test]
    fn every_exempt_task_exists_in_chores_yml() {
        let manifest = super::read_or_panic(&super::chores_yml());
        let ids: Vec<String> = task_bodies(&manifest)
            .into_iter()
            .map(|(id, _)| id)
            .collect();

        // The control: the lookup must be reading the real manifest, or
        // both assertions below are about nothing.
        assert!(
            ids.iter().any(|id| id == super::UNIT_TASK),
            "chores.yml's tasks did not parse into anything this guard recognises: {ids:?}"
        );
        for exempt in LOCKED_EXEMPT_TASKS {
            assert!(
                ids.iter().any(|id| id == exempt),
                "LOCKED_EXEMPT_TASKS names `{exempt}`, which chores.yml does not have: {ids:?}"
            );
        }
        assert!(
            LOCKED_EXEMPT_TASKS.is_empty(),
            "LOCKED_EXEMPT_TASKS has grown to {LOCKED_EXEMPT_TASKS:?}. Every test run in \
             this repository reaches cargo through scripts/test.sh, which every tier calls \
             with --locked, so an exemption is a hole rather than a convenience: write its \
             justification on the constant, and change this assertion with it."
        );
    }
}

/// The full dotted paths that switch overflow checks off for the
/// profile `cargo test` builds.
///
/// # This compares a whole path, because a key is not a word
///
/// The first version of this scan tracked the `[section]` and compared
/// the key to the literal `"overflow-checks"`. That reads correctly and
/// is defeated by ordinary TOML, because the same setting has several
/// spellings and cargo honours all of them without a warning. Measured
/// on a sibling repository with a runtime `u64::MAX + 1` unit test as
/// the probe -- `cargo test --locked --lib` EXIT=101 means the checks
/// are on, EXIT=0 means they are off, and `cargo metadata --no-deps`
/// was EXIT=0 for every one:
///
/// ```text
///   (nothing)                                          EXIT=101  on
///   [profile.test]  overflow-checks = false            EXIT=0    off
///   [profile.test]  "overflow-checks" = false          EXIT=0    off
///   [profile.test]  'overflow-checks' = false          EXIT=0    off
///   [profile]       test.overflow-checks = false       EXIT=0    off
/// ```
///
/// A bare key, a basic string, a literal string, and a dotted key that
/// puts the profile name on the key side where a section-matching scan
/// never looks. Four of those five defeated the first version, and each
/// leaves the debug tier present, running, green and blind -- the exact
/// state the guard exists to refuse.
///
/// So the section and the key are joined into one path and normalised
/// per segment, and the comparison is against the whole thing. That
/// covers the spellings above, a quoted *section* (`["profile"."test"]`),
/// and a fully top-level dotted key with no section at all.
///
/// Only `profile.dev` and `profile.test` count. `cargo test` builds the
/// `test` profile, which inherits from `dev`, so either can disable the
/// checks in one line. `profile.release` is deliberately absent: the
/// checks are off there by default, that is what ships, and the release
/// run exists to test what ships.
fn profiles_disabling_overflow_checks(manifest: &str) -> Vec<String> {
    /// Split a dotted TOML path and strip each segment's quoting, so
    /// that `"profile" . 'test'` and `profile.test` are one path.
    fn normalise(path: &str) -> String {
        path.split('.')
            .map(|segment| {
                segment
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'')
                    .trim()
            })
            .collect::<Vec<_>>()
            .join(".")
    }

    const DISABLED: [&str; 2] = [
        "profile.dev.overflow-checks",
        "profile.test.overflow-checks",
    ];

    let mut section = String::new();
    let mut found = Vec::new();
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or(raw).trim();
        if line.starts_with('[') {
            section = normalise(line.trim_matches(|c| c == '[' || c == ']'));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if value.trim() != "false" {
            continue;
        }
        let key = normalise(key);
        let path = if section.is_empty() {
            key
        } else {
            format!("{section}.{key}")
        };
        if DISABLED.contains(&path.as_str()) {
            found.push(path);
        }
    }
    found
}

/// The half of the property the workflow and task scans cannot see.
///
/// A debug tier only buys anything while the profile it builds actually
/// checks. One line -- `overflow-checks = false` under `[profile.test]`,
/// or under this repository's existing `[profile.dev]`, a plausible way
/// to make a slow suite faster -- would leave that tier present,
/// running, green, and no longer able to observe an overflow, with every
/// assertion above still passing. A guard for half a condition is the
/// defect it was written to prevent.
///
/// The runtime probe in `src/lib.rs` would also catch this. This scan
/// is kept as defence in depth: it fails earlier in the gate and names
/// the offending manifest key, which is a better diagnostic than "the
/// build did not trap".
#[test]
fn the_profile_that_cargo_test_builds_still_checks_for_overflow() {
    let path = manifest_dir().join("Cargo.toml");
    let manifest = read_or_panic(&path);

    let disabled = profiles_disabling_overflow_checks(&manifest);
    assert!(
        disabled.is_empty(),
        "{} sets `overflow-checks = false` under {disabled:?}. `cargo test` \
         builds the `test` profile, which inherits from `dev`, so this \
         switches off the check the unit tier exists to run -- leaving that \
         tier present, green, and blind. Put it back, or the debug tier is \
         costing a compile and buying nothing.",
        path.display()
    );
}

/// The shell scanner is the part of this that can rot, so it is checked
/// against each shape it has to tell apart.
///
/// Its argument is the shell text of one task's commands, not YAML.
/// What used to be tested here as YAML -- a command quoted in a `#`
/// line -- moved to `gating`, because the parser now answers it by
/// construction and this function never sees it.
mod shell_scan {
    use super::runs_covering_the_library_unit_tests;

    /// THE SHAPE THE TIER ACTUALLY HAS, and the wrappers it is written
    /// through. A scanner that knew only the words `cargo test` would
    /// find nothing in this line, and the guard would then refuse a
    /// correct tree -- the fastest way to get a guard deleted.
    #[test]
    fn the_tier_shape_this_repository_uses_counts() {
        let tier =
            "EXPECT_OVERFLOW_CHECKS=1 ../rust-fs-core/scripts/tier.sh test:unit unit 400 21000 -- \
                    scripts/test.sh --locked $(scripts/test-targets.sh unit)\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(tier).len(),
            1,
            "tier.sh runs what follows its `--`, and test.sh ends in `cargo test --features cli \"$@\"`; \
             the arguments that reach cargo are what decides the profile"
        );
    }

    /// The same chain carrying `--release`, which is what the other
    /// tiers are and what must never satisfy this guard.
    #[test]
    fn the_same_chain_with_release_does_not_count() {
        let tier = "../rust-fs-core/scripts/tier.sh test:images images 760 36000 -- \
                    scripts/test.sh --locked --release $(scripts/test-targets.sh images)\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(tier),
            Vec::<String>::new(),
            "overflow checks are off in release; a release tier proves nothing about them"
        );
    }

    /// A tier that wraps something else entirely. `test:vm` hands the
    /// whole suite to the harness, whose cargo invocation is in the
    /// sibling repository -- not a cargo test run this file can read the
    /// arguments of, and so not this repository's debug run.
    #[test]
    fn a_tier_wrapping_something_that_is_not_cargo_does_not_count() {
        let tier = "../rust-fs-core/scripts/tier.sh test:vm vm 2860 136000 -- \
                    ../fs-linux-test-harness/scripts/vm.sh guest-test\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(tier),
            Vec::<String>::new(),
            "vm.sh guest-test is not a cargo test whose profile this scan can see"
        );
    }

    /// The trap this repository actually contains, in the form that
    /// still reaches this function. `chores.yml` documents the tier by
    /// quoting what it does, and a block-scalar command can carry the
    /// same habit in shell comments, where the text survives the
    /// command's deletion.
    #[test]
    fn a_debug_run_quoted_in_a_shell_comment_does_not_count() {
        let block = "\
set -euo pipefail
# Measured on this branch:
#     scripts/test.sh --locked --release --lib   ->  EXIT=0
#     EXPECT_OVERFLOW_CHECKS=1 scripts/test.sh --locked --lib   ->  EXIT=101
cargo test --locked --release
";
        assert_eq!(
            runs_covering_the_library_unit_tests(block),
            Vec::<String>::new(),
            "a debug command quoted inside a comment is documentation, not a run"
        );
    }

    /// A COMMAND THAT IS ONLY MENTIONED IS NOT RUN (#118). Echoed, it
    /// is text; indented inside a branch that never fires, it is never
    /// reached. Each must leave the guard with nothing to count.
    #[test]
    fn a_debug_run_that_is_echoed_or_never_reached_does_not_count() {
        for block in [
            "echo \"EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib\"\n",
            "if false; then\n  EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib\nfi\n",
            "true && EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib\n",
            "echo \"EXPECT_OVERFLOW_CHECKS=1 scripts/test.sh --locked --lib\"\n",
        ] {
            assert_eq!(
                runs_covering_the_library_unit_tests(block),
                Vec::<String>::new(),
                "{block:?} runs no debug cargo test"
            );
        }
    }

    /// The control for the rule above: assignments before the command
    /// are still the command.
    #[test]
    fn assignments_before_the_command_are_still_the_command() {
        for line in [
            "cargo test --locked --lib",
            "EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib",
            "A=1 B_2= cargo test --locked --lib",
            "EXPECT_OVERFLOW_CHECKS=1 scripts/test.sh --locked --lib",
        ] {
            assert_eq!(
                runs_covering_the_library_unit_tests(line),
                vec![line.to_string()],
                "{line} is a debug run"
            );
        }
        assert_eq!(
            runs_covering_the_library_unit_tests("1A=x cargo test --locked --lib"),
            Vec::<String>::new(),
            "`1A=x` is not an assignment, so the line is a command named 1A=x"
        );
    }

    /// `-r` IS `--release`, IN EVERY SPELLING CLAP ACCEPTS (#136), and the
    /// profile is read from the run's own words. Each line in the first list
    /// builds the release profile; each in the second is a debug library run
    /// whose `r`, or whose release run, belongs to something else.
    #[test]
    fn the_short_release_flag_is_read_from_the_runs_own_arguments() {
        for line in [
            "cargo test --locked -r --lib",
            "cargo test --locked -qr --lib",
            "cargo test --locked -rq --lib",
            "cargo test --locked -j4 -r --lib",
            "cargo test --locked -j 4 -r --lib",
            "cargo test --locked --features x -r",
            "cargo test --locked --features 'a;b' -r",
            "cargo test --locked --target-dir \"build;\" -r",
            "cargo test --locked '-r'",
            "cargo test --locked --profile=release --lib",
            "RUSTFLAGS=-Dwarnings cargo test --locked -r --lib",
            "scripts/test.sh --locked -r --lib",
        ] {
            assert_eq!(
                runs_covering_the_library_unit_tests(&format!("{line}\n")),
                Vec::<String>::new(),
                "{line} builds the release profile"
            );
        }
        for line in [
            "cargo test --locked --lib -- -r",
            "cargo test --locked --features r --lib",
            "cargo test --locked -F r --lib",
            "cargo test --locked -pr --lib",
            "cargo test --locked --lib && rm -rf build",
            "cargo test --locked --lib&&rm -rf build",
            "cargo test --locked --lib; echo -r",
            "cargo test --locked --lib && cargo test --locked -r",
            "cargo test --locked --lib && cargo test --locked --release",
        ] {
            assert_eq!(
                runs_covering_the_library_unit_tests(&format!("{line}\n")).len(),
                1,
                "{line}: the leading run is a debug library run"
            );
        }
    }

    #[test]
    fn a_real_debug_run_counts() {
        let block = "\
cargo test --locked --release
EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib
";
        assert_eq!(
            runs_covering_the_library_unit_tests(block),
            vec!["EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib".to_string()],
        );
    }

    /// A command that is `--release` but which carries a trailing
    /// comment mentioning the debug run.
    #[test]
    fn a_trailing_comment_does_not_promote_a_release_run() {
        let inline = "cargo test --locked --release  # not cargo test --lib\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(inline),
            Vec::<String>::new(),
            "the command is --release; the comment after it is not a second run"
        );
    }

    /// The inline-comment strip, which nothing else here pins. A real
    /// debug run whose trailing comment happens to contain `--release`
    /// must still be counted. Without the strip that word disqualifies
    /// the command, and the guard then fails insisting there is no
    /// debug run while one is sitting in front of it.
    #[test]
    fn a_trailing_comment_naming_release_does_not_disqualify_a_debug_run() {
        let line = "cargo test --locked --lib  # deliberately not --release\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(line),
            vec!["cargo test --locked --lib".to_string()],
            "the command is a debug run; --release appears only in its comment"
        );
    }

    /// The ways a run can carry no `--release` and still be built
    /// without the checks.
    #[test]
    fn a_profile_named_another_way_does_not_count() {
        let lines = [
            "cargo test --locked --profile release-with-debug --lib",
            "CARGO_PROFILE_TEST_OVERFLOW_CHECKS=false cargo test --locked --lib",
            "CARGO_PROFILE_DEV_OVERFLOW_CHECKS=false cargo test --locked --lib",
            "CARGO_PROFILE_TEST_OVERFLOW_CHECKS=false scripts/test.sh --locked --lib",
        ];
        for line in lines {
            assert_eq!(
                runs_covering_the_library_unit_tests(line),
                Vec::<String>::new(),
                "{line} does not compile the overflow checks"
            );
        }
        assert_eq!(
            lines.len(),
            4,
            "the loop above must have examined every shape"
        );
    }

    /// A single integration target is not the library, whatever else
    /// the line carries -- including the handshake, which would
    /// otherwise look like the property being asserted.
    #[test]
    fn a_single_integration_target_is_not_the_library() {
        let lines = [
            "cargo test --test csum_oracle -- --nocapture",
            "cargo test --locked --test transaction_oracle",
            "EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --test capi",
            "EXPECT_OVERFLOW_CHECKS=1 scripts/test.sh --locked --test=capi",
        ];
        for line in lines {
            assert_eq!(
                runs_covering_the_library_unit_tests(line),
                Vec::<String>::new(),
                "{line} builds one integration target and no library unit tests"
            );
        }
        assert_eq!(
            lines.len(),
            4,
            "the loop above must have examined every shape"
        );
    }

    /// THE TIER NAME IS PART OF THE TARGET LIST. A run whose targets
    /// come from a substitution is covered exactly when that
    /// substitution is the `unit` tier; the others print `--test <name>`
    /// per file, which is the test above with the flags out of sight.
    #[test]
    fn the_tier_a_target_list_comes_from_decides_whether_it_counts() {
        for tier in ["images", "oracle", "kernel"] {
            let line = format!("scripts/test.sh --locked $(scripts/test-targets.sh {tier})\n");
            assert_eq!(
                runs_covering_the_library_unit_tests(&line),
                Vec::<String>::new(),
                "the {tier} tier's target list is `--test <name>` per file and builds no \
                 library unit test"
            );
        }
        let unit = "scripts/test.sh --locked $(scripts/test-targets.sh unit)\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(unit).len(),
            1,
            "the unit tier's list begins `--lib`, which the guard proves by running the \
             script rather than by trusting this"
        );

        // An explicit library flag settles it whatever list is beside
        // it: cargo builds the library unit tests as well as the named
        // targets, and refusing that would be the guard turning down a
        // run that genuinely satisfies it.
        let both = "scripts/test.sh --locked --lib $(scripts/test-targets.sh oracle)\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(both).len(),
            1,
            "`--lib` beside another list still builds the library unit tests"
        );
    }

    /// `--tests` and `--all-targets` build the library unit tests, so
    /// neither may be mistaken for a `--test <name>` run.
    ///
    /// THIS REPLACES A TEST THAT COULD NOT FAIL, and then the mechanism
    /// that one pinned. The first version asserted `--all-targets` was
    /// not excluded by a `--test` substring search -- and `--all-targets`
    /// does not contain `--test` in any spelling, so the assertion held
    /// however the search was written. The second pinned the trailing
    /// space in `"--test "`, which was what kept `--tests` from being
    /// swallowed. The exclusion compares whole arguments now, so there
    /// is no space to lose and nothing mechanical left to pin; what is
    /// worth asserting is the behaviour itself, which is what this does.
    #[test]
    fn the_flags_that_build_the_library_are_not_single_target_runs() {
        for line in [
            "EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --tests",
            "EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --all-targets",
            "EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --tests --test capi",
        ] {
            assert_eq!(
                runs_covering_the_library_unit_tests(&format!("{line}\n")),
                vec![line.to_string()],
                "{line} builds the library unit tests; excluding it would refuse a run \
                 that genuinely satisfies this guard"
            );
        }
    }

    /// `cargo build` is not `cargo test`.
    #[test]
    fn a_cargo_build_step_is_not_a_test_run() {
        let build_only = "cargo build --locked --release\n";
        assert_eq!(
            runs_covering_the_library_unit_tests(build_only),
            Vec::<String>::new(),
            "building is not running a test suite"
        );
    }
}

/// The handshake half of the shell scanner.
mod handshake {
    use super::debug_runs_that_prove_the_build_traps;

    #[test]
    fn a_debug_run_carrying_the_handshake_counts() {
        let script = "EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib\n";
        assert_eq!(
            debug_runs_that_prove_the_build_traps(script),
            vec!["EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib".to_string()],
        );
    }

    /// A debug run that exists and asks the build nothing. Buys a
    /// compile and no information.
    #[test]
    fn a_debug_run_without_the_handshake_does_not_count() {
        let script = "cargo test --locked --lib\n";
        assert_eq!(
            debug_runs_that_prove_the_build_traps(script),
            Vec::<String>::new(),
            "the tier is there but nothing checks the build it produced"
        );
    }

    /// AN EMPTY VALUE IS NOT THE HANDSHAKE, because `src/lib.rs` reads
    /// it that way -- `Ok(value) if !value.is_empty()`, and anything
    /// else returns without asserting. A tier spelled this way would
    /// look armed in the manifest and prove nothing at runtime.
    #[test]
    fn an_empty_handshake_value_does_not_count() {
        let script = "EXPECT_OVERFLOW_CHECKS= cargo test --locked --lib\n";
        assert_eq!(
            debug_runs_that_prove_the_build_traps(script),
            Vec::<String>::new(),
        );
    }

    /// The variable must reach the command, not merely appear in it. An
    /// assignment written after the command name is an argument -- here
    /// a test-name filter -- and the environment the build sees is
    /// unchanged.
    #[test]
    fn the_variable_must_be_set_on_the_command() {
        let script = "cargo test --locked --lib EXPECT_OVERFLOW_CHECKS=1\n";
        assert_eq!(
            debug_runs_that_prove_the_build_traps(script),
            Vec::<String>::new(),
        );
    }

    /// A handshake on a release run proves nothing and must not satisfy
    /// this: the checks are off in release on purpose.
    #[test]
    fn the_handshake_on_a_release_run_does_not_count() {
        let script = "EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --release\n";
        assert_eq!(
            debug_runs_that_prove_the_build_traps(script),
            Vec::<String>::new(),
        );
    }

    /// And quoted inside a shell comment, which is where a block-scalar
    /// command would explain it.
    #[test]
    fn the_handshake_quoted_in_a_comment_does_not_count() {
        let script = "#     EXPECT_OVERFLOW_CHECKS=1 cargo test --locked --lib\n";
        assert_eq!(
            debug_runs_that_prove_the_build_traps(script),
            Vec::<String>::new(),
        );
    }
}

/// The manifest scanner, held to the shapes it has to tell apart. These
/// do not depend on this repository's own `Cargo.toml`, so they keep
/// meaning something after it changes.
mod manifest_parser {
    use super::profiles_disabling_overflow_checks;

    #[test]
    fn the_test_profile_disabling_the_checks_is_caught() {
        let manifest = "\
[profile.release]
lto = true

[profile.test]
overflow-checks = false
";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.test.overflow-checks".to_string()],
        );
    }

    /// This repository has an explicit `[profile.dev]`, so this is the
    /// likeliest place the setting would actually arrive.
    #[test]
    fn the_dev_profile_disabling_the_checks_is_caught() {
        let manifest = "[profile.dev]\nopt-level = 1\noverflow-checks   =   false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.dev.overflow-checks".to_string()],
        );
    }

    /// Release is expected to have them off. Flagging it would make the
    /// guard fail on every correct manifest, which is the fastest way
    /// to get a guard deleted.
    #[test]
    fn the_release_profile_disabling_the_checks_is_not_flagged() {
        let manifest = "[profile.release]\noverflow-checks = false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            Vec::<String>::new(),
        );
    }

    /// A commented-out line is not a setting -- the same trap as the
    /// workflow parser's, in the other file this module reads.
    #[test]
    fn a_commented_out_setting_is_not_a_setting() {
        let manifest = "[profile.test]\n# overflow-checks = false\nopt-level = 1\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            Vec::<String>::new(),
        );
    }

    /// The comment strip, which nothing else here pins. The realistic
    /// way this setting arrives is with its excuse on the same line,
    /// and it must still be caught: unstripped, the value reads
    /// `false  # speeds the suite up`, which is not `false`, and the
    /// guard waves through the exact edit it exists to catch.
    #[test]
    fn a_disabling_line_with_a_trailing_comment_is_still_caught() {
        let manifest = "[profile.test]\noverflow-checks = false  # speeds the suite up\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.test.overflow-checks".to_string()],
        );
    }

    /// A different setting being `false` is not this setting being
    /// `false`. Without this the scanner could be keying on the value
    /// alone -- flagging any `= false` under those two sections -- and
    /// every other test here would still pass.
    #[test]
    fn another_setting_being_false_is_not_this_one() {
        let manifest = "[profile.test]\ndebug-assertions = false\nopt-level = 1\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            Vec::<String>::new(),
        );
    }

    /// THE SPELLINGS THAT DEFEATED THE FIRST VERSION. Each of these was
    /// measured to genuinely switch the checks off, with no warning
    /// from cargo -- see the table on
    /// `profiles_disabling_overflow_checks`. A guard that reads one
    /// spelling of a setting is a guard against typing it one way.
    #[test]
    fn a_double_quoted_key_is_the_same_key() {
        let manifest = "[profile.test]\n\"overflow-checks\" = false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.test.overflow-checks".to_string()],
        );
    }

    #[test]
    fn a_literal_quoted_key_is_the_same_key() {
        let manifest = "[profile.dev]\n'overflow-checks' = false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.dev.overflow-checks".to_string()],
        );
    }

    /// The one a section-matching scan cannot see at all: the profile
    /// name is on the key side, so the section is only `profile`.
    #[test]
    fn a_dotted_key_putting_the_profile_on_the_key_side_is_caught() {
        let manifest = "[profile]\ntest.overflow-checks = false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.test.overflow-checks".to_string()],
        );
    }

    /// And with no section header at all, which is still valid TOML.
    #[test]
    fn a_top_level_dotted_key_is_caught() {
        let manifest = "profile.test.overflow-checks = false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.test.overflow-checks".to_string()],
        );
    }

    #[test]
    fn a_quoted_section_is_the_same_section() {
        let manifest = "[\"profile\".'test']\noverflow-checks = false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            vec!["profile.test.overflow-checks".to_string()],
        );
    }

    /// Release stays exempt in the dotted spelling too, or normalising
    /// the path would have quietly widened what the guard refuses.
    #[test]
    fn the_release_profile_is_exempt_in_the_dotted_spelling_too() {
        let manifest = "[profile]\nrelease.overflow-checks = false\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            Vec::<String>::new(),
        );
    }

    /// `true` is the state we want and must not be reported as the
    /// state we do not. Without this the scanner could be keying on the
    /// word `overflow-checks` alone and nothing here would notice.
    #[test]
    fn enabling_the_checks_explicitly_is_not_flagged() {
        let manifest = "[profile.test]\noverflow-checks = true\n";
        assert_eq!(
            profiles_disabling_overflow_checks(manifest),
            Vec::<String>::new(),
        );
    }
}

/// WHAT ELSE DECIDES WHETHER THE STEP GATES -- one test per item on the
/// enumerated list, because each is a separate way for the gate to go
/// blind with the command still present and still matching.
///
/// The version of this guard these replace matched the `- run:` line in
/// isolation. Measured against this repository's own workflow, `if:
/// false` and `continue-on-error: true` each left all 31 of its tests
/// green while the gate stopped gating.
///
/// The step they are asserted against is `run: chore test:unit`, which
/// is what `ci.yml` now contains: the workflow's half of the chain is
/// the task being invoked, so that is the thing whose gating has to be
/// established.
mod gating {
    use super::gating_runs_of_the_unit_task;

    /// The shape that does gate, as a control. Every test below is this
    /// with one thing added, so a failure here would mean the fixture
    /// is wrong rather than the property.
    const GATING: &str = "\
on:
  pull_request:
    branches: [main]
jobs:
  unit:
    steps:
      - run: chore test:unit
";

    const STEP: &str = "      - run: chore test:unit\n";

    #[test]
    fn the_control_shape_gates() {
        assert_eq!(
            gating_runs_of_the_unit_task(GATING).len(),
            1,
            "the control must be counted, or every test below passes for the wrong reason"
        );
    }

    /// THE MESSAGE NAMES THE CAUSE (#124). A workflow that stopped
    /// triggering on pull requests is reported as that, with the
    /// triggers it has, and not as a missing step. The control is the
    /// gating shape, which has nothing to explain.
    #[test]
    fn a_workflow_off_pull_requests_is_reported_by_its_trigger() {
        assert_eq!(super::not_a_pull_request_gate(GATING), None, "control");
        for (trigger, names_target) in [
            ("pull_request_target", true),
            ("pull_request_review", false),
            ("push", false),
        ] {
            let yaml = GATING.replace("  pull_request:\n", &format!("  {trigger}:\n"));
            assert_ne!(yaml, GATING, "the mutation must actually apply");
            let why = super::not_a_pull_request_gate(&yaml)
                .unwrap_or_else(|| panic!("{trigger}: no reason given"));
            assert!(
                why.contains(&format!("{trigger:?}")) && why.contains("`on:` block"),
                "{trigger}: the message must name the trigger found and the on: block: {why}"
            );
            assert_eq!(
                why.contains("refused on purpose"),
                names_target,
                "{trigger}: only pull_request_target gets the refusal explained: {why}"
            );
        }
    }

    #[test]
    fn a_step_carrying_if_does_not_gate() {
        for condition in [
            "if: false",
            "if: ${{ false }}",
            "if: github.event_name == 'push'",
            "if: ${{ env.SOMETHING == 'yes' }}",
        ] {
            let yaml = GATING.replace(STEP, &format!("{STEP}        {condition}\n"));
            assert!(
                gating_runs_of_the_unit_task(&yaml).is_empty(),
                "a step carrying `{condition}` may or may not run, so it cannot be what \
                 makes the gate able to see an overflow. Rejected on the key's presence \
                 rather than by evaluating it -- the spellings are open-ended."
            );
        }
    }

    #[test]
    fn a_step_carrying_continue_on_error_does_not_gate() {
        let yaml = GATING.replace(STEP, &format!("{STEP}        continue-on-error: true\n"));
        assert!(
            gating_runs_of_the_unit_task(&yaml).is_empty(),
            "the step runs and its failure is discarded, which is the project's own named \
             defect: a step that runs and whose result nothing reads"
        );
    }

    #[test]
    fn a_job_carrying_if_does_not_gate() {
        let yaml = GATING.replace("  unit:\n", "  unit:\n    if: false\n");
        assert!(
            gating_runs_of_the_unit_task(&yaml).is_empty(),
            "the same reasoning one level up: a job that may not run cannot gate"
        );
    }

    #[test]
    fn a_job_carrying_continue_on_error_does_not_gate() {
        let yaml = GATING.replace("  unit:\n", "  unit:\n    continue-on-error: true\n");
        assert!(
            gating_runs_of_the_unit_task(&yaml).is_empty(),
            "a job whose failure is discarded cannot gate, however sound its steps"
        );
    }

    /// The assumption the `ci.yml`-only scan rests on, which is a fact
    /// about the file rather than a given.
    #[test]
    fn a_workflow_that_no_longer_runs_on_pull_request_does_not_gate() {
        let yaml = GATING.replace(
            "  pull_request:\n    branches: [main]\n",
            "  push:\n    branches: [main]\n",
        );
        assert!(
            gating_runs_of_the_unit_task(&yaml).is_empty(),
            "scoping the scan to ci.yml assumes ci.yml is what runs on a pull request; if its \
             triggers stop including pull_request, the step gates nothing no matter how it looks"
        );
    }

    /// A `run: |` block is read whole, so a command below a `set -e`
    /// line is seen rather than lost with the fragment before it.
    #[test]
    fn a_run_block_is_read_whole() {
        let yaml = "\
on:
  pull_request:
    branches: [main]
jobs:
  unit:
    steps:
      - name: a block
        run: |
          set -euo pipefail
          chore test:unit
";
        assert_eq!(
            gating_runs_of_the_unit_task(yaml).len(),
            1,
            "a command inside a `run: |` block must be seen; several of this workflow's \
             steps are written that way"
        );
    }

    /// THE QUOTED SPELLINGS, WHICH WERE SILENT DEFEATS. Measured on
    /// `main` at `57cf1b6`: `if: false` correctly turned the suite red,
    /// and `"if": false` -- the same key, quoted -- left all 34 tests
    /// green while Actions skipped the step. The old parser took its
    /// key as `cur.split(':').next()` with no un-quoting, so the key
    /// read `"if"` and matched no entry in `NON_GATING_KEYS`.
    ///
    /// Nothing un-quotes anything now: the key arrives from the parser
    /// already resolved, so every spelling of it is the same key by
    /// construction.
    #[test]
    fn a_quoted_key_is_the_same_key() {
        for spelling in [
            "\"if\": false",
            "'if': false",
            "\"continue-on-error\": true",
            "'continue-on-error': true",
        ] {
            let yaml = GATING.replace(STEP, &format!("{STEP}        {spelling}\n"));
            assert!(
                gating_runs_of_the_unit_task(&yaml).is_empty(),
                "`{spelling}` is the same key as its bare spelling; quoting it must not \
                 make a skipped step count as the thing gating the merge"
            );
        }
    }

    /// And one level up, on the job.
    #[test]
    fn a_quoted_key_on_the_job_is_the_same_key() {
        for spelling in ["\"if\": false", "\"continue-on-error\": true"] {
            let yaml = GATING.replace("  unit:\n", &format!("  unit:\n    {spelling}\n"));
            assert!(
                gating_runs_of_the_unit_task(&yaml).is_empty(),
                "`{spelling}` on the job is the same key as its bare spelling"
            );
        }
    }

    /// THE COMMENTED-OUT TRIGGER, ALSO A SILENT DEFEAT. The old check
    /// asked whether the `on:` block's raw text -- comments included --
    /// contained the characters `pull_request`, so commenting the
    /// trigger out left the guard green on a workflow that no longer
    /// ran on pull requests at all. Measured on `main`: 34 passed,
    /// both arms.
    #[test]
    fn a_commented_out_pull_request_trigger_does_not_gate() {
        let commented_with_another_trigger_left = GATING.replace(
            "  pull_request:\n    branches: [main]\n",
            "  # pull_request:\n  #   branches: [main]\n  push:\n    branches: [main]\n",
        );
        let only_a_comment_naming_it = GATING.replace(
            "  pull_request:\n    branches: [main]\n",
            "  # pull_request disabled while we investigate flaky runners\n  push:\n    branches: [main]\n",
        );
        for yaml in [
            &commented_with_another_trigger_left,
            &only_a_comment_naming_it,
        ] {
            assert!(
                gating_runs_of_the_unit_task(yaml).is_empty(),
                "a trigger named only in a comment is not a trigger; the parser drops \
                 comments before anything compares a name, so there is no `#` to strip \
                 and none to forget:\n{yaml}"
            );
        }
    }

    /// A whole-name comparison, so a trigger that merely begins with
    /// those characters is a different trigger. `pull_request_review`
    /// fires on a review, not on the pull request, and cannot be what
    /// gates the merge.
    #[test]
    fn a_trigger_that_merely_begins_with_pull_request_does_not_gate() {
        let yaml = GATING.replace(
            "  pull_request:\n    branches: [main]\n",
            "  pull_request_review:\n    types: [submitted]\n",
        );
        assert!(
            gating_runs_of_the_unit_task(&yaml).is_empty(),
            "pull_request_review is not pull_request; a substring match cannot tell \
             them apart and this comparison must"
        );
    }

    /// `pull_request_target` is not `pull_request`, and is refused on
    /// purpose. It runs against the base repository with a write token
    /// and the repository's secrets, and checks out the base ref by
    /// default, so a workflow triggered only that way may never build
    /// the contributor's code. `rust-fs-xfs#146` and
    /// `rust-fs-ext4#149` record it as a live gap in the hand-rolled
    /// guard this file replaces.
    ///
    /// Pinned as a test rather than left to the comparison, because the
    /// clause is one line and was previously written by hand and copied
    /// between repositories. This is what stops it coming back.
    #[test]
    fn pull_request_target_does_not_gate() {
        let yaml = GATING.replace(
            "  pull_request:\n    branches: [main]\n",
            "  pull_request_target:\n    branches: [main]\n",
        );
        assert!(
            gating_runs_of_the_unit_task(&yaml).is_empty(),
            "pull_request_target runs with the base repository's token and secrets \
             and checks out the base ref; it is not proof that the merge is gated"
        );
    }

    /// `on:` may be a sequence of names rather than a mapping, in
    /// either the flow or the block spelling, and all three are
    /// ordinary workflows.
    #[test]
    fn a_sequence_of_triggers_is_read() {
        for spelling in [
            "on: [push, pull_request]\n",
            "on:\n  - push\n  - pull_request\n",
        ] {
            let yaml = GATING.replace("on:\n  pull_request:\n    branches: [main]\n", spelling);
            assert_eq!(
                gating_runs_of_the_unit_task(&yaml).len(),
                1,
                "this workflow triggers on a pull request as surely as the mapping \
                 spelling does:\n{yaml}"
            );
        }
    }

    /// THE ARM THAT WAS A FALSE ALARM RATHER THAN A DEFEAT, AND SO
    /// CANNOT BE WITNESSED BY THE SUITE GOING RED -- it already did.
    /// The witness is that legal YAML now passes.
    ///
    /// The old parser treated only a bare `|` as a block opener
    /// (`after != "|"`), so `|-`, `|+`, `>`, `>-` and `|2` were read as
    /// the command itself and the block's contents never parsed at all.
    /// Measured on `main`: `run: |` 34 passed, `run: |-` and `run: >`
    /// each EXIT=101 with 2 failed -- the guard refusing a completely
    /// correct workflow, which is the fastest way to get a guard
    /// deleted.
    ///
    /// A parser knows all five styles because they are the grammar.
    #[test]
    fn every_block_scalar_style_is_read_whole() {
        for style in ["|", "|-", "|+", ">", ">-", "|2"] {
            let yaml = GATING.replace(
                STEP,
                &format!(
                    "      - name: a block\n        run: {style}\n          chore test:unit\n"
                ),
            );
            assert_eq!(
                gating_runs_of_the_unit_task(&yaml).len(),
                1,
                "`run: {style}` is a legal block scalar carrying the gating command; \
                 failing here is the guard refusing a correct workflow:\n{yaml}"
            );
        }
    }

    /// A command quoted in a YAML comment is not a run. This used to be
    /// the shell scanner's job and is the parser's now: comments do not
    /// survive parsing, so there is no `#` handling here to get wrong.
    /// It is asserted at this level because that is where the property
    /// now lives -- `ci.yml` really does explain the unit job in a
    /// comment block above it, so a scan that missed this would stay
    /// green after the step itself was deleted.
    #[test]
    fn a_debug_run_quoted_in_a_yaml_comment_does_not_gate() {
        let yaml = "\
on:
  pull_request:
    branches: [main]
jobs:
  test:
    steps:
      # Do not remove this as a duplicate of the run below it:
      #     - run: chore test:unit
      - run: chore test:images
";
        assert!(
            gating_runs_of_the_unit_task(yaml).is_empty(),
            "the gating command appears only inside a comment, and the step that \
             remains runs a different tier"
        );
    }

    /// A workflow the parser cannot read is a failure, never a pass.
    /// The direction matters: a guard that swallowed the error and
    /// returned an empty structure would report "no gating job runs
    /// this", which is also a failure and therefore safe -- but one
    /// that returned early with a pass would be the blindness this
    /// whole module exists to refuse.
    #[test]
    #[should_panic(expected = "not valid YAML")]
    fn a_workflow_that_does_not_parse_is_a_failure() {
        super::parse_workflow("jobs:\n  test:\n   - broken: [unclosed\n");
    }

    /// THE CONTROL THAT STOPS THE REFUSAL OVER-CORRECTING.
    ///
    /// `pull_request_target` is refused as insufficient ON ITS OWN.
    /// That is not the same as refusing any workflow that mentions it,
    /// and until this test existed nothing in the file could tell the
    /// two apart: every fixture carried at most one trigger, so this
    /// mutation survived the whole suite --
    ///
    /// ```text
    ///   any(t == "pull_request")
    ///       && !any(t == "pull_request_target")
    /// ```
    ///
    /// -- while refusing a perfectly gated workflow. Carrying both
    /// triggers is the ordinary way to reach repository secrets from a
    /// job without giving up the pull-request gate, and such a workflow
    /// IS gated, by its `pull_request:` key.
    ///
    /// An assertion whose result does not depend on the thing it claims
    /// to check is this project's own recurring defect; this one was in
    /// the test pinning the refusal rather than in the refusal itself.
    #[test]
    fn a_workflow_carrying_both_triggers_still_gates() {
        let yaml = GATING.replace(
            "  pull_request:\n    branches: [main]\n",
            "  pull_request:\n    branches: [main]\n  pull_request_target:\n    branches: [main]\n",
        );
        assert_ne!(yaml, GATING, "the mutation must actually apply");
        assert_eq!(
            gating_runs_of_the_unit_task(&yaml).len(),
            1,
            "the workflow still triggers on pull_request, so it still gates; refusing it \
             because pull_request_target is also present would be the over-correction"
        );
    }

    /// AND A TASK NAME IS A TASK NAME. `chore test` reaches `test:unit`
    /// through `test:native` and `chore test:images` does not reach it
    /// at all; neither is what this guard follows, because the task
    /// whose body it checked is `test:unit`'s. Following chore's task
    /// graph would be a second parser with its own defeats, to
    /// establish what the workflow already states in three words.
    #[test]
    fn another_task_is_not_this_task() {
        for task in ["chore test", "chore test:images", "chore test:vm"] {
            let yaml = GATING.replace(STEP, &format!("      - run: {task}\n"));
            assert_ne!(yaml, GATING, "the mutation must actually apply");
            assert!(
                gating_runs_of_the_unit_task(&yaml).is_empty(),
                "`{task}` is not `chore test:unit`"
            );
        }
    }
}

/// Which `rust-fs-core` this crate is built against, as stated in one file.
///
/// Four spellings, because the pin is a dependency version in a
/// manifest, a resolved version in a lockfile, an environment variable
/// in a workflow, and an argument to `git clone`. All four are returned
/// normalised without the leading `v`, paired with the spelling that
/// produced them so a failure names what to edit.
///
/// A comment is not a declaration. `chores.yml` explains its pin in
/// prose, and a scan that read the prose would report a disagreement
/// nobody can fix.
///
/// A value that is not a literal version is not a declaration either:
/// `release.yml` clones `--branch "$FS_CORE_REF"`, which is the
/// variable declared above it and not a second, independent pin.
fn am_fs_core_versions_declared(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut previous = "";
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if previous == "name = \"rust-fs-core\"" {
            if let Some(v) = quoted_value_after(line, "version") {
                found.push(("the locked rust-fs-core `version`".to_string(), v));
            }
        }
        previous = line;
        if line.contains("rust-fs-core") {
            if let Some(v) = quoted_value_after(line, "version") {
                found.push(("the rust-fs-core dependency's `version`".to_string(), v));
            }
        }
        if let Some(rest) = line.split_once("FS_CORE_REF:") {
            if let Some(v) = as_version(rest.1) {
                found.push(("`FS_CORE_REF`".to_string(), v));
            }
        }
        if let Some(rest) = line.split_once("core-ref:") {
            if let Some(v) = as_version(rest.1) {
                found.push(("the release-cli call's `core-ref`".to_string(), v));
            }
        }
        if line.contains("rust-fs-core") {
            if let Some(rest) = line.split_once("--branch ") {
                if let Some(v) = as_version(rest.1.split_whitespace().next().unwrap_or("")) {
                    found.push(("the `git clone --branch` ref".to_string(), v));
                }
            }
        }
    }
    found
}

/// The value of `key = "..."` on a line, if it is there.
fn quoted_value_after(line: &str, key: &str) -> Option<String> {
    let after = line.split_once(key)?.1;
    let after = after.trim_start().strip_prefix('=')?.trim_start();
    let inner = after.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

/// `v0.2.13`, `"0.2.13"` and `0.2.13` are the same pin; `"$FS_CORE_REF"`
/// and `main` are not pins at all.
fn as_version(raw: &str) -> Option<String> {
    let t = raw.trim().trim_matches(|c| c == '"' || c == '\'');
    let t = t.strip_prefix('v').unwrap_or(t);
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        Some(t.to_string())
    } else {
        None
    }
}

/// Every file that declares one.
///
/// A fixed list rather than a walk: the point is that each of these is
/// known to carry a pin, so one that stops carrying it is a failure
/// rather than a file the walk no longer visits. `fuzz/Cargo.toml` is
/// not here: it reaches `rust-fs-core` only through this crate, so its
/// pin lives in `fuzz/Cargo.lock` alone.
const FILES_THAT_PIN_AM_FS_CORE: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "fuzz/Cargo.lock",
    "chores.yml",
    ".github/workflows/release.yml",
    ".github/workflows/fuzz.yml",
];

/// How many declarations the files above carry between them: one each,
/// and a second in `release.yml`, whose call to rust-fs-core's
/// release-cli workflow passes the core tag as `core-ref` -- a literal,
/// because a reusable workflow's `with:` cannot read `env`.
///
/// Asserted as a number as well as file by file, so a second pin
/// appearing in one of them -- a second clone step, a duplicated
/// lockfile entry -- is a failure to look at rather than a count that
/// quietly grew.
const AM_FS_CORE_DECLARATIONS: usize = 7;

/// Every file that names the `rust-fs-core` this crate is built against
/// names the same one.
///
/// Six files carry that version and nothing compared them.
/// `fuzz/Cargo.lock` still resolved `rust-fs-core` 0.2.10 after the crate,
/// its own lockfile, `chores.yml` and both workflows had moved to
/// 0.2.13. Nothing noticed: the fuzz run does not build `--locked`, so
/// cargo rewrote the stale entry in the runner's copy and the committed
/// one stayed wrong. The same drift in rust-fs-squashfs left its nightly
/// fuzz run red four nights running on an overflow core had already
/// fixed.
///
/// The count is asserted as well as the agreement. A scan that stopped
/// matching would otherwise report perfect agreement among nothing.
#[test]
fn every_declaration_of_the_am_fs_core_pin_names_the_same_version() {
    let root = manifest_dir();
    let mut declared: Vec<(&str, String, String)> = Vec::new();
    let mut silent = Vec::new();

    for rel in FILES_THAT_PIN_AM_FS_CORE {
        let found = am_fs_core_versions_declared(&read_or_panic(&root.join(rel)));
        if found.is_empty() {
            silent.push(*rel);
        }
        for (spelling, version) in found {
            declared.push((rel, spelling, version));
        }
    }

    assert!(
        silent.is_empty(),
        "these files are listed as declaring the rust-fs-core pin and no longer do: \
         {silent:?}. Either the pin moved somewhere this guard does not read -- in \
         which case add it to FILES_THAT_PIN_AM_FS_CORE -- or the file stopped \
         pinning the sibling and now builds against whatever is checked out."
    );
    assert_eq!(
        declared.len(),
        AM_FS_CORE_DECLARATIONS,
        "expected {AM_FS_CORE_DECLARATIONS} declarations of the rust-fs-core pin \
         across {FILES_THAT_PIN_AM_FS_CORE:?}, found {declared:?}"
    );

    let agreed = &declared[0].2;
    let disagreeing: Vec<String> = declared
        .iter()
        .filter(|(_, _, v)| v != agreed)
        .map(|(file, spelling, v)| format!("{file}: {spelling} says {v}"))
        .collect();
    assert!(
        disagreeing.is_empty(),
        "the rust-fs-core pin disagrees across the files that declare it. \
         {}: {} says {agreed}, but {disagreeing:?}. A bump has to move every \
         one of them.",
        declared[0].0,
        declared[0].1,
    );
}

mod core_pin_parser {
    use super::{am_fs_core_versions_declared, as_version};

    /// The real dependency line.
    #[test]
    fn a_path_dependency_declares_its_version() {
        let toml = "rust-fs-core = { path = \"../rust-fs-core\", version = \"0.2.13\" }\n";
        assert_eq!(
            am_fs_core_versions_declared(toml),
            vec![(
                "the rust-fs-core dependency's `version`".to_string(),
                "0.2.13".to_string()
            )],
        );
    }

    /// A lockfile entry declares its version on the line after its name,
    /// and the bare `"rust-fs-core",` in a dependents list declares nothing.
    #[test]
    fn a_lockfile_entry_declares_its_version() {
        let lock = "[[package]]\nname = \"rust-fs-btrfs\"\nversion = \"0.7.0\"\n\
                    dependencies = [\n \"rust-fs-core\",\n]\n\n\
                    [[package]]\nname = \"rust-fs-core\"\nversion = \"0.2.10\"\n";
        assert_eq!(
            am_fs_core_versions_declared(lock),
            vec![(
                "the locked rust-fs-core `version`".to_string(),
                "0.2.10".to_string()
            )],
        );
    }

    /// The workflow spelling, with and without the `v`.
    #[test]
    fn the_env_variable_declares_its_version() {
        assert_eq!(
            am_fs_core_versions_declared("  FS_CORE_REF: v0.2.13\n"),
            vec![("`FS_CORE_REF`".to_string(), "0.2.13".to_string())],
        );
    }

    /// The reusable-workflow spelling, which is how release.yml's call to
    /// rust-fs-core's release-cli carries its pin.
    #[test]
    fn a_core_ref_input_declares_its_version() {
        assert_eq!(
            am_fs_core_versions_declared("      core-ref: v0.2.23\n"),
            vec![(
                "the release-cli call's `core-ref`".to_string(),
                "0.2.23".to_string()
            )],
        );
    }

    /// The clone spelling, which is how fuzz.yml carries its pin.
    #[test]
    fn a_clone_branch_declares_its_version() {
        let line = "run: git clone --depth 1 --branch v0.2.13 \
                    https://github.com/antimatter-studios/rust-fs-core.git ../rust-fs-core\n";
        assert_eq!(
            am_fs_core_versions_declared(line),
            vec![(
                "the `git clone --branch` ref".to_string(),
                "0.2.13".to_string()
            )],
        );
    }

    /// Prose naming a version is not a declaration.
    #[test]
    fn a_comment_naming_a_version_is_not_a_declaration() {
        let yaml = "  # v0.2.13: the release whose wrapper answers --verbose.\n  \
                    # FS_CORE_REF: v0.2.10 was the pin before.\n";
        assert!(am_fs_core_versions_declared(yaml).is_empty());
    }

    /// `release.yml` clones `--branch "$FS_CORE_REF"`: the variable, not
    /// a second pin.
    #[test]
    fn a_branch_that_is_a_variable_is_not_a_declaration() {
        let line = "run: git clone --depth 1 --branch \"$FS_CORE_REF\" \
                    https://github.com/antimatter-studios/rust-fs-core.git ../rust-fs-core\n";
        assert!(am_fs_core_versions_declared(line).is_empty());
    }

    /// A branch name is not a version.
    #[test]
    fn a_branch_name_is_not_a_version() {
        assert_eq!(as_version("main"), None);
        assert_eq!(as_version("v0.2"), None);
        assert_eq!(as_version("v0.2.13"), Some("0.2.13".to_string()));
    }
}

/// Every commit on `main` gets a whole CI run. With `cancel-in-progress: true`
/// for every event, a merge cancelled the run of the merge before it, and
/// `ci-ok` failed on jobs that were cancelled rather than broken: a red mark on
/// a commit nobody had finished testing. And GitHub keeps one pending run per
/// concurrency group, so a third merge cancels a second one still queued even
/// with cancelling switched off. So a push is a group of its own, keyed by its
/// commit, and only a pull request's superseded run is cancelled.
#[test]
fn ci_cancels_only_a_pull_requests_superseded_run() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml");
    let text = std::fs::read_to_string(&path).expect("read ci.yml");
    let (mut group, mut cancel) = (None, None);
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("concurrency:") {
            inside = true;
            continue;
        }
        if inside {
            if !line.starts_with(' ') && !line.trim().is_empty() {
                break;
            }
            if let Some(v) = line.trim().strip_prefix("group:") {
                group = Some(v.trim().to_string());
            }
            if let Some(v) = line.trim().strip_prefix("cancel-in-progress:") {
                cancel = Some(v.trim().to_string());
            }
        }
    }
    assert_eq!(
        cancel.as_deref(),
        Some("${{ github.event_name == 'pull_request' }}"),
        "ci.yml cancels a run that is not a pull request's"
    );
    let group = group.expect("ci.yml declares a concurrency group");
    assert!(
        group.contains("github.event.pull_request.number || github.sha"),
        "ci.yml's concurrency group is shared between pushes, so a queued push run is \
         cancelled by the next: {group}"
    );
}
