#!/usr/bin/env bash
# The `cli` tier's gate refuses what it exists to refuse (scripts/test-cli.sh).
#
# A gate that has never been seen to fail is indistinguishable from no gate,
# so each refusal is driven here with stand-ins on a PATH built for the
# purpose -- no build, no cargo:
#
#   - no rust-fs-btrfs on PATH: the tier fails naming `chore cli:install`
#     and the brew line, and runs nothing;
#   - a rust-fs-btrfs that is someone else's: fails, naming the path;
#   - ours, but doctor finds a shadowed name: fails with doctor's report and
#     tests nothing;
#   - a test file that exits 0 without its trailing line: fails, because it
#     stopped early;
#   - a file that exits non-zero: fails;
#   - and the good path passes, so the refusals are not a tier that always
#     fails.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
TIER="$REPO/scripts/test-cli.sh"
CRATE="$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$REPO/Cargo.toml" | head -n 1)"

fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$((fails + 1)); }

jq_path="$(command -v jq 2>/dev/null || true)"
if [ -z "$jq_path" ]; then
    echo "not ok jq is not installed; the cli tier needs it (brew install jq / apt-get install jq)" >&2
    exit 1
fi

mkdir -p "$REPO/tmp"
sandbox="$(mktemp -d "$REPO/tmp/cli-tier.XXXXXX")"
trap 'rm -rf "$sandbox"' EXIT HUP INT TERM
mkdir -p "$sandbox/jq" "$sandbox/bin" "$sandbox/tests"
ln -s "$jq_path" "$sandbox/jq/jq"
SAFE_PATH="$sandbox/bin:$sandbox/jq:/usr/bin:/bin"

# A stand-in rust-fs-btrfs: VERSION_LINE for --version, DOCTOR_STATUS for doctor.
stand_in() {
    cat >"$sandbox/bin/rust-fs-btrfs" <<STUB
#!/bin/sh
case "\$1" in
    --version) echo '$1' ;;
    doctor) echo 'fs.btrfs: $3 (/somewhere/fs.btrfs)'; echo '  fix: \`brew unlink some-formula\`'; exit $2 ;;
esac
STUB
    chmod +x "$sandbox/bin/rust-fs-btrfs"
}

run_tier() {
    PATH="$SAFE_PATH" CLI_TESTS="$sandbox/tests" bash "$TIER" >"$sandbox/out" 2>&1
}

good_file() {
    printf '%s\n' 'echo "test result: ok. 1 passed; 0 failed"' 'echo "test-good: all checks passed"' \
        >"$sandbox/tests/test-good.sh"
}

good_file

# No rust-fs-btrfs at all.
rm -f "$sandbox/bin/rust-fs-btrfs"
if run_tier; then
    fail "the tier passed with no rust-fs-btrfs on PATH"
elif ! grep -q 'chore cli:install' "$sandbox/out" || ! grep -q 'brew install antimatter-studios/tap/rust-fs-btrfs' "$sandbox/out"; then
    fail "no rust-fs-btrfs: the refusal does not name both ways to install it: $(cat "$sandbox/out")"
else
    ok "no rust-fs-btrfs on PATH: refused, naming both ways to install it"
fi

# Someone else's rust-fs-btrfs.
stand_in "rust-fs-btrfs 9.9" 0 ours
if run_tier; then
    fail "the tier passed with a rust-fs-btrfs that is not $CRATE's"
elif ! grep -q "$sandbox/bin/rust-fs-btrfs is not $CRATE's" "$sandbox/out"; then
    fail "a foreign rust-fs-btrfs: the refusal does not name it: $(cat "$sandbox/out")"
else
    ok "someone else's rust-fs-btrfs: refused, naming its path"
fi

# Ours, but doctor finds a shadowed name.
stand_in "rust-fs-btrfs ($CRATE) 0.0.0" 1 foreign
if run_tier; then
    fail "the tier passed although doctor failed"
else
    grep -q 'brew unlink some-formula' "$sandbox/out" ||
        fail "doctor failed: its report (the fix) is not in the tier's output: $(cat "$sandbox/out")"
    grep -q 'test-good' "$sandbox/out" &&
        fail "doctor failed and the tier still ran a test file"
    ok "doctor failing: refused with its report, before any test file"
fi

# From here the stand-in passes doctor.
stand_in "rust-fs-btrfs ($CRATE) 0.0.0" 0 ours

# The good path passes: the refusals above are not a tier that always fails.
if ! run_tier; then
    fail "the tier failed with ours on PATH and one passing file: $(cat "$sandbox/out")"
else
    ok "ours on PATH and a passing file: the tier passes"
fi

# A file that exits 0 part-way.
printf '%s\n' 'echo "test result: ok. 1 passed; 0 failed"' 'exit 0' 'echo "test-early: all checks passed"' \
    >"$sandbox/tests/test-early.sh"
if run_tier; then
    fail "the tier passed a file that exited 0 before its last line"
elif ! grep -q 'test-early exited 0 without its last line' "$sandbox/out"; then
    fail "an early exit is not named: $(cat "$sandbox/out")"
else
    ok "a file that exits 0 before its last line: refused, by name"
fi
rm -f "$sandbox/tests/test-early.sh"

# A file that fails.
printf '%s\n' 'echo "FAIL  something"' 'exit 1' >"$sandbox/tests/test-red.sh"
if run_tier; then
    fail "the tier passed a file that exited 1"
else
    ok "a file that exits 1: refused"
fi
rm -f "$sandbox/tests/test-red.sh"

# No test files at all.
rm -f "$sandbox/tests/test-good.sh"
if run_tier; then
    fail "the tier passed with no test files"
else
    ok "no test files at all: refused"
fi

if [ "$fails" -gt 0 ]; then
    echo "FAIL  $fails refusal(s) of the cli tier did not refuse" >&2
    exit 1
fi
echo "PASS  the cli tier refuses a missing, foreign or shadowed tool, and a file that stops early"
