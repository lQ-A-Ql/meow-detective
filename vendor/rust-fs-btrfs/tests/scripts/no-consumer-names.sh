#!/usr/bin/env bash
#
# no-consumer-names.sh — no tracked file names an application built on
# top of this crate.
#
# This crate is a standalone project (AGENTS.md: "Never mention a consuming
# application in the README, the source, or CLI help"). A name slips in the
# easy way -- a CI comment saying which product ships on which
# architecture, a task comment recalling where the build knowledge used to
# live -- and nothing else here would notice (#209). So this reads EVERY
# tracked file: source, tests, scripts, docs, workflows, the changelog.
# Comments included: a name in a comment is still a name in the repository.
#
# THE NAME LIST LIVES HERE AND NOWHERE ELSE. Each entry is a
# case-insensitive extended regex, so one entry covers the spellings of one
# name. This file is the one place allowed to spell them, and the only file
# the scan does not read.
#
#   bash tests/scripts/no-consumer-names.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SELF="$(basename "${BASH_SOURCE[0]}")"
fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$(( fails + 1 )); }

NAMES=(
    'disk[-_ ]?jockey'
    'in[-_]?pace'
)

PATTERN="$(IFS='|'; echo "${NAMES[*]}")"

# Every tracked line under <root> (a git work tree) that names one of them.
scan() {
    git -C "$1" grep -nIiE -e "$PATTERN" -- . ":(exclude)tests/scripts/$SELF" || true
}

mkdir -p "$REPO/tmp"
SANDBOX="$(mktemp -d "$REPO/tmp/no-consumer-names.XXXXXX")"
trap 'rm -rf "$SANDBOX"' EXIT HUP INT TERM

# --- 1. The scan recognises every name it refuses. -----------------------
#
# Without this a pattern that matched nothing -- a typo, a grep that reads
# the flags differently -- would pass the real tree having checked nothing.
git -C "$SANDBOX" init -q
mkdir -p "$SANDBOX/tests/scripts" "$SANDBOX/src" "$SANDBOX/docs"
cat > "$SANDBOX/src/lib.rs" <<'EOF'
//! Written for the DiskJockey app.
fn main() {}
EOF
cat > "$SANDBOX/docs/notes.md" <<'EOF'
Seen on an inpace.service unit.
disk-jockey and Disk Jockey and INPACE_HOME too.
Nothing to see on this line: a disk, a jockey, in place.
EOF
# An untracked file is not the repository's, and this file's own name is
# the one allowed mention.
printf 'DiskJockey\n' > "$SANDBOX/untracked.txt"
printf 'DiskJockey\n' > "$SANDBOX/tests/scripts/$SELF"
git -C "$SANDBOX" add src/lib.rs docs/notes.md "tests/scripts/$SELF"

found="$(scan "$SANDBOX")"
for e in "src/lib.rs:1:" "docs/notes.md:1:" "docs/notes.md:2:"; do
    if grep -qF "$e" <<<"$found"; then
        ok "the scan catches $e"
    else
        fail "the scan missed $e"
    fi
done
count="$(grep -c . <<<"$found")"
if [ "$count" -eq 3 ]; then
    ok "the scan ignores untracked files, its own file and plain words"
else
    fail "the scan found $count lines, expected 3:"$'\n'"$found"
fi

# --- 2. The repository names none of them. --------------------------------
found="$(scan "$REPO")"
if [ -z "$found" ]; then
    ok "no tracked file names a consuming application"
else
    fail "these name an application built on this crate; describe the scenario instead:"$'\n'"$found"
fi

if [ "$fails" -gt 0 ]; then
    echo "FAIL  $fails consumer-name violation(s)" >&2
    exit 1
fi
echo "PASS  no tracked file names a consuming application"
