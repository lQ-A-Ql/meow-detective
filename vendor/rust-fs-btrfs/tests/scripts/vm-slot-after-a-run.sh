#!/usr/bin/env bash
#
# vm-slot-after-a-run.sh — a test run that boots the harness VM brings it
# down and gives the machine-wide VM slot back when it ends, however it
# ends (#141).
#
# THE LEAK. An oracle or kernel test boots the VM from inside its own
# process (`vm.sh up`, tests/support/src/oracle.rs), and the slot that
# boot takes is ONE SLOT FOR THE WHOLE MACHINE. Nothing the run owned
# gave it back: release was left to chore's `after_all` reaper, which
# only runs inside a chore invocation of THIS repository. A run that
# reached cargo any other way — scripts/test.sh, ../rust-fs-core/scripts/tier.sh, by hand
# — exited 0 with the VM idle and the slot recorded as held, and every
# other repository's VM work then queued behind it at 0% CPU, up to the
# guest's eight-hour idle deadline. That is a slow test to anyone
# watching, which is how it survived.
#
# What is driven here is the real scripts/test.sh and the real harness
# scripts (vm.sh, vm-slot.sh, vm-session.sh) from the pinned sibling,
# with only the harness's engine swapped for its own stub — so no VM, no
# KVM and no root. `cargo` is a stand-in that does what an oracle test
# process does: `vm.sh up`, then pass, fail, or wait to be killed.
#
# And the negative arm, because the obvious fix — release on exit,
# whoever holds it — is the defect #100 closed: a run must never free a
# slot another repository holds.
#
#   bash tests/scripts/vm-slot-after-a-run.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HARNESS="$REPO/../fs-linux-test-harness"
fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$(( fails + 1 )); }

# NOTHING SKIPS: without the harness sibling this proves nothing, so it
# fails and names the task that provides it.
for need in scripts/vm.sh scripts/vm-slot.sh scripts/vm-session.sh tests/stubs/engine.sh; do
    if [ ! -f "$HARNESS/$need" ]; then
        echo "not ok the fs-linux-test-harness sibling has no $need -- run 'chore siblings'" >&2
        echo "FAIL  vm-slot-after-a-run could not run" >&2
        exit 1
    fi
done

mkdir -p "$REPO/tmp"
sandbox="$(mktemp -d "$REPO/tmp/vm-slot-after-a-run.XXXXXX")"
trap 'rm -rf "$sandbox"' EXIT

# The layout test.sh resolves its sibling from: <dir>/repo beside
# <dir>/fs-linux-test-harness.
SUT="$sandbox/repo"
H="$sandbox/fs-linux-test-harness"
mkdir -p "$SUT/scripts" "$H" "$sandbox/bin" "$sandbox/stub"
cp "$REPO/scripts/test.sh" "$SUT/scripts/"
cp "$REPO/fs-linux-test-harness.toml" "$SUT/"
# The setup script runs on the host under the stub engine; it must do nothing.
printf '#!/usr/bin/env bash\nexit 0\n' > "$SUT/scripts/vm-setup.sh"
cp -R "$HARNESS/scripts" "$H/"
cp "$HARNESS/tests/stubs/engine.sh" "$H/scripts/lib/engine.sh"
host_tools() { printf '#!/usr/bin/env bash\nexit %s\n' "$1" > "$H/scripts/host-tools.sh"; }
host_tools 0

# What an oracle test process does with the VM, then how it ends.
cat > "$sandbox/bin/cargo" <<STUB
#!/usr/bin/env bash
if [ "\${CARGO_STUB:-}" = no-vm ]; then exit 0; fi
"$H/scripts/vm.sh" up >/dev/null 2>&1 || exit 1
case "\${CARGO_STUB:-pass}" in
    pass) exit 0 ;;
    fail) exit 101 ;;
    hang) sleep 30 & wait \$! ;;
esac
STUB
chmod +x "$sandbox/bin/cargo" "$SUT/scripts/test.sh" "$H/scripts/"*.sh

export PATH="$sandbox/bin:$PATH"
export FLTH_TEST_STUB="$sandbox/stub"
export FLTH_CACHE_DIR="$sandbox/cache"
export FLTH_STATE_DIR="$sandbox/state"
unset FLTH_CONFIG FLTH_KEEP_VM FLTH_GUEST FLTH_SLOT_WAIT FLTH_SLOT_BOOT_GRACE FS_BTRFS_TEST_TMPDIR

reset() { rm -rf "$FLTH_STATE_DIR/slot.lock"; echo absent > "$FLTH_TEST_STUB/state"; : > "$FLTH_TEST_STUB/calls"; }
slot() { "$H/scripts/vm-slot.sh" status 2>&1 | head -1; }
vm_state() { cat "$FLTH_TEST_STUB/state"; }
run() { (cd "$SUT" && scripts/test.sh --test oracle_stand_in) >/dev/null 2>&1; }

expect_released() {
    # expect_released <how the run ended>
    case "$(slot)" in
        "the VM slot is free") ok "$1: the slot is free afterwards" ;;
        *) fail "$1: the slot is free afterwards (status says: $(slot))" ;;
    esac
    [ "$(vm_state)" = stopped ] && ok "$1: and the VM it booted is down" \
                                || fail "$1: and the VM it booted is down (state: $(vm_state))"
}

# --- 1. A run that PASSES gives the slot back. ---------------------------
reset
CARGO_STUB=pass run; rc=$?
[ "$rc" = 0 ] && ok "a passing run exits 0" || fail "a passing run exits 0 (got $rc)"
expect_released "a passing run"

# --- 2. A run that FAILS gives it back, and keeps its own status. --------
reset
CARGO_STUB=fail run; rc=$?
[ "$rc" = 101 ] && ok "a failing run keeps cargo's status" || fail "a failing run keeps cargo's status (got $rc)"
expect_released "a failing run"

# --- 3. A run KILLED by a signal gives it back. ---------------------------
#
# Sent to the run's whole process group, as Ctrl-C or a job's timeout
# does. Job control gives the background run a group of its own.
reset
set -m
(cd "$SUT" && CARGO_STUB=hang exec scripts/test.sh --test oracle_stand_in) >/dev/null 2>&1 &
pid=$!
set +m
tries=0
until [ "$(vm_state)" = running ] || [ "$tries" -ge 200 ]; do
    sleep 0.05; tries=$(( tries + 1 ))
done
[ "$(vm_state)" = running ] && ok "the killed run had booted the VM first" \
                            || fail "the killed run had booted the VM first (state: $(vm_state))"
kill -TERM -- "-$pid" 2>/dev/null
wait "$pid"; rc=$?
[ "$rc" != 0 ] && ok "a run killed by TERM does not report success" \
               || fail "a run killed by TERM does not report success"
expect_released "a run killed by TERM"

# --- 4. A run never frees a slot ANOTHER repository holds. ---------------
#
# The other holder is fresh, so it is inside the boot grace and cannot be
# reclaimed; with no wait allowed, this run's boot is refused, and its
# teardown must leave that holder exactly where it was.
reset
mkdir -p "$FLTH_STATE_DIR/slot.lock"
printf '%s\t%s\t%s\t%s\n' "/elsewhere/vagrant" other-repository "$(date +%s)" tok-other \
    > "$FLTH_STATE_DIR/slot.lock/holder"
FLTH_SLOT_WAIT=0 CARGO_STUB=pass run; rc=$?
[ "$rc" != 0 ] && ok "a run refused the slot fails" || fail "a run refused the slot fails"
case "$(slot)" in
    "held by other-repository"*) ok "and the other repository still holds the slot" ;;
    *) fail "and the other repository still holds the slot (status says: $(slot))" ;;
esac

# --- 5. Where there is no VM to own, there is no session. ----------------
#
# In the guest (`chore test:vm`) the suite IS in the VM; on a host that
# cannot run one (CI's unit and aarch64 jobs) there is nothing to bring
# down. Neither may ask the engine anything, or the unit tier would fail
# on a teardown of a VM that could never exist.
reset
FLTH_GUEST=1 CARGO_STUB=no-vm run; rc=$?
[ "$rc" = 0 ] && [ ! -s "$FLTH_TEST_STUB/calls" ] \
    && ok "in the guest the run makes no harness call" \
    || fail "in the guest the run makes no harness call (rc $rc; calls: $(tr '\n' ' ' < "$FLTH_TEST_STUB/calls"))"
reset
host_tools 1
CARGO_STUB=no-vm run; rc=$?
[ "$rc" = 0 ] && [ ! -s "$FLTH_TEST_STUB/calls" ] \
    && ok "on a host that cannot run the VM the run makes no harness call" \
    || fail "on a host that cannot run the VM the run makes no harness call (rc $rc; calls: $(tr '\n' ' ' < "$FLTH_TEST_STUB/calls"))"
host_tools 0

if [ "$fails" -gt 0 ]; then
    echo "FAIL  $fails vm-slot-after-a-run violation(s)" >&2
    exit 1
fi
echo "PASS  a test run gives the VM slot back however it ends, and never frees another's"
