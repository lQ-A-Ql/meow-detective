#!/usr/bin/env bash
#
# fixture-builder.sh — the fixture builder refuses rather than skips, and
# the two halves of it agree about what a build produces.
#
# THE FAILURE THIS GUARDS AGAINST HAS HAPPENED HERE TWICE. A geometry
# mkfs.btrfs rejected used to leave the builder green with a warning, and
# the suites that read that image took their absent-fixture path and
# passed (#140); and the compression and nodatacow images were built by
# one builder and not the other, so their oracles compared nothing on
# every pull request (#69). There is one builder now — a host-side driver
# and a guest-side recipe file — and this checks that it stops when a
# fixture cannot be made, and that the driver's artefact list, the guest
# recipes and chores.yml's `generates:` still describe the same build.
#
# The guest recipes are exercised with `mkfs.btrfs`, `btrfs` and `id`
# stubbed, so this needs no VM, no root and no btrfs-progs — which is
# what lets it run in the unit-shaped `chore test:scripts` tier.
#
#   bash tests/scripts/fixture-builder.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DRIVER="$REPO/test-disks/build-fixtures.sh"
RECIPES="$REPO/test-disks/guest-build-images.sh"
fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$(( fails + 1 )); }

mkdir -p "$REPO/tmp"
sandbox="$(mktemp -d "$REPO/tmp/fixture-builder-test.XXXXXX")"
trap 'rm -rf "$sandbox"' EXIT
mkdir -p "$sandbox/bin" "$sandbox/out"

# --- the stubs -----------------------------------------------------------
#
# `id` says root, because the recipes refuse to run as anyone else — a
# real refusal, checked below before it is stubbed away.
printf '#!/usr/bin/env bash\necho 0\n' > "$sandbox/bin/id"
# mkfs.btrfs accepts everything unless told to reject a geometry.
cat > "$sandbox/bin/mkfs.btrfs" <<'STUB'
#!/usr/bin/env bash
for arg in "$@"; do
    if [ -n "${REJECT_GEOMETRY:-}" ] && [ "$arg" = "$REJECT_GEOMETRY" ]; then
        echo "mkfs.btrfs: stub refuses $arg" >&2
        exit 1
    fi
done
exit 0
STUB
# `btrfs` answers the two questions the geometry recipe asks of it.
cat > "$sandbox/bin/btrfs" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
    --version) echo "btrfs-progs v0 (stub)" ;;
    inspect-internal) echo "superblock: stub dump" ;;
    *) exit 0 ;;
esac
STUB
for tool in setfattr getfattr setfacl getfacl chattr lsattr losetup; do
    printf '#!/usr/bin/env bash\nexit 0\n' > "$sandbox/bin/$tool"
done
chmod +x "$sandbox"/bin/*

recipes() { PATH="$sandbox/bin:$PATH" bash "$RECIPES" "$@"; }

# --- 1. A misspelt target is named as one, before anything else. ---------
#
# Deliberately WITHOUT the stubs on PATH: a caller who typed `nodatcow`
# on a workstation should be told they typed it, not that they need a
# Debian VM.
out="$(bash "$RECIPES" "$sandbox/out" nodatcow 2>&1)"; rc=$?
[ "$rc" = 2 ] && ok "an unknown target exits 2" || fail "an unknown target exits 2 (got $rc)"
case "$out" in
    *"unknown target 'nodatcow'"*) ok "an unknown target is named" ;;
    *) fail "an unknown target is named: $out" ;;
esac
case "$out" in
    *"runs as root"*) fail "an unknown target is reported before the root check" ;;
    *) ok "an unknown target is reported before the root check" ;;
esac

# --- 2. The recipes refuse to run anywhere but as root in the guest. -----
out="$(bash "$RECIPES" "$sandbox/out" geometry 2>&1)"; rc=$?
[ "$rc" = 1 ] && ok "a non-root run is refused" || fail "a non-root run is refused (got $rc)"
case "$out" in
    *"runs as root in the guest"*) ok "the refusal says where it belongs" ;;
    *) fail "the refusal says where it belongs: $out" ;;
esac

# --- 3. A geometry mkfs.btrfs rejects FAILS the build. -------------------
out="$(REJECT_GEOMETRY=dup recipes "$sandbox/out" geometry 2>&1)"; rc=$?
[ "$rc" != 0 ] && ok "a rejected geometry fails the build" \
               || fail "a rejected geometry fails the build (exited 0)"
case "$out" in
    *"refused the 'dup' geometry"*) ok "the failure names the geometry" ;;
    *) fail "the failure names the geometry: $out" ;;
esac
case "$out" in
    *"hole in the gate"*) ok "the failure says why it is not a skip" ;;
    *) fail "the failure says why it is not a skip" ;;
esac

# --- 4. A clean run produces every geometry, and its dump beside it. -----
rm -rf "$sandbox/out"; mkdir -p "$sandbox/out"
out="$(recipes "$sandbox/out" geometry 2>&1)"; rc=$?
[ "$rc" = 0 ] && ok "a clean geometry run succeeds" || fail "a clean geometry run succeeds: $out"
images=$(find "$sandbox/out" -name '*.img' | wc -l | tr -d ' ')
dumps=$(find "$sandbox/out" -name '*.superdump' | wc -l | tr -d ' ')
[ "$images" = 10 ] && ok "the geometry matrix is ten images" \
                   || fail "the geometry matrix is ten images (got $images)"
[ "$dumps" = "$images" ] && ok "every image has its superblock dump" \
                         || fail "every image has its superblock dump ($dumps of $images)"

# --- 5. The driver, the recipes and chores.yml describe one build. -------
#
# Three lists, in three files: `--artefacts` is what `--check` fails a
# test run over, chores.yml's `generates:` is what decides the task is up
# to date, and the guest recipes are what actually writes the files. If
# they disagree, at least one of them is lying about what a build
# produces — which is how a fixture stops being generated without
# anything going red.
targets="$("$DRIVER" --list | tr '\n' ' ')"
case "$targets" in
    *geometry*populated*rich*compression*subvol*xattr*acl*nodatacow*commit*cow*split*pool*)
        ok "the driver lists every target in build order" ;;
    *) fail "the driver lists every target in build order: $targets" ;;
esac

# Every target the driver lists has a recipe of that name.
for target in $targets; do
    grep -q "^build_$target()" "$RECIPES" \
        || fail "guest-build-images.sh has a build_$target recipe"
done
ok "every target the driver lists has a guest recipe"

declared="$(awk '/^    generates:/ { inside = 1; next }
                 inside && /^    [a-z]/ { inside = 0 }
                 inside && /test-disks\// {
                     sub(/^ *- */, ""); sub(/^test-disks\//, ""); print
                 }' "$REPO/chores.yml" | sort -u)"
produced="$("$DRIVER" --artefacts | grep -E '\.img$' | sort -u)"

[ -n "$declared" ] && ok "chores.yml declares the fixtures it generates" \
                   || fail "chores.yml declares the fixtures it generates"

missing=""
for image in $produced; do
    case $'\n'"$declared"$'\n' in
        *$'\n'"$image"$'\n'*) ;;
        *) missing="$missing $image" ;;
    esac
done
[ -z "$missing" ] && ok "every image the builder produces is in chores.yml generates:" \
    || fail "every image the builder produces is in chores.yml generates:$missing"

extra=""
for image in $declared; do
    case $'\n'"$produced"$'\n' in
        *$'\n'"$image"$'\n'*) ;;
        *) extra="$extra $image" ;;
    esac
done
[ -z "$extra" ] && ok "chores.yml declares no image the builder does not produce" \
    || fail "chores.yml declares no image the builder does not produce:$extra"

# --- 6. --check names what is missing, rather than passing quietly. ------
#
# The empty tree is the case that matters: a checkout with no fixtures
# must fail the tier that needs them, naming the task that builds them.
empty="$(mktemp -d "$REPO/tmp/fixture-empty.XXXXXX")"
mkdir -p "$empty/test-disks"
cp "$DRIVER" "$empty/test-disks/"
out="$(cd "$empty" && bash test-disks/build-fixtures.sh --check 2>&1)"; rc=$?
rm -rf "$empty"
[ "$rc" = 1 ] && ok "--check on an empty tree exits 1" \
              || fail "--check on an empty tree exits 1 (got $rc)"
case "$out" in
    *"btrfs-default.img"*) ok "--check names a fixture that is missing" ;;
    *) fail "--check names a fixture that is missing: $out" ;;
esac
case "$out" in
    *"chore fixtures"*) ok "--check names the task that builds them" ;;
    *) fail "--check names the task that builds them: $out" ;;
esac
case "$out" in
    *"never skip"*) ok "--check says a missing fixture is not a skip" ;;
    *) fail "--check says a missing fixture is not a skip" ;;
esac

if [ "$fails" -gt 0 ]; then
    echo "FAIL  $fails fixture-builder violation(s)" >&2
    exit 1
fi
echo "PASS  the fixture builder refuses rather than skips, and its two halves agree"
