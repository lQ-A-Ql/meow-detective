#!/usr/bin/env bash
#
# trace-commit.sh — record the block writes of one live Btrfs commit.
#
# `docs/transaction-format.md` says of write ordering and barrier
# placement: "an I/O trace of a live commit; static images cannot show
# it". This is that trace.
#
# It is the one question about the write path that a fixture cannot
# answer. A finished filesystem shows WHAT was written; it never shows in
# what order, and never where the flushes went. Both are what a
# copy-on-write writer's crash-consistency rests on: the rule is
# supposed to be data, then checksums, then tree blocks leaf-to-root,
# then a barrier, then superblocks — and the document is explicit that
# the ordering is "reasoned rather than observed".
#
# # How it works
#
# blktrace watches the loop device the filesystem is mounted on, one
# `touch` and one `sync` are performed, and blkparse renders the events.
# What matters in the output:
#
#   W          a write, with its sector and size
#   FWFS/FF    a flush — the barrier, which is what orders everything
#              before it against everything after
#   FUA        force-unit-access: this write must reach the medium
#              before it is acknowledged, which is how a superblock is
#              made durable without a second flush
#
# # Reading the result
#
# Sectors are 512 bytes, so a byte offset is `sector * 512`. The
# superblock copies live at 64 KiB, 64 MiB and 256 GiB — sectors 128,
# 131072 and 536870912 — so a write to sector 128 is the primary
# superblock and is expected LAST, after a flush.
#
# # Where to run it
#
# In the fs-linux-test-harness guest, like every other thing here that
# needs a kernel and a loop device:
#
#   chore vm:up
#   chore vm:run -- bash /repo/scripts/trace-commit.sh
#   chore vm:down
#
# It is a diagnostic rather than a test — nothing in `chore test` calls
# it — but it obeys the same rule as the tests do, and for the same
# reason: `blktrace` needs a kernel, and the answer must not depend on
# which laptop asked. Its output lands in `tmp/`, which the guest and
# the host both see (the repository is mounted at `/repo`).
set -euo pipefail

OUT="${BTRFS_TRACE_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/tmp}"
SIZE="${BTRFS_FIXTURE_SIZE:-512M}"

SUDO=""
[ "$(id -u)" -eq 0 ] || SUDO="sudo"

for tool in mkfs.btrfs blktrace blkparse losetup; do
    command -v "$tool" >/dev/null || { echo "$tool not found" >&2; exit 1; }
done

mkdir -p "$OUT"
img="$OUT/btrfs-trace.img"
trace="$OUT/btrfs-commit.trace"
rm -f "$img" "$trace"

truncate -s "$SIZE" "$img"
mkfs.btrfs -f "$img" >/dev/null

# An explicit loop device rather than `mount -o loop`, because blktrace
# needs a device name to watch and the implicit one is not reported.
loop="$($SUDO losetup --find --show "$img")"
trap '$SUDO losetup -d "$loop" 2>/dev/null || true' EXIT

m="$(mktemp -d)"
# Mounted with no options. The transaction document's own measurement
# used `space_cache=v1`, but a filesystem mkfs.btrfs makes today has the
# free-space tree and the kernel refuses that option on it — so this
# traces the filesystem people actually have. The document already
# anticipates the difference: "Add to that: a free-space-tree leaf, a
# checksum-tree leaf and data blocks for data writes."
$SUDO mount "$loop" "$m"

# Settle everything the mount itself did, so the trace holds one commit
# and not the tail of another.
$SUDO sync
sleep 1

work="$(mktemp -d)"
$SUDO blktrace -d "$loop" -o trace -D "$work" &
tracer=$!
sleep 1

# The commit being measured: one file, then a sync to force it out.
echo "one commit" | $SUDO tee "$m/measured.txt" >/dev/null
$SUDO sync
sleep 1

$SUDO kill -INT "$tracer" 2>/dev/null || true
wait "$tracer" 2>/dev/null || true

$SUDO umount "$m"
rmdir "$m"

{
    echo "# One Btrfs commit, as the block layer saw it."
    echo "#"
    echo "# Recorded by scripts/trace-commit.sh. Only D events — the moment"
    echo "# each request was DISPATCHED to the device — because that is the"
    echo "# order the device sees, and the order is the whole question."
    echo "#"
    echo "# Sectors are 512 bytes, so a byte offset is sector * 512. The"
    echo "# superblock copies are at 64 KiB and 64 MiB: sectors 128 and"
    echo "# 131072. A 512 MiB filesystem has no third copy."
    echo "#"
    echo "# RWBS flags: W write, S sync, M metadata, F flush, A FUA."
    echo "# A flush carries no sector — it orders everything before it"
    echo "# against everything after."
    echo "#"
    echo "# action rwbs sector size"
    echo
    $SUDO blkparse -i trace -D "$work" 2>/dev/null \
        | awk '$6 == "D" { printf "%s %s %s %s\n", $6, $7, ($8 == "" ? "-" : $8), ($10 == "" ? "-" : $10) }'
} | $SUDO tee "$trace" >/dev/null

$SUDO rm -rf "$work"

echo "TRACED  $(grep -cvE '^#|^$' "$trace") events into $(basename "$trace")"
