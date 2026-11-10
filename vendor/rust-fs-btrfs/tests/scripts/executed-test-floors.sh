#!/usr/bin/env bash
#
# executed-test-floors.sh — the per-target floors name real targets, and CI
# holds the suite to them.
#
# rust-fs-core's floor (`../rust-fs-core/scripts/test-floor.sh --targets FILE TIER`)
# refuses a suite that emptied from the inside: it pairs cargo's `Running
# tests/<name>.rs` line with libtest's `test result: ok. N passed`, and a
# target below its floor -- or missing from the log -- fails the run. How it
# reads a log, coloured or not, is core's and is tested there. What is this
# repository's is the file it reads, .github/test-floors.txt, and that file
# can be wrong in ways core cannot see:
#
#   * a line naming a target that does not exist checks nothing real;
#   * a floor that is not a positive number is no floor;
#   * an integration target with no line is a suite that can empty
#     unnoticed;
#   * a file nothing applies protects nothing.
#
#   bash tests/scripts/executed-test-floors.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FLOORS="$REPO/.github/test-floors.txt"
fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$(( fails + 1 )); }

[ -f "$FLOORS" ] || { echo "not ok .github/test-floors.txt is missing" >&2; exit 1; }

named=""
bad_lines=""
while read -r target floor rest; do
    case "$target" in ''|\#*) continue ;; esac
    named="$named $target"
    case "$floor" in ''|*[!0-9]*) bad_lines="$bad_lines $target(non-numeric '${floor:-none}')"; continue ;; esac
    [ "$floor" -ge 1 ] || bad_lines="$bad_lines $target(floor 0)"
    [ "$target" = lib ] || [ -f "$REPO/tests/$target.rs" ] || bad_lines="$bad_lines $target(no tests/$target.rs)"
done < "$FLOORS"
[ -n "$named" ] && ok "the floors file names targets" || fail "the floors file names no targets"
[ -z "$bad_lines" ] && ok "every line names a real target with a floor of at least 1" \
    || fail "lines that check nothing real:$bad_lines"

unlisted=""
for f in "$REPO"/tests/*.rs; do
    t="$(basename "$f" .rs)"
    case " $named " in *" $t "*) ;; *) unlisted="$unlisted $t" ;; esac
done
[ -z "$unlisted" ] && ok "every integration target has a floor" \
    || fail "integration targets with no floor, which could empty unnoticed:$unlisted"

grep -qE 'test-floor\.sh --targets \.github/test-floors\.txt [a-z]+' "$REPO/.github/workflows/ci.yml" \
    && ok "ci.yml applies the floors file through rust-fs-core's floor" \
    || fail "nothing in ci.yml runs ../rust-fs-core/scripts/test-floor.sh --targets .github/test-floors.txt"

[ "$fails" = 0 ] || exit 1
echo "PASS  every integration target has a floor, and CI holds the suite to them"
