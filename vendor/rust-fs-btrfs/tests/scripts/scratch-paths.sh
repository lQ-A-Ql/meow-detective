#!/usr/bin/env bash
#
# scratch-paths.sh — the tests reach for the guest, and for nothing else
# on this machine.
#
# THREE RULES, ALL OF THEM THE ORACLE'S. The btrfs-progs tools and the
# kernel oracles run inside the fs-linux-test-harness VM, which sees this
# repository mounted at the path the host knows it by AND NOTHING ELSE OF
# THE HOST:
#
#   * a scratch image under `/tmp` is a path the tool asked to read it
#     cannot open, and the failure arrives as a puzzling "No such file or
#     directory" from a guest, a long way from the line that chose it;
#   * a fixture under `.vm-share` is a fixture in the directory a run
#     hands things across in, which is how a scratch image one test left
#     behind became a fixture the next suite walked over;
#   * and `sudo` is a privilege this suite no longer asks for anywhere,
#     because the mounts happen in the guest.
#
# `fs_btrfs_test_support::temp_dir()` (and the `temp_path!` macro) is the
# one way to a scratch path; `fixture()` and `fixture_dir()` are the one
# way to a fixture. tests/test_contract.rs is the thorough version of
# this, in Rust, over the parsed sources; this is the cheap one that also
# covers the shell and the workflow.
#
# EVERY CHECK BELOW IS A CODE SHAPE, not a word. Prose that says "this
# used to run under sudo on the runner" is exactly the sort of comment
# worth keeping, and a guard that forbade the word would delete the
# history along with the practice.
#
#   bash tests/scripts/scratch-paths.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SELF="${BASH_SOURCE[0]}"
fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$(( fails + 1 )); }

# ripgrep is REQUIRED. Every search below treats "no match" as the
# passing case, so without this a missing `rg` would print "command not
# found", match nothing and report PASS — which is how two violations sat
# in a sister repository's tree while its CI stayed green.
if ! command -v rg >/dev/null 2>&1; then
    echo "not ok ripgrep (rg) is installed; run 'chore tools'" >&2
    echo "FAIL  this check cannot run without rg, and a check that cannot run must not pass" >&2
    exit 1
fi
ok "ripgrep is installed, so a no-match result means what it says"

# check <description> <matches>
check() {
    local what="$1" matches="$2"
    if [ -n "$matches" ]; then
        fail "$what:"
        printf '%s\n' "$matches" >&2
    else
        ok "$what"
    fi
}

# --- 1. Nothing writes outside the repository. ---------------------------
check "no test reaches for the system temporary directory" \
    "$(rg -n 'std::env::temp_dir\(\)|PathBuf::from\("/tmp|Path::new\("/tmp|"/tmp/' \
        "$REPO/tests" "$REPO/src" "$REPO/examples" \
        --glob '!**/support/src/lib.rs' \
        --glob "!$(basename "$SELF")" || true)"

# --- 2. Fixtures come from test-disks/, not from the harness's share. ----
#
# A string literal beginning `.vm-share` is a path; a backtick-quoted
# `.vm-share` in a comment is an explanation, and several of them are
# load-bearing.
check "no test reads fixtures out of the harness share" \
    "$(rg -n '"\.vm-share|join\("\.vm-share' \
        "$REPO/tests" "$REPO/src" "$REPO/examples" || true)"

# --- 3. Nothing INVOKES a script this migration deleted. -----------------
#
# The shape that matters is a command: `./scripts/vm.sh up` in a README,
# `bash scripts/build-fixtures-native.sh` in a workflow. A sentence
# recording that a file replaced one of them is not an instruction and is
# worth keeping — which is why this matches an invocation rather than a
# name.
deleted='vm\.sh|vm-build-[a-z-]*\.sh|build-fixtures-native\.sh|build-[a-z]+-fixtures\.sh|fixture-geometries\.sh|install-host-tools\.sh'
check "nothing invokes a script this repository no longer has" \
    "$(rg -n "(\./|bash |sh |source |run: |\\\$\()scripts/($deleted)" \
        "$REPO/tests" "$REPO/src" "$REPO/scripts" "$REPO/examples" "$REPO/test-disks" \
        "$REPO/chores.yml" "$REPO/.github" "$REPO/README.md" "$REPO/docs" \
        --glob '!**/human-code-*.md' \
        --glob '!**/audit-log.md' \
        --glob '!**/code-quality-review-*.md' \
        --glob "!$(basename "$SELF")" \
        | grep -v 'fs-linux-test-harness/scripts/' || true)"

# --- 4. The oracle tools are reached one way only. -----------------------
check "no test spawns an oracle tool on the host" \
    "$(rg -n 'Command::new\("(mkfs\.btrfs|btrfs|btrfstune|btrfs-image|setfattr|getfattr|chattr|lsattr|mount|umount|losetup)"\)' \
        "$REPO/tests" "$REPO/src" "$REPO/examples" \
        --glob '!**/test_contract.rs' || true)"

# --- 5. `sudo` has left the tests. ---------------------------------------
#
# The old gate mounted every fixture with `sudo mount -o loop` on the CI
# runner, and built most of them the same way. That is why the kernel
# half of this repository's gate could not be run anywhere else — not on
# a Mac, and not on a Linux workstation without handing a test suite
# root. Nothing under tests/ or test-disks/ asks for it now.
#
# Two places outside that still may, and should: `scripts/tools.sh`
# installs the HOST's own packages (ripgrep) with the system package
# manager, and `scripts/trace-commit.sh` is a root-only blktrace
# diagnostic meant to be run in the guest. `ci.yml` uses it once, to
# REPLACE the runner's btrfs-progs with stubs that fail loudly, which is
# the opposite request.
#
# `\b` on BOTH sides (#200). It used to require a trailing space, which
# catches `sudo mount` in a shell recipe and misses `Command::new("sudo")`
# in a test by one character -- the quote. That shape passed every gate here
# while running mkfs.btrfs and a loop mount on the host. The probe below
# proves the pattern reads the Rust spelling before the tree is scanned.
root_pattern='^[^#/]*\bsudo\b'
probe="$(mktemp -d "${TMPDIR:-/tmp}/scratch-paths.XXXXXX")"
printf '    let out = Command::new("%s")\n' sudo > "$probe/escape.rs"
if rg -q "$root_pattern" "$probe/escape.rs"; then
    ok "the root check reads a test that spawns it from Rust"
else
    fail "the root check reads a test that spawns it from Rust (its pattern misses Command::new(\"sudo\"))"
fi
rm -rf "$probe"
check "no test or fixture recipe needs root" \
    "$(rg -n "$root_pattern" "$REPO/tests" "$REPO/test-disks" \
        --glob '!**/*.md' \
        --glob "!$(basename "$SELF")" || true)"

# --- 6. The VM is the harness's, and this repository keeps none. ---------
#
# This repository used to carry its own Debian VM — a Vagrantfile, a
# vm.sh, a vm-slot.sh — and a sister repository carried another copy of
# the same machine; the two drifted until a fix made in one reached
# nothing in the other (#109). The machine now lives once, in
# fs-linux-test-harness, which keeps its state OUTSIDE this tree
# (FLTH_CACHE_DIR/machines/<project>/), so nothing a run makes here
# needs ignoring. A rule that ignores a VM's state would only hide a
# local copy brought back: its `.vagrant/` would never show in
# `git status`.
if tracked="$(git -C "$REPO" ls-files)"; then
    check "no VM definition or orchestrator is kept in this repository" \
        "$(printf '%s\n' "$tracked" |
            rg -n '(^|/)(Vagrantfile|vm-slot\.sh|vm-session\.sh|vm\.sh|cloud-init[^/]*|user-data)$|^tests/vagrant/' || true)"
else
    fail "git ls-files ran, so the tracked tree could be searched for a VM copy"
fi
check "no ignore rule hides a local VM's state" \
    "$(rg -n '^[^#]*(\.vagrant|\.qemu|\.vm-cache)' "$REPO/.gitignore" || true)"

if [ "$fails" -gt 0 ]; then
    echo "FAIL  $fails violation(s) of where the tests may reach" >&2
    exit 1
fi
echo "PASS  the tests reach for the guest, and for nothing else on this machine"
