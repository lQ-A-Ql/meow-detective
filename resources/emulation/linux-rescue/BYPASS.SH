#!/bin/sh
# =============================================================================
# BYPASS.SH -- Meow~Detective Linux rescue shadow patch (self-authored manual aid)
#
# SCOPE: Operates ONLY on a materialized emulation disk copy (COW overlay /
# disk.raw). The original E01 evidence source is read-only and is never mounted,
# read, written, or referenced by this script. Do not run this against a physical
# disk, a live system disk, or an unmodified evidence image.
#
# This script contains no third-party GPL/LGPL code and ships no OS components.
# The rescue environment (recovery ISO) must be supplied by the user under that
# ISO's own license; see README.md.
#
# Usage:
#   BYPASS.SH --disk /dev/vg/root [--payload /mnt/meow-payload] [--dry-run]
#   BYPASS.SH --disk /dev/sdb2 --verify-only
#
# Options:
#   -d, --disk <device>   Root partition or logical-volume device (required).
#   -p, --payload <dir>   Payload CD mount point; auto-probed if omitted.
#       --dry-run         Validate only; print the plan, do not mount or write.
#       --verify-only    Read-only check: compare on-disk shadow to SHADOW.NEW.
#   -h, --help            Show this help text.
# =============================================================================
set -eu

DISK=""
PAYLOAD=""
DRY_RUN=0
VERIFY_ONLY=0

info()  { printf 'meow-bypass: %s\n' "$*"; }
warn()  { printf 'meow-bypass: warning: %s\n' "$*" >&2; }
fail()  { printf 'meow-bypass: error: %s\n' "$*" >&2; exit 1; }

usage() {
    sed -n '2,20p' "$0"
}

# --- payload discovery -------------------------------------------------------

find_payload_dir() {
    for candidate in \
        /mnt/meow-payload /media/cdrom /media/cdrom0 /mnt/cdrom /cdrom; do
        if [ -f "$candidate/SHADOW.NEW" ] && [ -f "$candidate/BYPASS.JSON" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    set -- /run/media/*/* /media/*
    for candidate in "$@"; do
        [ -d "$candidate" ] || continue
        if [ -f "$candidate/SHADOW.NEW" ] && [ -f "$candidate/BYPASS.JSON" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

# Minimal BYPASS.JSON readers (string / number), no jq required.
json_string() {
    sed -n 's/.*"'"$2"'"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -n 1
}

json_number() {
    sed -n 's/.*"'"$2"'"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$1" | head -n 1
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$1" | awk '{print $NF}'
    else
        return 1
    fi
}

file_size() {
    wc -c < "$1" | tr -d ' '
}

# --- validation --------------------------------------------------------------

validate_payload() {
    [ -f "$PAYLOAD/SHADOW.NEW" ] || fail "payload directory missing SHADOW.NEW: $PAYLOAD"
    [ -s "$PAYLOAD/SHADOW.NEW" ] || fail "SHADOW.NEW is empty: $PAYLOAD/SHADOW.NEW"
    [ -f "$PAYLOAD/BYPASS.JSON" ] || fail "payload directory missing BYPASS.JSON: $PAYLOAD"
    [ -s "$PAYLOAD/BYPASS.JSON" ] || fail "BYPASS.JSON is empty: $PAYLOAD/BYPASS.JSON"

    declared_hash=$(json_string "$PAYLOAD/BYPASS.JSON" shadowSha256 || true)
    if [ -n "$declared_hash" ]; then
        actual_hash=$(sha256_of "$PAYLOAD/SHADOW.NEW") || \
            fail "cannot compute sha256 of SHADOW.NEW (install sha256sum, shasum, or openssl)"
        [ "$actual_hash" = "$declared_hash" ] || \
            fail "SHADOW.NEW sha256 mismatch: declared $declared_hash, actual $actual_hash"
    fi

    declared_size=$(json_number "$PAYLOAD/BYPASS.JSON" sizeBytes || true)
    if [ -n "$declared_size" ]; then
        actual_size=$(file_size "$PAYLOAD/SHADOW.NEW")
        [ "$actual_size" = "$declared_size" ] || \
            fail "SHADOW.NEW size mismatch: declared $declared_size bytes, actual $actual_size bytes"
    fi

    TARGET_USER=$(json_string "$PAYLOAD/BYPASS.JSON" username || true)
    BLOCKER=$(json_string "$PAYLOAD/BYPASS.JSON" blocker || true)
    FS_DECLARE=$(json_string "$PAYLOAD/BYPASS.JSON" filesystem || true)
    info "payload validated: user=${TARGET_USER:-unknown} blocker=${BLOCKER:-n/a} declared fs=${FS_DECLARE:-n/a}"
}

detect_fs() {
    blkid -o value -s TYPE "$1" 2>/dev/null || true
}

validate_device() {
    [ -n "$DISK" ] || fail "--disk <device> is required"
    [ -b "$DISK" ] || fail "target device does not exist or is not a block device: $DISK"
    FS_TYPE=$(detect_fs "$DISK")
    case "$FS_TYPE" in
        xfs|ext4) ;;
        "") fail "blkid reports no filesystem on $DISK; refusing to mount an unknown volume" ;;
        *) fail "unsupported filesystem '$FS_TYPE' on $DISK; only xfs and ext4 are accepted" ;;
    esac
    info "target device $DISK detected as $FS_TYPE"
}

# --- argument parsing --------------------------------------------------------

while [ $# -gt 0 ]; do
    case "$1" in
        -d|--disk)
            DISK="${2:-}"
            shift 2
            ;;
        -p|--payload)
            PAYLOAD="${2:-}"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --verify-only)
            VERIFY_ONLY=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'meow-bypass: error: unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$PAYLOAD" ]; then
    PAYLOAD=$(find_payload_dir) || fail "payload CD not found; pass --payload <mount point>"
fi

validate_payload
validate_device

# --- dry-run: validate and print the plan, touch nothing --------------------

if [ "$DRY_RUN" -eq 1 ]; then
    TS=$(date +%Y%m%d-%H%M%S)
    info "[dry-run] would mount -t $FS_TYPE -o ro $DISK and verify /etc/shadow exists"
    info "[dry-run] would back up original /etc/shadow to /tmp/meow-shadow.orig.$TS"
    info "[dry-run] would remount $DISK rw and install SHADOW.NEW (mode 000, owner root:shadow)"
    info "[dry-run] would sync, umount, remount ro, and compare size + first 32 bytes"
    info "[dry-run] no changes written"
    exit 0
fi

MNT=$(mktemp -d /tmp/meow-rescue.XXXXXX)
trap 'umount "$MNT" >/dev/null 2>&1 || true; rmdir "$MNT" 2>/dev/null || true' EXIT

# --- verify-only: read-only comparison against SHADOW.NEW -------------------

if [ "$VERIFY_ONLY" -eq 1 ]; then
    mount -t "$FS_TYPE" -o ro "$DISK" "$MNT"
    [ -f "$MNT/etc/shadow" ] || { umount "$MNT"; fail "$MNT/etc/shadow not found after read-only mount"; }
    on_disk_size=$(file_size "$MNT/etc/shadow")
    new_size=$(file_size "$PAYLOAD/SHADOW.NEW")
    head -c 32 "$MNT/etc/shadow" | od -An -tx1 | tr -s ' \n' ' ' > /tmp/meow-a.$$
    head -c 32 "$PAYLOAD/SHADOW.NEW" | od -An -tx1 | tr -s ' \n' ' ' > /tmp/meow-b.$$
    match=0
    if [ "$on_disk_size" = "$new_size" ] && cmp -s /tmp/meow-a.$$ /tmp/meow-b.$$; then
        match=1
    fi
    rm -f /tmp/meow-a.$$ /tmp/meow-b.$$
    umount "$MNT"
    if [ "$match" -eq 1 ]; then
        info "OK: on-disk /etc/shadow already matches SHADOW.NEW (size $new_size bytes)"
        exit 0
    fi
    fail "on-disk /etc/shadow does not match SHADOW.NEW (disk $on_disk_size bytes, payload $new_size bytes)"
fi

# --- apply: backup, rw install, sync, re-verify -----------------------------

TS=$(date +%Y%m%d-%H%M%S)
BACKUP="/tmp/meow-shadow.orig.$TS"

mount -t "$FS_TYPE" -o ro "$DISK" "$MNT"
[ -f "$MNT/etc/shadow" ] || { umount "$MNT"; fail "$MNT/etc/shadow not found after read-only mount"; }
cp "$MNT/etc/shadow" "$BACKUP"
umount "$MNT"
info "original shadow backed up to $BACKUP (rescue tmpfs; copy out before reboot if needed)"

mount -t "$FS_TYPE" -o rw "$DISK" "$MNT"
if command -v install >/dev/null 2>&1; then
    if getent group shadow >/dev/null 2>&1; then
        install -m 000 -o root -g shadow "$PAYLOAD/SHADOW.NEW" "$MNT/etc/shadow"
    else
        warn "shadow group not found; falling back to group 0 (root)"
        install -m 000 -o root -g 0 "$PAYLOAD/SHADOW.NEW" "$MNT/etc/shadow"
    fi
else
    warn "'install' unavailable; using cp + chmod/chown"
    cp "$PAYLOAD/SHADOW.NEW" "$MNT/etc/shadow"
    chmod 000 "$MNT/etc/shadow"
    chown root:root "$MNT/etc/shadow" 2>/dev/null || chown 0:0 "$MNT/etc/shadow"
fi
sync
umount "$MNT"

mount -t "$FS_TYPE" -o ro "$DISK" "$MNT"
on_disk_size=$(file_size "$MNT/etc/shadow")
new_size=$(file_size "$PAYLOAD/SHADOW.NEW")
head -c 32 "$MNT/etc/shadow" | od -An -tx1 | tr -s ' \n' ' ' > /tmp/meow-a.$$
head -c 32 "$PAYLOAD/SHADOW.NEW" | od -An -tx1 | tr -s ' \n' ' ' > /tmp/meow-b.$$
if [ "$on_disk_size" != "$new_size" ] || ! cmp -s /tmp/meow-a.$$ /tmp/meow-b.$$; then
    rm -f /tmp/meow-a.$$ /tmp/meow-b.$$
    umount "$MNT"
    fail "post-write check failed: disk $on_disk_size bytes, payload $new_size bytes (first 32 bytes differ)"
fi
rm -f /tmp/meow-a.$$ /tmp/meow-b.$$
umount "$MNT"

info "OK: /etc/shadow matches SHADOW.NEW ($new_size bytes)"
info "next steps:"
info "  1. Power off the rescue environment; boot the emulated VM from this disk copy."
info "  2. Log in as '${TARGET_USER:-the edited account}' with the temporary password '123456'."
info "  3. Original shadow backup (rescue tmpfs; copy out before reboot if needed): $BACKUP"
