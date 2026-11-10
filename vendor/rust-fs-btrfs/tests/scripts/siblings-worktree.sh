#!/usr/bin/env bash
# A sibling that is a git WORKTREE counts as checked out.
#
# Every working copy in this family is a worktree, a sibling at a pinned ref
# included. A worktree's `.git` is a FILE naming its gitdir, so a
# `[ -d "$dir/.git" ]` test reads every worktree as missing: `chore siblings`
# then tries to `git init` + `remote add` over it and dies, and anything that
# guards on the sibling refuses to run.
#
# This runs the real `siblings` task body out of chores.yml in a sandbox
# where one sibling is a worktree of a scratch repository at a tag and the
# others are ordinary clones, and requires every one of them to be reported
# present and at the pinned ref. Then one clone is removed, and it must be
# fetched afresh while the worktree is still left alone.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHORES="$REPO/chores.yml"

fails=0
fail() { echo "FAIL  $*" >&2; fails=$((fails + 1)); }

sandbox="$(mktemp -d)"
trap 'rm -rf "$sandbox"' EXIT

# Nothing from the caller's git configuration reaches the sandbox.
export GIT_CONFIG_GLOBAL="$sandbox/gitconfig" GIT_CONFIG_NOSYSTEM=1
git config --global user.name test
git config --global user.email test@example.invalid
git config --global init.defaultBranch main
git config --global commit.gpgsign false
git config --global tag.gpgsign false

# --- The task body, as chores.yml has it. ----------------------------------
body="$(awk '
    $0 == "  siblings:" { task = 1; next }
    task && /^  [a-z][a-z:_-]*:$/ { exit }
    task && /^      - \|$/ { inside = 1; next }
    task && inside && /^      - / { exit }
    task && inside { sub(/^        /, ""); print }
' "$CHORES")"
[ -n "$body" ] || { fail "chores.yml has a siblings task with a script body"; exit 1; }

names="$(printf '%s\n' "$body" \
    | sed -nE "s/^ *([a-z0-9-]+) +'\{\{\.[A-Z_]+_URL\}\}'.*/\1/p")"
[ -n "$names" ] || { fail "the siblings task names its siblings"; exit 1; }
first="$(printf '%s\n' "$names" | head -n 1)"
last="$(printf '%s\n' "$names" | tail -n 1)"

origin="$sandbox/origin"
script="$(printf '%s\n' "$body" \
    | sed -E "s#\{\{\.[A-Z_]+_URL\}\}#$origin#g; s#\{\{\.[A-Z_]+_REF\}\}#v1#g")"
case "$script" in
    *'{{'*) fail "every template variable in the siblings task was substituted" ;;
esac

# --- A scratch upstream: v1 is tagged, main is one commit past it. ---------
git init -q "$origin"
git -C "$origin" commit -q --allow-empty -m one
git -C "$origin" tag v1
git -C "$origin" commit -q --allow-empty -m two
git -C "$origin" remote add origin "$origin"

# The checkout the task runs from. Siblings resolve beside its main tree.
root="$sandbox/root"
git init -q "$root/this"
git -C "$root/this" commit -q --allow-empty -m this

# The first sibling is a worktree at the tag; every other one is a clone.
git -C "$origin" worktree add -q --detach "$root/$first" v1
for n in $names; do
    [ "$n" = "$first" ] && continue
    git clone -q "$origin" "$root/$n"
done
[ -f "$root/$first/.git" ] || fail "the fixture's $first is a worktree (.git is a file)"

run() { (cd "$root/this" && bash -c "$script") 2>&1; }

out="$(run)"; rc=$?
[ "$rc" -eq 0 ] || fail "siblings exits 0 when a sibling is a worktree (rc=$rc): $out"
for n in $names; do
    case "$out" in
        *"siblings: $n at or ahead of v1"*) ;;
        *) fail "siblings reports $n present at v1; it said: $out" ;;
    esac
done
case "$out" in
    *"cloning $first"*) fail "siblings tried to clone over the $first worktree: $out" ;;
esac
[ "$(git -C "$root/$first" rev-parse HEAD)" = "$(git -C "$origin" rev-parse v1)" ] \
    || fail "the $first worktree was left at v1"

# A sibling that is genuinely missing is still fetched.
if [ "$last" != "$first" ]; then
    rm -rf "${root:?}/$last"
    out="$(run)"; rc=$?
    [ "$rc" -eq 0 ] || fail "siblings exits 0 fetching a missing sibling (rc=$rc): $out"
    case "$out" in
        *"cloning $last at v1"*) ;;
        *) fail "siblings fetches the missing $last; it said: $out" ;;
    esac
    [ -d "$root/$last/.git" ] || fail "the missing $last was checked out"
    case "$out" in
        *"siblings: $first at or ahead of v1"*) ;;
        *) fail "siblings still reports the $first worktree present; it said: $out" ;;
    esac
fi

# --- No other guard in chores.yml asks for a .git DIRECTORY. ---------------
dir_tests="$(grep -nE -- '-d "[^"]*/\.git"' "$CHORES" || true)"
[ -z "$dir_tests" ] || fail "chores.yml tests for a .git directory, which a worktree does not have:
$dir_tests"

if [ "$fails" -gt 0 ]; then
    exit 1
fi
echo "PASS  a sibling that is a git worktree counts as checked out"
