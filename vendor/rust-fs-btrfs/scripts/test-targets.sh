#!/usr/bin/env bash
#
# test-targets.sh unit|images|oracle|kernel — print the `cargo test`
# target arguments for one tier of the suite.
#
#   unit    the library and every tests/*.rs that reaches neither a
#           fixture nor the VM: no tool, no kernel, no harness
#   images  the tests that read a fixture but need no VM — everything a
#           host without KVM (GitHub's arm64 runners) can still run
#   oracle  every tests/*.rs that runs a btrfs-progs tool (in the VM)
#   kernel  every tests/*.rs that mounts one of our images with the real
#           kernel (in the VM)
#
# DERIVED FROM THE TESTS THEMSELVES, not from a list someone has to
# keep. That is not a preference here, it is the defect this replaces:
# the old workflow named every suite by hand in `cargo test --test
# <name>` steps, and `read_path_cost` and `dup_mirror_fallback` both
# arrived after the list and were never added to it, so they ran only in
# the job with no fixtures and never executed an assertion on a pull
# request (#70).
#
# A test names a fixture through fs_btrfs_test_support::fixture /
# fixtures_matching / fixture_dir, reaches a tool only through `oracle`
# / `assert_btrfs_check_clean` / `dump_super` / `dump_tree`, and the
# kernel only through `guest_kernel_*` — the helpers that fail, never
# skip, when the VM or the tool is missing. tests/test_contract.rs fails
# if a test reaches either any other way, so the classification cannot
# drift.
#
# A file that does both is a KERNEL test: the tiers are disjoint so that
# `chore test:oracle` and `chore test:kernel` together run each file
# once.
#
# Library tests that need a fixture or the VM live in modules named
# `needs_host`; `unit` excludes them with cargo's name filter, and the
# whole-suite release run (`chore test`) runs them with the VM up.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
HOST='test-disks|fixture\(|fixtures_matching\(|fixture_dir\(|oracle\(|assert_btrfs_check_clean\(|dump_super\(|dump_tree\(|guest_kernel_'
VM='oracle\(|assert_btrfs_check_clean\(|dump_super\(|dump_tree\(|guest_kernel_'
TOOL='oracle\(|assert_btrfs_check_clean\(|dump_super\(|dump_tree\('
KERNEL='guest_kernel_'

tier="${1:-}"
args=()
for f in "$REPO"/tests/*.rs; do
    name="$(basename "$f" .rs)"
    case "$tier" in
        unit) grep -qE "$HOST" "$f" || args+=(--test "$name") ;;
        images)
            if grep -qE "$HOST" "$f" && ! grep -qE "$VM" "$f"; then
                args+=(--test "$name")
            fi
            ;;
        oracle)
            if grep -qE "$TOOL" "$f" && ! grep -qE "$KERNEL" "$f"; then
                args+=(--test "$name")
            fi
            ;;
        kernel) grep -qE "$KERNEL" "$f" && args+=(--test "$name") ;;
        *) echo "usage: test-targets.sh unit|images|oracle|kernel" >&2; exit 2 ;;
    esac
done
case "$tier" in
    unit) printf '%s\n' --lib "${args[@]}" -- --skip needs_host:: ;;
    *) printf '%s\n' "${args[@]}" ;;
esac
