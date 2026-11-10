#!/usr/bin/env bash
#
# build-fixtures.sh [target...]  build the test-disks/ fixtures
#                                (`chore fixtures`); name some to rebuild
#                                only those, e.g.
#                                `build-fixtures.sh split pool`
# build-fixtures.sh --check      exit 1 naming every fixture that is missing
# build-fixtures.sh --list       print the targets, in build order
# build-fixtures.sh --artefacts  print every file a full build produces
#
# THE HOST SIDE, AND IT DOES NOT BUILD ANYTHING. Every one of these
# fixtures needs the real kernel — most of them need a mount — so the
# work happens in the fs-linux-test-harness VM (the sibling checkout at
# ../fs-linux-test-harness, moved to its pinned ref by `chore
# siblings`): test-disks/guest-build-images.sh runs as root in the
# guest, writes finished artefacts into the shared directory, and this
# script moves them into test-disks/ on the host.
#
# That is the whole point of the migration this replaced. The old
# arrangement had two builders — one that ran in this repository's own
# VM for macOS developers, one that ran on the CI runner with `sudo
# mount -o loop` — and they disagreed: the compression and nodatacow
# images existed only on developers' machines, so their oracles found
# nothing in CI and reported success (#69). One builder, one kernel, one
# answer.
#
# THE FIXTURE LIST lives here (ARTEFACTS below) and in chores.yml's
# `fixtures` generates:, which must name the same files.
#
# The VM comes down when this script exits (FLTH_KEEP_VM=1 keeps it up
# for a quicker next run).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
DISKS="$REPO/test-disks"

# target -> the artefacts it produces, relative to test-disks/, in build
# order. Every name here is checked by --check and named in chores.yml.
TARGETS="geometry populated rich compression subvol xattr acl nodatacow commit cow split pool dirtylog cli"

artefacts_for() {
    case "$1" in
        geometry)
            local name
            for name in default node4k node16k csum-crc32c csum-xxhash csum-sha256 \
                        csum-blake2 single dup mixed; do
                echo "btrfs-$name.img"
                echo "btrfs-$name.superdump"
            done
            ;;
        populated)
            echo btrfs-deep4k.img; echo btrfs-deep4k.superdump
            echo btrfs-deep16k.img; echo btrfs-deep16k.superdump
            ;;
        rich)
            echo btrfs-rich.img; echo btrfs-rich.superdump
            ;;
        compression)
            local algo
            for algo in zlib lzo zstd; do
                echo "btrfs-comp-$algo.img"
                echo "btrfs-comp-$algo.superdump"
                echo "btrfs-comp-$algo.manifest"
                echo "btrfs-comp-$algo.compression"
            done
            ;;
        subvol)
            echo btrfs-subvol.img; echo btrfs-subvol.superdump; echo btrfs-subvol.manifest
            ;;
        xattr)
            echo btrfs-xattr.img; echo btrfs-xattr.superdump; echo btrfs-xattr.manifest
            ;;
        acl)
            local node
            for node in node4k node16k; do
                echo "btrfs-acl-$node.img"
                echo "btrfs-acl-$node.superdump"
                echo "btrfs-acl-$node.manifest"
            done
            ;;
        nodatacow)
            echo btrfs-nodatacow.img; echo btrfs-nodatacow.superdump
            echo snapshot/btrfs-nodatacow-snapshot.img
            ;;
        commit)
            local suffix n
            for suffix in "" -sha256-dup; do
                echo "btrfs-commit${suffix}.img"
                echo "btrfs-commit${suffix}.superdump"
                for n in 0 1 2 3 4 5 6; do
                    echo "btrfs-commit${suffix}-$n.super"
                done
            done
            ;;
        cow)
            local suffix which
            for suffix in "" -sha256-dup; do
                for which in before control after; do
                    echo "btrfs-cow-${which}${suffix}.img"
                done
            done
            ;;
        split)
            local suffix
            for suffix in "" -vary; do
                echo "btrfs-split${suffix}-before.img"
                echo "btrfs-split${suffix}-after.img"
                echo "btrfs-split${suffix}.txt"
            done
            ;;
        pool)
            echo btrfs-pool-a.img; echo btrfs-pool-b.img; echo btrfs-pool.manifest
            ;;
        dirtylog)
            echo dirtylog/btrfs-dirty-log.img
            ;;
        cli)
            echo cli/btrfs-cli.img; echo cli/btrfs-cli.superdump; echo cli/btrfs-cli.manifest
            ;;
        *)
            echo "build-fixtures: unknown target '$1'" >&2
            exit 2
            ;;
    esac
}

all_artefacts() {
    local target
    for target in $TARGETS; do artefacts_for "$target"; done
}

if [ "${1:-}" = "--list" ]; then
    printf '%s\n' $TARGETS
    exit 0
fi

# What a full build produces, for anything that has to agree with this
# list — chores.yml's `generates:`, and the shell test that compares the
# two (tests/scripts/fixture-builder.sh).
if [ "${1:-}" = "--artefacts" ]; then
    all_artefacts
    exit 0
fi

if [ "${1:-}" = "--check" ]; then
    gone=""
    total=0
    while read -r artefact; do
        total=$((total + 1))
        [ -f "$DISKS/$artefact" ] || gone="$gone $artefact"
    done <<EOF
$(all_artefacts)
EOF
    if [ -n "$gone" ]; then
        echo "fixtures missing from test-disks/:$gone" >&2
        echo "build them with 'chore fixtures' — tests never skip on a missing fixture." >&2
        exit 1
    fi
    echo "fixtures: all $total present in test-disks/"
    exit 0
fi

targets="$*"
[ -n "$targets" ] || targets="$TARGETS"

HARNESS="$REPO/../fs-linux-test-harness"
VM="$HARNESS/scripts/vm.sh"

if [ ! -x "$VM" ]; then
    echo "build-fixtures: the harness is not checked out at $HARNESS." >&2
    echo "                Run 'chore siblings' first." >&2
    exit 1
fi

cd "$REPO"
# shellcheck source=/dev/null
. "$HARNESS/scripts/vm-session.sh"

share="$("$VM" share)"
out="$share/fixtures"
# EMPTIED, NOT REPLACED. The share is mounted in the guest over 9p, and
# a VM that is already up (a previous run left it with FLTH_KEEP_VM, or
# this one is a retry) holds this directory by inode. Removing it on the
# host and making a new one leaves the guest writing into the deleted
# one: `cp` succeeds, and the `mv` beside it cannot find what it just
# wrote. Measured here, as `mv: cannot stat .../btrfs-default.img.partial`.
mkdir -p "$out"
find "$out" -mindepth 1 -delete

started=$(date +%s)
# The repository is mounted at /repo in the guest, so the recipes are
# already there: nothing is copied in, and there is exactly one copy of
# them to keep right.
"$VM" run "bash /repo/test-disks/guest-build-images.sh /share/fixtures $targets"

built=0
while IFS= read -r -d '' image; do
    rel="${image#"$out"/}"
    # Checked on the host rather than taken on the guest's word: every
    # image must carry the btrfs superblock magic (`_BHRfS_M` at byte
    # 0x40 of the superblock, which lives at 64 KiB).
    magic="$(dd if="$image" bs=1 skip=$((65536 + 0x40)) count=8 status=none)"
    if [ "$magic" != "_BHRfS_M" ]; then
        echo "build-fixtures: $rel has no btrfs superblock magic (got '$magic')" >&2
        exit 1
    fi
    mkdir -p "$(dirname "$DISKS/$rel")"
    cp --sparse=always "$image" "$DISKS/$rel.partial"
    mv -f "$DISKS/$rel.partial" "$DISKS/$rel"
    rm -f "$image"
    built=$((built + 1))
done < <(find "$out" -name '*.img' -print0)

moved=0
while IFS= read -r -d '' artefact; do
    rel="${artefact#"$out"/}"
    mkdir -p "$(dirname "$DISKS/$rel")"
    cp "$artefact" "$DISKS/$rel.partial"
    mv -f "$DISKS/$rel.partial" "$DISKS/$rel"
    rm -f "$artefact"
    moved=$((moved + 1))
done < <(find "$out" -type f ! -name '*.img' -print0)

[ "$built" -gt 0 ] || { echo "build-fixtures: the guest produced no images" >&2; exit 1; }
echo "build-fixtures: $built image(s) and $moved other artefact(s) in test-disks/ ($(( $(date +%s) - started ))s in the VM)"
