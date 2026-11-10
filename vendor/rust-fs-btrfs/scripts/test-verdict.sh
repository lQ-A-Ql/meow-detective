#!/usr/bin/env bash
# test-verdict.sh TIER [TIER...]  the last line a green `chore test` prints
#
# One number and one path. Each named tier has a log in tmp/logs/<tier>.log
# (written by ../rust-fs-core/scripts/tier.sh); this reads the cargo result lines out of them
# and prints the total.
#
# WHAT THE NUMBER COUNTS is EXECUTIONS, not distinct tests: `chore test` runs
# the oracle and kernel tiers on their own and then again inside the whole
# release suite, so those tests are counted twice, on purpose. A distinct-test
# count would be the same number whether or not the tiers still ran, and the
# thing worth noticing here is a tier quietly stopping -- which shows up as a
# drop in this number and in nothing else.
#
# A missing log is a failure, not a zero: it means a tier did not run.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOGS="$REPO/tmp/logs"

total=0
tiers=0
for tier in "$@"; do
    log="$LOGS/$tier.log"
    if [ ! -f "$log" ]; then
        echo "test-verdict.sh: $log is missing -- the $tier tier did not run." >&2
        exit 1
    fi
    # `test result: ok. 37 passed; 0 failed; ...`, one per test binary.
    n="$(awk '/^test result: ok\./ { sum += $4 } END { print sum + 0 }' "$log")"
    total=$(( total + n ))
    tiers=$(( tiers + 1 ))
done

printf '%s\n' "test: $tiers tiers green, $total test executions -- logs in tmp/logs/"
