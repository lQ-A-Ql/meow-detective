#!/usr/bin/env bash
# test.sh [cargo test args...]  run the suite in an owned scratch directory
# test.sh --print-temp-dir      print that directory and exit
#
# SCRATCH LIVES IN THE REPOSITORY, always: tmp/ (gitignored), and never
# the system temporary directory or a runner-supplied one. The oracle
# tools run inside the fs-linux-test-harness VM, which sees this
# repository at the path the host knows it by and nothing else of the
# host — so an image anywhere else is a path the tool asked to read it
# cannot open. The same rule is written in Rust in
# tests/support/src/lib.rs (select_temp_dir), and
# tests/test_contract.rs checks that no test writes anywhere else.
#
# FS_BTRFS_TEST_TMPDIR supplies an exact directory instead; it must be
# inside the repository, and it is the caller's to delete.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR=""

cleanup() {
    if [[ -n "$RUN_DIR" && -d "$RUN_DIR" ]]; then
        find "$RUN_DIR" -depth -mindepth 1 -delete
        rmdir "$RUN_DIR"
    fi
}
trap cleanup EXIT HUP INT TERM

if [[ -n "${FS_BTRFS_TEST_TMPDIR:-}" ]]; then
    case "$FS_BTRFS_TEST_TMPDIR" in
        "$REPO"/*) ;;
        *)
            echo "test.sh: FS_BTRFS_TEST_TMPDIR is $FS_BTRFS_TEST_TMPDIR, which is outside" >&2
            echo "         $REPO. The oracle tools run in the harness VM, which sees this" >&2
            echo "         repository and nothing else of the host." >&2
            exit 1
            ;;
    esac
    # An exact caller-supplied directory is not ours to delete.
    mkdir -p "$FS_BTRFS_TEST_TMPDIR"
else
    mkdir -p "$REPO/tmp"
    RUN_DIR="$(mktemp -d "$REPO/tmp/fs-btrfs-tests.XXXXXX")"
    export FS_BTRFS_TEST_TMPDIR="$RUN_DIR"
fi

export TMPDIR="$FS_BTRFS_TEST_TMPDIR"

if [[ "${1:-}" == "--print-temp-dir" ]]; then
    printf '%s\n' "$FS_BTRFS_TEST_TMPDIR"
    exit 0
fi

# THE RUN OWNS THE VM ITS TESTS BOOT (#141). An oracle or kernel test
# brings the harness VM up from inside its own process, and that boot
# takes the slot -- ONE for the whole machine, shared by every
# repository. Left to chore's `after_all` reaper, which runs only inside
# a chore invocation of this repository, a run that reached cargo any
# other way (this script, ../rust-fs-core/scripts/tier.sh, by hand) exited 0 with the VM
# idle and the slot held, and every other repository's VM work queued
# behind it at 0% CPU until the guest's idle deadline.
#
# So cargo runs inside a harness session: vm-session.sh's EXIT trap
# brings the VM down and releases the slot when the run ends -- passed,
# failed or killed -- and its marker keeps another invocation's reaper
# off the VM while the run is using it. The release is the harness's
# own, which frees the slot only when this machine holds it, and only
# once the VM is confirmed stopped. FLTH_KEEP_VM=1 keeps the VM up for
# runs back to back, as it does for `chore fixtures`.
#
# No session where there is no VM to own: inside the guest (`chore
# test:vm`), without the harness sibling (a test that needs the VM then
# fails naming `chore siblings`), or on a host that cannot run the VM at
# all (CI's unit and aarch64 jobs), where a teardown would fail on a
# machine that could never have existed.
HARNESS="$REPO/../fs-linux-test-harness"
session=()
if [[ "${FLTH_GUEST:-}" != 1 && -f "$HARNESS/scripts/vm-session.sh" ]] &&
    "$HARNESS/scripts/host-tools.sh" --quiet >/dev/null 2>&1; then
    export FLTH_CONFIG="$REPO/fs-linux-test-harness.toml"
    # Not `exec`, and cargo is not exec'd either: the trap belongs to
    # this shell, which must outlive cargo to run it.
    # shellcheck disable=SC2016  # expanded by the inner shell
    session=(bash -c '. "$1" || exit 1; shift; "$@"' fs-btrfs-test "$HARNESS/scripts/vm-session.sh")
fi

# `--features cli` builds the command-line tools (the `rust-fs-btrfs`
# target requires it), so every tier reaches their tests. The library a
# consumer links is built without it, and gains nothing from it.
#
# The `+` form because an empty array is an unbound variable to bash
# before 4.4, which is what macOS ships.
${session[@]+"${session[@]}"} cargo test --features cli "$@"
