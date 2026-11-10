#!/usr/bin/env bash
# Run every fuzz target for a bounded time.
#
# This is the explorer, not the gate. It is allowed to find something
# and fail; what it is not allowed to do is run for an unbounded time,
# which is why every target gets the same budget and the script reports
# which ones found something rather than stopping at the first.
#
# Anything it finds belongs in fuzz/corpus/, where tests/fuzz_decoders.rs
# replays it on every pull request from then on.
#
# Usage: scripts/fuzz-all.sh [seconds-per-target]
set -uo pipefail

budget="${1:-60}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"

targets=$(sed -n 's/^name = "\(.*\)"$/\1/p' fuzz/Cargo.toml | tail -n +2)
[ -n "$targets" ] || { echo "no fuzz targets declared in fuzz/Cargo.toml" >&2; exit 1; }

scratch="$(mktemp -d "${TMPDIR:-/tmp}/btrfs-fuzz-scratch.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

failed=""
for target in $targets; do
    echo "::group::fuzz $target (${budget}s)"
    # The committed corpus is a seed, not a scratch pad. libFuzzer
    # writes every coverage-expanding input back into the FIRST corpus
    # directory it is given, so it gets a throwaway one and the
    # committed seeds are passed after it, read-only. Without this a
    # local run leaves dozens of hash-named blobs in fuzz/corpus/, and
    # the curated seeds -- real structures, and the reproducer for each
    # defect ever found -- get lost among them.
    mkdir -p "$scratch/$target"
    if cargo +nightly fuzz run "$target" "$scratch/$target" "fuzz/corpus/$target" \
        -- -max_total_time="$budget"; then
        echo "$target: clean"
    else
        echo "::error::fuzz target '$target' found an input that crashed or hung"
        failed="$failed $target"
    fi
    echo "::endgroup::"
done

if [ -n "$failed" ]; then
    echo "::error::fuzzing found crashes in:$failed"
    echo "Each reproducer is under fuzz/artifacts/<target>/. Commit it to"
    echo "fuzz/corpus/<target>/ so tests/fuzz_decoders.rs replays it from now on."
    exit 1
fi
echo "every target ran ${budget}s without finding a crash"
