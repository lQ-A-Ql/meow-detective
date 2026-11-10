#!/usr/bin/env bash
# A sibling `chore siblings` CLONES lands on a branch, so raising the pin
# later moves it.
#
# The task has two paths. The existing-checkout path treats the pin as a
# floor and will only fast-forward a checkout that is on `main`; anything
# else it refuses to move, because a checkout a person put on a branch is
# theirs. The fresh-clone path used to end `checkout FETCH_HEAD` -- a
# DETACHED head -- and `continue` past that floor check. Nothing showed
# while the pin stood still. The moment it was raised, the sibling this
# task had itself created was below the floor, on `HEAD` rather than
# `main`, and the task exited 1 telling the developer to move it by hand.
#
# This runs the real `siblings` body out of chores.yml in a sandbox: every
# sibling is missing, the pin is v1, and each must be cloned onto a branch
# at v1. Then the pin is raised to v2 and the same run must move them.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHORES="$REPO/chores.yml"

fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$((fails + 1)); }

sandbox="$(mktemp -d)"
trap 'rm -rf "$sandbox"' EXIT

# Nothing from the caller's git configuration reaches the sandbox, and the
# default branch is deliberately NOT main: a fresh clone must end up on main
# because the task put it there, not because of whoever ran it.
export GIT_CONFIG_GLOBAL="$sandbox/gitconfig" GIT_CONFIG_NOSYSTEM=1
git config --global user.name test
git config --global user.email test@example.invalid
git config --global init.defaultBranch trunk
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

origin="$sandbox/origin"
script_at() {
    printf '%s\n' "$body" \
        | sed -E "s#\{\{\.[A-Z_]+_URL\}\}#$origin#g; s#\{\{\.[A-Z_]+_REF\}\}#$1#g"
}

# --- A scratch upstream on main: v1, then v2, then one commit past it. -----
git init -q -b main "$origin"
git -C "$origin" commit -q --allow-empty -m one
git -C "$origin" tag v1
git -C "$origin" commit -q --allow-empty -m two
git -C "$origin" tag v2
git -C "$origin" commit -q --allow-empty -m three

# The checkout the task runs from. Siblings resolve beside its main tree.
root="$sandbox/root"
git init -q "$root/this"
git -C "$root/this" commit -q --allow-empty -m this

run() { (cd "$root/this" && bash -c "$(script_at "$1")") 2>&1; }

# --- Pinned at v1: every sibling is cloned, onto a branch, at v1. ----------
out="$(run v1)"; rc=$?
if [ "$rc" -eq 0 ]; then
    ok "siblings clones every missing sibling at v1"
else
    fail "siblings clones every missing sibling at v1 (rc=$rc): $out"
fi
for n in $names; do
    branch="$(git -C "$root/$n" rev-parse --abbrev-ref HEAD 2>/dev/null)"
    if [ "$branch" = "main" ]; then
        ok "a freshly cloned $n is on main, not detached"
    else
        fail "a freshly cloned $n is on main, not detached (it is on '$branch')"
    fi
    if [ "$(git -C "$root/$n" rev-parse HEAD 2>/dev/null)" = "$(git -C "$origin" rev-parse v1)" ]; then
        ok "a freshly cloned $n sits exactly at the pin"
    else
        fail "a freshly cloned $n sits exactly at the pin v1"
    fi
done

# --- The pin is raised to v2: the task moves what it created. --------------
out="$(run v2)"; rc=$?
if [ "$rc" -eq 0 ]; then
    ok "raising the pin moves the siblings this task cloned"
else
    fail "raising the pin moves the siblings this task cloned (rc=$rc): $out"
fi
for n in $names; do
    if git -C "$root/$n" merge-base --is-ancestor v2 HEAD 2>/dev/null; then
        ok "$n is at or ahead of the raised pin"
    else
        fail "$n is at or ahead of the raised pin v2"
    fi
done

# --- A checkout a person put on a branch is still not moved. ---------------
first="$(printf '%s\n' "$names" | head -n 1)"
git -C "$root/$first" checkout -q -b mine v1
out="$(run v2)"; rc=$?
if [ "$rc" -ne 0 ] && [ "$(git -C "$root/$first" rev-parse --abbrev-ref HEAD)" = "mine" ]; then
    ok "a sibling on someone's own branch below the pin is refused, not moved"
else
    fail "a sibling on someone's own branch below the pin is refused, not moved (rc=$rc): $out"
fi

if [ "$fails" -gt 0 ]; then
    echo "FAIL  $fails siblings fresh-clone violation(s)" >&2
    exit 1
fi
echo "PASS  a sibling this task clones can be moved when the pin is raised"
