#!/usr/bin/env bash
#
# guest-build-images.sh <output-dir> <target>... — GUEST side: runs as
# root inside the fs-linux-test-harness VM (Debian), started by
# test-disks/build-fixtures.sh through `vm.sh run`.
#
# ONE FILE, BECAUSE THE RECIPES ARE ONE THING. This replaces nine
# scripts: scripts/build-fixtures-native.sh (which CI ran on the runner,
# with sudo), the eight scripts/build-*-fixtures.sh it could not cover,
# scripts/fixture-geometries.sh, and the eight scripts/vm-build-*.sh
# wrappers that ran the same recipes through this repository's own VM.
# Two builders meant two chances to build a different filesystem, and
# that happened: the compression and nodatacow images existed only on
# developers' machines, so their oracles found nothing in CI and
# reported success (#69).
#
# EVERY FIXTURE HERE NEEDS THE REAL KERNEL, and most of them need a
# mount: subvolumes and snapshots, extended attributes, `chattr +C`, a
# compressing mount, a leaf split caught in the act, a two-device pool.
# None of that can happen on a host that is not Linux, and none of it
# should happen on a CI runner, where `sudo mount -o loop` is a
# privilege this suite should not need and a macOS developer cannot
# have. It happens in the guest, which is the one place with a kernel
# we chose.
#
# Every image is built on the guest's own disk and copied to
# <output-dir> (on the share) only once it is complete and unmounted, so
# a failure half way never leaves a truncated image where the host picks
# fixtures up.
#
# DETERMINISM. What `mkfs.btrfs` writes carries a fresh UUID per build
# and the kernel stamps generation and mount times as it goes, so two
# builds are identical in STRUCTURE AND CONTENT rather than byte for
# byte. Tests compare against the superblock dump and the manifests
# taken from the same build, never across builds.
set -euo pipefail

[ $# -ge 2 ] || { echo "usage: guest-build-images.sh <output-dir> <target>..." >&2; exit 2; }
OUT="$1"
shift

TARGETS="geometry populated rich compression subvol xattr acl nodatacow commit cow split pool dirtylog cli"

# ARGUMENTS FIRST, ENVIRONMENT SECOND. A misspelt target is the caller's
# mistake and should be named as one wherever it is made; the root and
# tool checks below describe the guest, and reporting those first would
# tell someone who typed `nodatcow` on a workstation that they need a
# Debian VM.
for target in "$@"; do
    case " $TARGETS " in
        *" $target "*) ;;
        *)
            echo "guest-build-images.sh: unknown target '$target'" >&2
            echo "  one of: $TARGETS" >&2
            exit 2
            ;;
    esac
done

[ "$(id -u)" -eq 0 ] || { echo "guest-build-images.sh runs as root in the guest." >&2; exit 1; }

for tool in mkfs.btrfs btrfs setfattr getfattr setfacl getfacl chattr lsattr python3 sha256sum losetup; do
    command -v "$tool" >/dev/null || {
        echo "guest-build-images.sh: $tool is not in the guest." >&2
        echo "  It is installed by scripts/vm-setup.sh; 'chore vm:provision' applies it again." >&2
        exit 1
    }
done

WORK="$(mktemp -d /var/tmp/btrfs-fixtures.XXXXXX)"
MNT="$WORK/mnt"
mkdir -p "$MNT" "$OUT"
cleanup() {
    mountpoint -q "$MNT" && umount "$MNT" || true
    rm -rf "$WORK"
}
trap cleanup EXIT

# ---------------------------------------------------------------------
# THE GEOMETRY MATRIX — the one list the fixtures are built from.
#
# Each entry is "<name>:<mkfs.btrfs args>". The name becomes the fixture
# filename, so keep it filesystem-safe and stable — the oracle tests
# report failures by it.
#
# Geometries are chosen to move the fields most likely to be misread:
# node size changes every b-tree item offset, the checksum algorithm
# changes both the csum_type field and the width of every csum in the
# superblock and tree blocks, the profile flags change the chunk-tree
# layout, and mixed block groups fold data and metadata into one block
# group type.
# ---------------------------------------------------------------------
GEOMETRIES=(
    "default:"
    "node4k:-n 4096"
    "node16k:-n 16384"
    "csum-crc32c:--csum crc32c"
    "csum-xxhash:--csum xxhash"
    "csum-sha256:--csum sha256"
    "csum-blake2:--csum blake2"
    "single:-d single -m single"
    "dup:-d dup -m dup"
    "mixed:-M"
)

# Every geometry fixture is this size. Large enough to clear
# mkfs.btrfs's minimum device size with room for several block groups,
# small enough that ten of them are cheap to build and to copy out.
FIXTURE_SIZE=400M

# Populated fixtures. The geometries above are all freshly-made
# filesystems, and every tree on one of those is a single leaf. That
# leaves internal-node parsing, the KeyPtr layout and the descent loop
# itself exercised only by hand-built blocks — which this project does
# not count as validated. These are mounted and filled with enough files
# to push the fs tree above level 0 (both reach level 2).
# "<name>:<mkfs.btrfs args>:<file count>:<image size>".
POPULATED=(
    "deep4k:-n 4096:20000:2G"
    "deep16k:-n 16384:60000:2G"
)

# The compression algorithms btrfs defines. All three are decoded by
# entirely different code and only one of them can be got right by
# accident: zstd and zlib are ordinary streams of their format; LZO is
# wrapped in framing btrfs invented, and that framing only becomes
# visible in a file long enough to span several sectors.
COMPRESSION_ALGOS="zlib lzo zstd"

# The two commit/COW geometries. The default one, and a SHA-256 volume
# with DUP profiles — a different checksum width and a different chunk
# layout, which is what catches a writer that hardcoded either.
GEOMETRY_VARIANTS=(
    ":--csum crc32c"
    "-sha256-dup:--csum sha256 -d dup -m dup"
)

# ---------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------

# publish <file>... — move finished artefacts into the output directory,
# each through a .partial name so the host never sees half a file.
publish() {
    local f base
    for f in "$@"; do
        base="$(basename "$f")"
        cp --sparse=always "$f" "$OUT/$base.partial"
        mv -f "$OUT/$base.partial" "$OUT/$base"
        rm -f "$f"
    done
}

# dump_super <image> — the superblock dump beside the image. `-f` dumps
# every copy, not just the primary, which is what the mirror tests read.
dump_super() {
    btrfs inspect-internal dump-super -f "$1" > "${1%.img}.superdump"
}

note() { echo "[guest] $*"; }

# fs_tree_level <image> — the level of the fs tree's root, 0 for a leaf.
#
# THROUGH A FILE, NOT A PIPE. `awk ... exit` closes its input on the
# first matching line, the dump of a twenty-thousand-file tree is many
# megabytes, and the writer then takes SIGPIPE — which `set -o pipefail`
# turns into a failed build with status 141 and nothing to say for
# itself. Measured here, on the deep4k fixture.
fs_tree_level() {
    btrfs inspect-internal dump-tree -t 5 "$1" > "$WORK/tree-dump.txt"
    awk '/^leaf /{print 0; exit} /^node /{print $4; exit}' "$WORK/tree-dump.txt"
    rm -f "$WORK/tree-dump.txt"
}

# ---------------------------------------------------------------------
# geometry — one freshly-made filesystem per geometry, plus its dump.
#
# NOTHING IS SKIPPED. A geometry mkfs.btrfs rejects fails this build:
# a fixture that quietly stopped being generated is a hole in the gate
# that still reports green, and the suites that read it used to take
# their absent-fixture path and pass (#140).
# ---------------------------------------------------------------------
build_geometry() {
    local geom name args img
    for geom in "${GEOMETRIES[@]}"; do
        name="${geom%%:*}"
        args="${geom#*:}"
        img="$WORK/btrfs-$name.img"
        rm -f "$img"
        truncate -s "$FIXTURE_SIZE" "$img"
        # shellcheck disable=SC2086  # $args is a deliberate argument list
        mkfs.btrfs $args -f "$img" >/dev/null || {
            echo "guest-build-images: mkfs.btrfs refused the '$name' geometry (${args:-no args})" >&2
            echo "  A geometry that cannot be built is a hole in the gate, not a fixture to skip." >&2
            exit 1
        }
        dump_super "$img"
        publish "$img" "${img%.img}.superdump"
        note "built btrfs-$name"
    done
}

# ---------------------------------------------------------------------
# populated — mounted and filled until the fs tree is more than a leaf.
# ---------------------------------------------------------------------
build_populated() {
    local spec name rest args count size img level
    for spec in "${POPULATED[@]}"; do
        name="${spec%%:*}"; rest="${spec#*:}"
        args="${rest%%:*}"; rest="${rest#*:}"
        count="${rest%%:*}"; size="${rest##*:}"
        img="$WORK/btrfs-$name.img"
        rm -f "$img"
        truncate -s "$size" "$img"
        # shellcheck disable=SC2086
        mkfs.btrfs $args -f "$img" >/dev/null
        mount -o loop "$img" "$MNT"
        mkdir -p "$MNT/many"
        note "filling btrfs-$name with $count files"
        # xargs -P keeps this to seconds rather than minutes.
        seq 1 "$count" | xargs -P4 -I{} sh -c "echo {} > $MNT/many/f{}.txt"
        sync
        umount "$MNT"

        # The reason this fixture exists, checked rather than assumed: a
        # level-0 fs tree means the descent loop is still untested.
        level=$(fs_tree_level "$img")
        [ "${level:-0}" -ge 1 ] || {
            echo "guest-build-images: btrfs-$name's fs tree is level ${level:-?}" >&2
            echo "  $count files did not grow it past a single leaf, so it tests nothing new." >&2
            exit 1
        }
        dump_super "$img"
        publish "$img" "${img%.img}.superdump"
        note "built btrfs-$name ($count files, fs tree level $level)"
    done
}

# ---------------------------------------------------------------------
# rich — written through a COMPRESSING mount and holding a deliberately
# varied tree: a highly compressible file (so zstd actually engages), an
# incompressible one (so the same image still has a plain extent), a
# small file stored inline in its item, a sparse file that is nearly all
# holes, a symlink, and a nested directory.
#
# It is what proves the driver reads a compressed extent while still
# reading everything else on the same filesystem. Without the
# incompressible file, a driver that simply refused every read would
# pass the compression test just as well.
# ---------------------------------------------------------------------
build_rich() {
    local img="$WORK/btrfs-rich.img"
    rm -f "$img"
    truncate -s 600M "$img"
    mkfs.btrfs -f "$img" >/dev/null
    mount -o loop,compress=zstd "$img" "$MNT"
    python3 -c "print('the quick brown fox jumps over the lazy dog '*20000)" > "$MNT/compressed.txt"
    dd if=/dev/urandom of="$MNT/plain.bin" bs=1M count=2 status=none
    echo 'small inline' > "$MNT/inline.txt"
    truncate -s 8M "$MNT/sparse.bin"
    ln -s inline.txt "$MNT/link-short"
    mkdir -p "$MNT/sub/nested" && echo nested > "$MNT/sub/nested/file.txt"
    sync
    umount "$MNT"
    dump_super "$img"
    publish "$img" "${img%.img}.superdump"
    note "built btrfs-rich (compressing mount)"
}

# ---------------------------------------------------------------------
# compression — one filesystem per algorithm, each with a manifest the
# kernel generated.
#
# The manifest is the whole point: it records what Linux says each file
# contains, so the driver's decoders are checked against the encoder
# that produced the bytes rather than against themselves.
#
#   big.txt     compressible and many sectors long, the only way the LZO
#               segment framing shows up at all
#   small.txt   compressible but under one sector: the single-segment case
#   plain.bin   incompressible, so it stays an ordinary extent and the
#               plain path is checked beside the compressed one
#   inline.txt  small enough to live inline in its own item
#
# `compress=`, not `compress-force=`: the incompressible file must stay
# an ordinary extent, so that a driver which simply refused (or mangled)
# every read cannot pass by reading nothing.
# ---------------------------------------------------------------------
build_compression() {
    local algo img manifest record
    for algo in $COMPRESSION_ALGOS; do
        img="$WORK/btrfs-comp-$algo.img"
        manifest="$WORK/btrfs-comp-$algo.manifest"
        record="$WORK/btrfs-comp-$algo.compression"
        rm -f "$img" "$manifest" "$record"
        truncate -s 600M "$img"
        mkfs.btrfs -f "$img" >/dev/null

        mount -o "loop,compress=$algo" "$img" "$MNT"
        python3 -c "print('the quick brown fox jumps over the lazy dog '*40000)" > "$MNT/big.txt"
        python3 -c "print('ab'*200)" > "$MNT/small.txt"
        dd if=/dev/urandom of="$MNT/plain.bin" bs=1M count=2 status=none
        echo 'inline and compressible aaaaaaaaaaaaaaaaaaaaaaaa' > "$MNT/inline.txt"
        sync
        umount "$MNT"

        # What the kernel says each file holds, read back through its own
        # driver on a read-only mount so the image is not disturbed.
        mount -o loop,ro "$img" "$MNT"
        ( cd "$MNT"
          find . -mindepth 1 -type f | sort | while read -r p; do
              printf '%s\t%s\t%s\n' "${p#.}" "$(stat -c%s "$p")" \
                  "$(sha256sum "$p" | cut -d' ' -f1)"
          done
        ) > "$manifest"
        umount "$MNT"

        # Which compression types actually ended up on disk. If this does
        # not name the algorithm, the mount option was ignored and the
        # fixture is not testing what it claims to, so the build fails
        # here rather than in a test that reads it as a pass.
        btrfs inspect-internal dump-tree -t 5 "$img" 2>/dev/null |
            grep -o 'extent compression [0-9]* ([a-z]*)' | sort -u > "$record"
        grep -q "($algo)" "$record" || {
            echo "guest-build-images: btrfs-comp-$algo has no $algo-compressed extent on disk: $(tr '\n' ' ' < "$record")" >&2
            exit 1
        }
        dump_super "$img"
        publish "$img" "${img%.img}.superdump" "$manifest" "$record"
        note "built btrfs-comp-$algo"
    done
}

# ---------------------------------------------------------------------
# subvol — a filesystem with subvolumes and snapshots, and a manifest of
# what is in it.
#
# Every geometry fixture has exactly one subvolume: the default fs tree,
# objectid 5. That is not the shape a real btrfs filesystem has —
# subvolumes are how people use it, and a snapshot is how they back it up.
#
#   top        the default subvolume, with files of its own
#   sub        a subvolume beside it
#   sub/inner  a subvolume nested inside that one
#   snap       a snapshot of `sub`, taken before `sub` is written to again
#   rosnap     a read-only snapshot of `sub`
#
# `sub` gains a file AFTER `snap` is taken, so the two are genuinely
# divergent rather than identical — a driver that resolved a snapshot to
# its parent's current tree would read the extra file and pass anything
# that only counted names.
#
# `btrfs subvolume list` is the reference answer, recorded beside the
# image while the filesystem is still mounted, so the driver's own
# enumeration is compared against what btrfs-progs reports rather than
# against what the driver believes.
# ---------------------------------------------------------------------
build_subvol() {
    local img="$WORK/btrfs-subvol.img" manifest="$WORK/btrfs-subvol.manifest" s
    rm -f "$img" "$manifest"
    truncate -s 512M "$img"
    mkfs.btrfs -f "$img" >/dev/null
    mount -o loop "$img" "$MNT"

    mkdir -p "$MNT/top"
    echo "in the default subvolume" > "$MNT/top/a.txt"
    btrfs subvolume create "$MNT/sub" >/dev/null
    echo "in sub" > "$MNT/sub/b.txt"
    btrfs subvolume create "$MNT/sub/inner" >/dev/null
    echo "in sub/inner" > "$MNT/sub/inner/c.txt"
    sync
    btrfs subvolume snapshot "$MNT/sub" "$MNT/snap" >/dev/null
    btrfs subvolume snapshot -r "$MNT/sub" "$MNT/rosnap" >/dev/null
    sync
    # Now make `sub` differ from its snapshots.
    echo "added after the snapshot" > "$MNT/sub/after.txt"
    sync

    {
        echo "# btrfs subvolume list -pcgu, taken from the mounted filesystem."
        echo "# Columns as btrfs-progs printed them; this is the reference the"
        echo "# driver's own enumeration is compared against."
        btrfs subvolume list -pcgu "$MNT"
        echo "# --- what each subvolume holds, one line per path"
        for s in top sub sub/inner snap rosnap; do
            [ -e "$MNT/$s" ] || continue
            printf 'contains %s:' "$s"
            find "$MNT/$s" -maxdepth 1 -type f -printf ' %f' 2>/dev/null || true
            echo
        done
    } > "$manifest"

    umount "$MNT"
    dump_super "$img"
    publish "$img" "${img%.img}.superdump" "$manifest"
    note "built btrfs-subvol"
}

# ---------------------------------------------------------------------
# xattr — a filesystem carrying extended attributes, and a manifest of
# what the kernel says is on it.
#
#   plain.txt     four attributes, covering the value shapes that decode
#                 differently: ordinary text, a ZERO-LENGTH value (a real
#                 value, and not the same as the attribute being absent),
#                 arbitrary binary including NUL bytes, and one long
#                 enough that its item is mostly value.
#   collide.txt   TWO NAMES THAT HASH TO THE SAME KEY. Btrfs files an
#                 attribute under (ino, 24, crc32c(~1, name)), so
#                 colliding names share one item and their records are
#                 packed end to end inside it. A driver that read only
#                 the first record would return one attribute and no
#                 error, which is the failure this fixture exists to
#                 catch. The two names below were found by searching the
#                 hash; both must come back.
#   bare.txt      no attributes at all, so an empty list is distinguished
#                 from a failure to look.
#   dir/          a directory with an attribute, since directories carry
#                 them too and nothing else here would prove it.
#   dir/inner.txt an attribute in the `trusted` namespace, which only
#                 root may set. Btrfs stores the prefix as part of the
#                 name, so this must come back spelled in full.
#
# The two colliding names hash to 0x5bc5594f under crc32c(~1, name).
# Regenerating the pair means searching the same hash for another
# collision — the numbers are not arbitrary and cannot be tidied up.
# ---------------------------------------------------------------------
build_xattr() {
    local img="$WORK/btrfs-xattr.img" manifest="$WORK/btrfs-xattr.manifest"
    local collide_a="user.tag1371838" collide_b="user.tag2000402"
    rm -f "$img" "$manifest"
    truncate -s 512M "$img"
    mkfs.btrfs -f "$img" >/dev/null
    mount -o loop "$img" "$MNT"

    echo "plain" > "$MNT/plain.txt"
    setfattr -n user.colour -v "blue" "$MNT/plain.txt"
    # -v "" is a zero-length value, not an unset attribute.
    setfattr -n user.empty -v "" "$MNT/plain.txt"
    # 0x… is setfattr's hex form: NUL, high bytes, a newline — the things
    # a value must survive being.
    setfattr -n user.binary -v 0x000102ff7f0a00 "$MNT/plain.txt"
    setfattr -n user.long -v "$(printf 'x%.0s' $(seq 1 2000))" "$MNT/plain.txt"

    echo "collide" > "$MNT/collide.txt"
    setfattr -n "$collide_a" -v "first of the pair" "$MNT/collide.txt"
    setfattr -n "$collide_b" -v "second of the pair" "$MNT/collide.txt"

    echo "bare" > "$MNT/bare.txt"

    mkdir -p "$MNT/dir"
    setfattr -n user.on-a-directory -v "yes" "$MNT/dir"
    echo "inner" > "$MNT/dir/inner.txt"
    setfattr -n trusted.root-only -v "only root may set this" "$MNT/dir/inner.txt"
    sync

    # `-e hex` so a binary value survives being written down; `-d -m -`
    # so every namespace is dumped, not just `user`; paths relative to
    # the mount point so the manifest carries no temporary name.
    {
        echo "# getfattr -R -d -m - -e hex, from the mounted filesystem."
        echo "# The reference the driver's own listing is compared against."
        echo "# An attribute with a zero-length value prints as a bare name,"
        echo "# with no '=' — that is getfattr's spelling, not a truncation."
        (cd "$MNT" && getfattr -R -d -m - -e hex .)
    } > "$manifest"

    umount "$MNT"
    dump_super "$img"
    publish "$img" "${img%.img}.superdump" "$manifest"
    note "built btrfs-xattr"
}

# ---------------------------------------------------------------------
# acl — POSIX ACLs, set by setfacl and inherited by the kernel, on a
# 4 KiB-node and a 16 KiB-node filesystem, and the kernel's own account
# of every one of them.
#
# An ACL is an extended attribute (`system.posix_acl_access`, or
# `system.posix_acl_default` on a directory) whose value has a format of
# its own: a 4-byte header carrying version 2, then one 8-byte entry per
# ACL entry. The value is the kernel's to write, so everything here is
# set through setfacl or created under a default ACL and left for the
# kernel to fill in.
#
#   minimal.txt       an ACL setfacl was given that the mode bits already
#                     say. The kernel stores NO attribute for it, and a
#                     driver must not invent one.
#   one-user.txt      the smallest ACL that is stored: one named user,
#                     which brings a mask with it — five entries.
#   mixed.txt         named users AND named groups, so the tags sort.
#   sweep/            one file per entry count, from five up to the
#                     largest the node size admits (see below).
#   over.txt, over.d  one entry past that largest: the kernel refuses it,
#                     so these carry no ACL at all.
#   inherit/          a directory with access and default ACLs; the file,
#                     the subdirectory and the file inside that were
#                     CREATED under it, so their ACLs are the kernel's
#                     inheritance, not anything setfacl was told.
#   maxdir/           a directory whose access ACL AND default ACL are
#                     both the largest the node size admits — two items
#                     that cannot share a leaf on a 4 KiB node — and a
#                     file and a directory created under it, which
#                     inherit that largest default.
#
# THE BOUNDARY. Btrfs keeps every attribute inline, in one item of one
# leaf; there is no out-of-line value to spill into. So the largest ACL
# is set by the node size: the kernel refuses a value whose name and
# value would not fit an item in an otherwise empty leaf, with ENOSPC.
# That largest count is FOUND here, by bisecting on what setfacl is
# allowed to set, rather than computed — the arithmetic is the test's
# to check against it — and recorded in the manifest with the refusal
# the next count up met.
#
# The manifest is `getfattr -R -d -m - -e hex`, as the xattr fixture's,
# preceded by `#`-lines the xattr parser skips:
#   # acl-nodesize: <bytes>
#   # acl-max: <access|default> <largest accepted> refused <next>: <setfacl's error>
#   # acl-count: <path> access <n> default <n>      (getfacl's entry counts)
#   # acl-value: <path> <name> 0x<hex>              (getfattr -n <name>)
# ---------------------------------------------------------------------

# acl_spec <total> [default] — an ACL of exactly <total> entries for
# `setfacl -M`: the four a named entry requires (owner, owning group,
# mask, other) and <total - 4> named users with distinct ids.
acl_spec() {
    local total="$1" prefix="${2:+default:}" i
    printf '%suser::rw-\n%sgroup::r--\n%smask::rw-\n%sother::r--\n' \
        "$prefix" "$prefix" "$prefix" "$prefix"
    for ((i = 0; i < total - 4; i++)); do
        printf '%suser:%d:r--\n' "$prefix" $((10000 + i))
    done
}

# acl_set <path> <total> [default] — set that ACL; setfacl's error, if
# any, is left in $WORK/acl-err. `-n`: the spec's mask stands as given,
# so the entry count is exactly <total>.
acl_set() {
    acl_spec "$2" "${3:-}" > "$WORK/acl-spec"
    setfacl -n -M "$WORK/acl-spec" "$1" 2> "$WORK/acl-err"
}

# acl_max <path> [default] — the largest entry count the kernel accepts
# on <path>, by bisection between a count it must accept and one it must
# refuse. Either end behaving otherwise fails the build: the search
# would then be measuring something other than the boundary.
acl_max() {
    local path="$1" kind="${2:-}" lo=5 hi=8200 mid
    setfacl -b "$path"
    if ! acl_set "$path" "$lo" "$kind"; then
        echo "guest-build-images.sh: a $lo-entry ${kind:-access} ACL was refused: $(cat "$WORK/acl-err")" >&2
        return 1
    fi
    setfacl -b "$path"
    if acl_set "$path" "$hi" "$kind"; then
        echo "guest-build-images.sh: a $hi-entry ${kind:-access} ACL was accepted — larger than any value an attribute can hold" >&2
        return 1
    fi
    while (( hi - lo > 1 )); do
        mid=$(( (lo + hi) / 2 ))
        setfacl -b "$path"
        if acl_set "$path" "$mid" "$kind"; then lo=$mid; else hi=$mid; fi
    done
    setfacl -b "$path"
    echo "$lo"
}

# acl_refused <path> <total> [default] — the ACL must be refused, and
# refused for want of room. Prints setfacl's error.
acl_refused() {
    if acl_set "$@"; then
        echo "guest-build-images.sh: a $2-entry ${3:-access} ACL on $1 was accepted, one past the largest" >&2
        return 1
    fi
    if ! grep -q 'No space left on device' "$WORK/acl-err"; then
        echo "guest-build-images.sh: the $2-entry ${3:-access} ACL was refused for another reason: $(cat "$WORK/acl-err")" >&2
        return 1
    fi
    # Relative to the mount point, so the manifest names no temporary path.
    tr '\n' ' ' < "$WORK/acl-err" | sed -e "s|$MNT/|/|g" -e 's/ *$//'
}

build_acl() {
    local geometry name nodesize img manifest max_access max_default
    local refused_access refused_default n p a d attr v
    for geometry in "node4k:4096" "node16k:16384"; do
        name="${geometry%%:*}"
        nodesize="${geometry#*:}"
        img="$WORK/btrfs-acl-$name.img"
        manifest="$WORK/btrfs-acl-$name.manifest"
        rm -f "$img" "$manifest"
        truncate -s 256M "$img"
        mkfs.btrfs -f -n "$nodesize" "$img" >/dev/null
        # `acl` is btrfs's default, named anyway so a changed default
        # cannot quietly build a fixture with no ACLs in it.
        mount -o loop,acl "$img" "$MNT"

        # The boundary, measured on scratch paths removed afterwards.
        touch "$MNT/scratch"
        max_access="$(acl_max "$MNT/scratch")"
        rm -f "$MNT/scratch"
        mkdir "$MNT/scratch.d"
        max_default="$(acl_max "$MNT/scratch.d" default)"
        rmdir "$MNT/scratch.d"

        echo minimal > "$MNT/minimal.txt"
        setfacl -m u::rw-,g::r--,o::r-- "$MNT/minimal.txt"
        echo one > "$MNT/one-user.txt"
        setfacl -m u:1000:rw- "$MNT/one-user.txt"
        echo mixed > "$MNT/mixed.txt"
        setfacl -m u:1001:r--,u:1002:rw-,g:2001:r--,g:2002:rwx "$MNT/mixed.txt"

        mkdir "$MNT/sweep"
        n=5
        while (( n < max_access - 1 )); do
            echo "$n" > "$MNT/sweep/entries-$n.txt"
            acl_set "$MNT/sweep/entries-$n.txt" "$n"
            n=$(( n * 2 ))
        done
        for n in $(( max_access - 1 )) "$max_access"; do
            echo "$n" > "$MNT/sweep/entries-$n.txt"
            acl_set "$MNT/sweep/entries-$n.txt" "$n"
        done

        echo over > "$MNT/over.txt"
        refused_access="$(acl_refused "$MNT/over.txt" $(( max_access + 1 )))"
        mkdir "$MNT/over.d"
        refused_default="$(acl_refused "$MNT/over.d" $(( max_default + 1 )) default)"

        mkdir "$MNT/inherit"
        setfacl -m u:1000:rwx,g:2000:r-x,d:u:1000:rwx,d:g:2000:r-x,d:o::--- "$MNT/inherit"
        echo file > "$MNT/inherit/file.txt"
        mkdir "$MNT/inherit/sub"
        echo nested > "$MNT/inherit/sub/nested.txt"

        mkdir "$MNT/maxdir"
        acl_set "$MNT/maxdir" "$max_access"
        acl_set "$MNT/maxdir" "$max_default" default
        echo child > "$MNT/maxdir/child.txt"
        mkdir "$MNT/maxdir/child.d"
        sync

        {
            echo "# POSIX ACLs on a ${nodesize}-byte-node btrfs, as the kernel reports them."
            echo "# acl-nodesize: $nodesize"
            echo "# acl-max: access $max_access refused $(( max_access + 1 )): $refused_access"
            echo "# acl-max: default $max_default refused $(( max_default + 1 )): $refused_default"
            ( cd "$MNT" && find . -mindepth 1 | sort | while read -r p; do
                a="$(getfacl -cnE "$p" | grep -cv -e '^default:' -e '^$' || true)"
                d="$(getfacl -cnE "$p" | grep -c '^default:' || true)"
                echo "# acl-count: ${p#.} access $a default $d"
                for attr in system.posix_acl_access system.posix_acl_default; do
                    v="$(getfattr -e hex -n "$attr" "$p" 2>/dev/null | sed -n "s/^$attr=//p" || true)"
                    [ -z "$v" ] || echo "# acl-value: ${p#.} $attr $v"
                done
              done )
            echo "# getfattr -R -d -m - -e hex, from the mounted filesystem."
            (cd "$MNT" && getfattr -R -d -m - -e hex .)
        } > "$manifest"

        # The reference must actually carry ACLs, from BOTH readings of
        # them, or every comparison against it compares nothing.
        for attr in system.posix_acl_access system.posix_acl_default; do
            grep -q "^# acl-value: .* $attr 0x" "$manifest" \
                || { echo "guest-build-images.sh: getfattr -n reported no $attr on btrfs-acl-$name" >&2; exit 1; }
            grep -q "^$attr=0x" "$manifest" \
                || { echo "guest-build-images.sh: getfattr -d -m - listed no $attr on btrfs-acl-$name" >&2; exit 1; }
        done

        umount "$MNT"
        dump_super "$img"
        publish "$img" "${img%.img}.superdump" "$manifest"
        note "built btrfs-acl-$name (largest ACL: $max_access access, $max_default default entries)"
    done
}

# ---------------------------------------------------------------------
# nodatacow — one filesystem holding a checksummed file and a file with
# no checksums, so a driver's data-checksum path can be judged on both.
#
#   nc/inplace.bin  written into a `chattr +C` directory, so the kernel
#                   marks it NODATACOW, which implies NODATASUM: it has
#                   no checksums at all. A driver that treated a missing
#                   digest as damage would refuse this file, which the
#                   kernel reads happily — the failure that is worse for
#                   a user than the one being guarded against.
#   cow.bin         an ordinary file beside it, so every sector has a
#                   digest in the csum tree. Without it a driver that
#                   simply refused every read would pass the first half.
#
# And a second image: the same file AFTER A SNAPSHOT (#63). A snapshot
# does NOT raise a data extent's reference count when the subvolume's
# tree is taller than one leaf — `btrfs_copy_root` adds a reference to
# each block the root points at, and those are tree blocks, not data. So
# `nc/inplace.bin`'s extent still reads `refs 1` while the snapshot
# reads the same bytes. The kernel tells the two apart with the fs
# tree's `last_snapshot`. The 3000 empty files in `many/` are only there
# so the fs tree is a node before the snapshot is taken, and the build
# checks that it is.
#
# It goes in its own directory: the suites that walk every image in
# test-disks/ assume what an ordinary volume holds, and a snapshot
# breaks that on purpose — the blocks under the shared root carry two
# inline references, a 42-byte METADATA_ITEM where every other image has
# 33.
# ---------------------------------------------------------------------
build_nodatacow() {
    local img="$WORK/btrfs-nodatacow.img" level
    rm -f "$img"
    truncate -s 512M "$img"
    mkfs.btrfs -f "$img" >/dev/null
    mount -o loop "$img" "$MNT"
    mkdir "$MNT/nc"
    chattr +C "$MNT/nc"
    dd if=/dev/urandom of="$MNT/nc/inplace.bin" bs=4096 count=64 status=none
    dd if=/dev/urandom of="$MNT/cow.bin" bs=4096 count=64 status=none
    # The flags as the kernel recorded them, checked rather than assumed:
    # a build where chattr silently did nothing would produce two
    # ordinary files and an oracle that compares nothing.
    lsattr "$MNT/nc/inplace.bin" "$MNT/cow.bin"
    lsattr -d "$MNT/nc" | grep -q 'C' || {
        echo "guest-build-images: the nc/ directory is not NODATACOW — chattr +C did not take" >&2
        umount "$MNT"
        exit 1
    }
    sync
    umount "$MNT"
    dump_super "$img"
    publish "$img" "${img%.img}.superdump"
    note "built btrfs-nodatacow"

    mkdir -p "$OUT/snapshot"
    img="$WORK/btrfs-nodatacow-snapshot.img"
    rm -f "$img"
    truncate -s 512M "$img"
    mkfs.btrfs -f "$img" >/dev/null
    mount -o loop "$img" "$MNT"
    mkdir "$MNT/nc" "$MNT/many"
    chattr +C "$MNT/nc"
    dd if=/dev/urandom of="$MNT/nc/inplace.bin" bs=4096 count=64 status=none
    (cd "$MNT/many" && seq -f 'f%05g' 1 3000 | xargs touch)
    sync
    btrfs subvolume snapshot -r "$MNT" "$MNT/snap" >/dev/null
    lsattr "$MNT/nc/inplace.bin"
    sync
    umount "$MNT"
    level=$(fs_tree_level "$img")
    [ "${level:-0}" -ge 1 ] || {
        echo "guest-build-images: the fs tree is level ${level:-?}, so the snapshot raised every data reference" >&2
        exit 1
    }
    cp --sparse=always "$img" "$OUT/snapshot/btrfs-nodatacow-snapshot.img.partial"
    mv -f "$OUT/snapshot/btrfs-nodatacow-snapshot.img.partial" \
        "$OUT/snapshot/btrfs-nodatacow-snapshot.img"
    rm -f "$img"
    note "built btrfs-nodatacow-snapshot (fs tree level $level)"
}

# ---------------------------------------------------------------------
# commit — the superblock before and after each of six commits.
#
# The superblock is the commit point. Everything a transaction writes is
# invisible until the superblock names the new root, and a writer that
# gets one field of it wrong produces a filesystem the kernel either
# refuses or, worse, mounts against the wrong tree.
#
# SIX COMMITS RATHER THAN ONE: the backup slot index is
# `(generation - 1) mod 4`, so a single commit exercises one slot out of
# four and a writer that hardcoded a slot would pass. Six wraps the ring
# and then some.
#
# BOTH GEOMETRIES COEXIST, under their own suffix. They used to be built
# one after the other under the SAME names, with the oracle run in
# between — which works only in a workflow that interleaves builds and
# tests, and is exactly what a one-task `chore fixtures` cannot do.
# Suffixing them means the suite reads both in one run.
# ---------------------------------------------------------------------
build_commit() {
    local variant suffix args img n gen root commits=6
    for variant in "${GEOMETRY_VARIANTS[@]}"; do
        suffix="${variant%%:*}"
        args="${variant#*:}"
        img="$WORK/btrfs-commit${suffix}.img"
        rm -f "$img" "$WORK"/btrfs-commit"${suffix}"-*.super
        truncate -s 512M "$img"
        # shellcheck disable=SC2086
        mkfs.btrfs $args -f "$img" >/dev/null

        # The superblock as mkfs left it: the "before" of the first
        # commit. At 64 KiB, not at the start of the device — offset 0 is
        # empty, and dumping it gives 4096 zero bytes that read as a
        # generation of 0 and a root of 0, which looks like a filesystem
        # rather than like a mistake.
        dd if="$img" of="$WORK/btrfs-commit${suffix}-0.super" bs=4096 skip=16 count=1 status=none

        for n in $(seq 1 "$commits"); do
            mount -o loop "$img" "$MNT"
            # One file per commit, so each transaction has something to
            # write and the fs tree genuinely changes.
            echo "commit $n" > "$MNT/file-$n.txt"
            sync
            # Unmounted rather than only synced: a mounted filesystem's
            # superblock on disk lags what is in memory, and the point
            # here is what a committed superblock looks like.
            umount "$MNT"
            dd if="$img" of="$WORK/btrfs-commit${suffix}-$n.super" bs=4096 skip=16 count=1 status=none
        done

        dump_super "$img"
        publish "$img" "${img%.img}.superdump" "$WORK"/btrfs-commit"${suffix}"-*.super
        note "built btrfs-commit${suffix}: $((commits + 1)) superblocks, $commits commits apart ($args)"
        for n in $(seq 0 "$commits"); do
            local f="$OUT/btrfs-commit${suffix}-$n.super"
            # generation is at 0x048, root at 0x050, little-endian.
            gen=$(od -An -tu8 -j 72 -N 8 "$f" | tr -d ' ')
            root=$(od -An -tu8 -j 80 -N 8 "$f" | tr -d ' ')
            printf '[guest]   %s  generation %-4s root %s\n' "$(basename "$f")" "$gen" "$root"
        done
    done
}

# ---------------------------------------------------------------------
# cow — one filesystem, before and after one change.
#
# The remaining piece of the write path is the recursion: recording an
# allocation modifies the extent tree, which itself lives in allocated
# blocks. Reasoning about how the kernel breaks that cycle is how a
# writer ends up implementing something plausible and wrong. So it is
# measured instead: a WHOLE IMAGE before and after a single minimal
# metadata change, so every block the kernel rewrote can be identified
# and every item it changed can be diffed.
#
#   btrfs-cow-before.img    straight after mkfs, mounted and unmounted
#                           once so the first-mount feature write is
#                           already done and does not pollute the diff
#   btrfs-cow-control.img   mounted and unmounted again, CHANGING NOTHING
#   btrfs-cow-after.img     one `touch` and one `sync` later
#
# The control is what makes the measurement mean anything. Mounting a
# filesystem read-write commits by itself, so a before/after pair around
# a `touch` contains the touch AND whatever a bare mount cycle does.
# Without a pair that did nothing, every one of those writes would be
# attributed to creating the file.
# ---------------------------------------------------------------------
build_cow() {
    local variant suffix args before control after f gen root used
    for variant in "${GEOMETRY_VARIANTS[@]}"; do
        suffix="${variant%%:*}"
        args="${variant#*:}"
        before="$WORK/btrfs-cow-before${suffix}.img"
        control="$WORK/btrfs-cow-control${suffix}.img"
        after="$WORK/btrfs-cow-after${suffix}.img"
        rm -f "$before" "$control" "$after"
        truncate -s 512M "$before"
        # shellcheck disable=SC2086
        mkfs.btrfs $args -f "$before" >/dev/null

        # Settle the first-mount writes so they are not part of the diff.
        mount -o loop "$before" "$MNT"
        umount "$MNT"

        cp --sparse=always "$before" "$control"
        cp --sparse=always "$before" "$after"

        # The control: the same mount cycle, changing nothing.
        mount -o loop "$control" "$MNT"
        sync
        umount "$MNT"

        # The change under study: one empty file, one sync. The smallest
        # metadata transaction there is.
        mount -o loop "$after" "$MNT"
        touch "$MNT/one"
        sync
        umount "$MNT"

        for f in "$before" "$control" "$after"; do
            gen=$(od -An -tu8 -j $((65536 + 72)) -N 8 "$f" | tr -d ' ')
            root=$(od -An -tu8 -j $((65536 + 80)) -N 8 "$f" | tr -d ' ')
            used=$(od -An -tu8 -j $((65536 + 120)) -N 8 "$f" | tr -d ' ')
            printf '[guest]   %-32s generation %-4s root %-12s bytes_used %s\n' \
                "$(basename "$f")" "$gen" "$root" "$used"
        done
        publish "$before" "$control" "$after"
        note "built btrfs-cow${suffix}: before, control (no change) and after (one touch) ($args)"
    done
}

# ---------------------------------------------------------------------
# split — one leaf split, caught either side.
#
# `src/leaf_edit.rs` refuses an item that will not fit rather than
# splitting the leaf, because where the kernel puts the boundary is a
# policy. Measuring the leaves of an existing filesystem showed the
# median is 91-98% FULL, so it is plainly not "half" — but a
# distribution says what the results look like, not what the rule is.
#
# This catches the event itself. Files are added one at a time, each
# committed, and after each the fs tree's leaves are counted from the
# reference tool's own dump. When the count goes up, the previous image
# is the "before" and the current one is the "after".
#
# The `-vary` pair makes the items WILDLY different sizes, by
# alternating a near-maximum filename with a one-character one. That is
# the experiment that separates the two candidate rules: a split at half
# the ITEM COUNT does not care about sizes, and a split at half the
# BYTES lands somewhere else entirely once the items are uneven. With
# every item the same size the two agree and the measurement says
# nothing.
# ---------------------------------------------------------------------
build_split() {
    local vary suffix work prev before after last now i name split_at max=400
    for vary in 0 1; do
        suffix=""
        [ "$vary" = 1 ] && suffix="-vary"
        work="$WORK/btrfs-split-work.img"
        prev="$WORK/btrfs-split-prev.img"
        before="$WORK/btrfs-split${suffix}-before.img"
        after="$WORK/btrfs-split${suffix}-after.img"
        rm -f "$work" "$prev" "$before" "$after"

        truncate -s 512M "$work"
        # The smallest nodesize btrfs accepts, so a leaf fills quickly.
        mkfs.btrfs -n 4096 -f "$work" >/dev/null

        # Settle the first-mount writes before anything is measured.
        mount -o loop "$work" "$MNT"
        umount "$MNT"

        last=$(btrfs inspect-internal dump-tree -t 5 "$work" 2>/dev/null | grep -c '^leaf ' || true)
        note "split${suffix}: start: $last leaf/leaves in the fs tree"

        split_at=""
        for i in $(seq 1 "$max"); do
            cp --sparse=always "$work" "$prev"
            mount -o loop "$work" "$MNT"
            if [ "$vary" = 1 ] && [ $((i % 2)) -eq 0 ]; then
                name="$(printf 'x%.0s' $(seq 1 200))-$i"
            else
                name="f$i"
            fi
            echo "$i" > "$MNT/$name"
            sync
            umount "$MNT"

            now=$(btrfs inspect-internal dump-tree -t 5 "$work" 2>/dev/null | grep -c '^leaf ' || true)
            if [ "$now" -gt "$last" ]; then
                split_at="$i"
                cp --sparse=always "$prev" "$before"
                cp --sparse=always "$work" "$after"
                {
                    echo "files_before_split=$((i - 1))"
                    echo "leaves_before=$last"
                    echo "leaves_after=$now"
                    echo "nodesize=4096"
                } > "$WORK/btrfs-split${suffix}.txt"
                note "split${suffix}: at file $i: $last leaves -> $now"
                break
            fi
            last="$now"
        done
        rm -f "$prev" "$work"
        [ -n "$split_at" ] || {
            echo "guest-build-images: no leaf split after $max files in the ${suffix:-default} run" >&2
            exit 1
        }
        publish "$before" "$after" "$WORK/btrfs-split${suffix}.txt"
        note "built btrfs-split${suffix}-before.img and btrfs-split${suffix}-after.img"
    done
}

# ---------------------------------------------------------------------
# pool — a btrfs filesystem spanning two devices.
#
# Everything else in the matrix is one image. A pool is different in
# kind: a chunk stripe names the device it lives on, so reading one disk
# of a two-disk filesystem is not a partial read, it is a read of the
# WRONG BYTES — they parse, and on a mirrored pool they may even
# checksum. What a reader is expected to do with one of them is refuse,
# and tests/pool_oracle.rs holds it to that.
# ---------------------------------------------------------------------
build_pool() {
    local a="$WORK/btrfs-pool-a.img" b="$WORK/btrfs-pool-b.img"
    local manifest="$WORK/btrfs-pool.manifest" la lb i f devs
    rm -f "$a" "$b" "$manifest"
    truncate -s 512M "$a"
    truncate -s 512M "$b"

    # Loop devices, because mkfs.btrfs takes devices rather than files
    # when building a multi-device filesystem.
    la=$(losetup --find --show "$a")
    lb=$(losetup --find --show "$b")
    pool_cleanup() {
        mountpoint -q "$MNT" && umount "$MNT" || true
        losetup -d "$la" 2>/dev/null || true
        losetup -d "$lb" 2>/dev/null || true
    }
    trap 'pool_cleanup; cleanup' EXIT

    mkfs.btrfs -f -d raid1 -m raid1 "$la" "$lb" >/dev/null

    # Content, so a reader has something to be right or wrong about. An
    # empty pool proves only that the superblock parses.
    mount "$la" "$MNT"
    mkdir -p "$MNT/dir"
    for i in 1 2 3; do
        echo "pool file $i" > "$MNT/dir/file-$i.txt"
    done
    # Big enough to need a data extent rather than living inline in its
    # item, so the chunk mapping is actually exercised on a read.
    dd if=/dev/urandom of="$MNT/big.bin" bs=1M count=4 status=none
    sync

    # Size AND content. A size alone is a weak check on a mirrored pool:
    # reading four megabytes of the WRONG bytes has the right length.
    ( cd "$MNT" && find . -mindepth 1 | sort | while read -r p; do
        if [ -f "$p" ]; then
            printf '%s\t%s\t%s\n' "${p#.}" "$(stat -c%s "$p")" \
                "$(sha256sum "$p" | cut -d' ' -f1)"
        else
            printf '%s\tdir\t-\n' "${p#.}"
        fi
      done ) > "$manifest"

    umount "$MNT"
    losetup -d "$la"
    losetup -d "$lb"
    trap cleanup EXIT

    for f in "$a" "$b"; do
        devs=$(od -An -tu8 -j $((65536 + 0x88)) -N 8 "$f" | tr -d ' ')
        printf '[guest]   %s  num_devices %s\n' "$(basename "$f")" "$devs"
    done
    publish "$a" "$b" "$manifest"
    note "built btrfs-pool-a and btrfs-pool-b (two-device RAID1 with content)"
}

# ---------------------------------------------------------------------
# dirtylog — a filesystem with a log tree the kernel has not replayed.
#
# `fsync` writes a log tree and points the superblock's `log_root` at it;
# the NEXT mount replays it into the real trees. Until then the committed
# trees hold durable.txt as it was before the fsync, and fsynced.txt not
# at all — so a reader that ignored `log_root` would return exactly the
# bytes an application was told had been durably replaced.
#
# The image is copied WHILE MOUNTED, after the fsyncs and before any
# commit: the mount's commit interval is five minutes, and the unmount
# that follows would replay nothing but commit everything, erasing the
# very state being captured. The loop device writes through the backing
# file's page cache, so the copy sees every write the fsyncs made.
#
# The copy's `log_root` is checked, not assumed: a kernel that chose a
# full commit over a log would leave an ordinary image, and the test
# reading it would be testing nothing.
#
# In its own directory, like the snapshot: the suites that walk every
# image in test-disks/ mount each one, and this one is refused on
# purpose.
# ---------------------------------------------------------------------
build_dirtylog() {
    local img="$WORK/btrfs-dirty-log.img" copy="$WORK/btrfs-dirty-log-copy.img" log_root
    rm -f "$img" "$copy"
    truncate -s 256M "$img"
    mkfs.btrfs -f "$img" >/dev/null
    mount -o loop,commit=300 "$img" "$MNT"
    printf 'committed by a sync\n' > "$MNT/durable.txt"
    sync
    printf 'fsynced, not synced\n' | dd of="$MNT/durable.txt" conv=notrunc,fsync status=none
    printf 'created then fsynced\n' | dd of="$MNT/fsynced.txt" conv=fsync status=none
    cp --sparse=always "$img" "$copy"
    umount "$MNT"
    rm -f "$img"
    btrfs inspect-internal dump-super "$copy" > "$WORK/dirty-log.super"
    log_root=$(awk '$1 == "log_root" { print $2 }' "$WORK/dirty-log.super")
    if [ -z "$log_root" ] || [ "$log_root" = 0 ]; then
        echo "guest-build-images: the dirty-log image has log_root '${log_root}' — the fsync did not leave a log tree" >&2
        exit 1
    fi
    mkdir -p "$OUT/dirtylog"
    cp --sparse=always "$copy" "$OUT/dirtylog/btrfs-dirty-log.img.partial"
    mv -f "$OUT/dirtylog/btrfs-dirty-log.img.partial" "$OUT/dirtylog/btrfs-dirty-log.img"
    rm -f "$copy" "$WORK/dirty-log.super"
    note "built btrfs-dirty-log (log_root $log_root)"
}

# ---------------------------------------------------------------------
# cli — the volume the command-line tools are tested against: labelled,
# and holding one of every shape a user will point `fs.btrfs` at.
#
#   hello.txt           a few bytes, so it is stored inline in its item
#   dir/random.bin      300 KiB of noise: an ordinary extent
#   dir/sub/deep.txt    a file two directories down
#   link                a symlink to hello.txt
#   sparse.bin          a 256 KiB hole and then 4 KiB of data
#   zstd/text.txt       compressible text under `compression=zstd`
#   nocow/data.bin      64 KiB in a `chattr +C` directory: NODATACOW, the
#                       one kind of file this crate can overwrite in place
#   vol/                a subvolume, with inside.txt, and later.txt written
#                       after the snapshot below was taken
#   snap/               a read-only snapshot of vol/, so it lacks later.txt
#
# THE MANIFEST IS WHAT THE KERNEL SAYS IS THERE, read back through its own
# driver on a read-only mount: one line per path, tab-separated --
# `<f|d|l> <path> <size> <sha256|target|-> <subvol|->` -- so the tools'
# listings and bytes are compared against Linux rather than against this
# crate. Each shape the list promises is checked on the built image, and a
# build where one did not happen (no inline extent, no zstd, no hole, no
# NODATACOW flag) fails here rather than in a test that would read it as a
# pass. Its own directory, like the snapshot fixture's: the suites that walk
# every image in test-disks/ assume what an ordinary volume holds.
# ---------------------------------------------------------------------
build_cli() {
    local img="$WORK/btrfs-cli.img" manifest="$WORK/btrfs-cli.manifest" dump="$WORK/cli-tree.txt"
    rm -f "$img" "$manifest"
    truncate -s 512M "$img"
    mkfs.btrfs -f -L CLITEST "$img" >/dev/null
    mount -o loop "$img" "$MNT"
    printf 'hello from the kernel\n' > "$MNT/hello.txt"
    mkdir -p "$MNT/dir/sub"
    dd if=/dev/urandom of="$MNT/dir/random.bin" bs=1024 count=300 status=none
    python3 -c "print('\n'.join('line %d of a file two directories down' % i for i in range(500)))" \
        > "$MNT/dir/sub/deep.txt"
    ln -s hello.txt "$MNT/link"
    dd if=/dev/urandom of="$MNT/sparse.bin" bs=4096 count=1 seek=64 status=none
    mkdir "$MNT/zstd"
    btrfs property set "$MNT/zstd" compression zstd
    python3 -c "print('the quick brown fox jumps over the lazy dog ' * 4000)" > "$MNT/zstd/text.txt"
    mkdir "$MNT/nocow"
    chattr +C "$MNT/nocow"
    dd if=/dev/urandom of="$MNT/nocow/data.bin" bs=4096 count=16 status=none
    btrfs subvolume create "$MNT/vol" >/dev/null
    echo "inside the subvolume" > "$MNT/vol/inside.txt"
    sync
    btrfs subvolume snapshot -r "$MNT/vol" "$MNT/snap" >/dev/null
    echo "written after the snapshot" > "$MNT/vol/later.txt"
    lsattr "$MNT/nocow/data.bin" | grep -q '^[^ ]*C' || {
        echo "guest-build-images: btrfs-cli's nocow/data.bin is not NODATACOW -- chattr +C did not take" >&2
        umount "$MNT"
        exit 1
    }
    sync
    umount "$MNT"

    mount -o loop,ro "$img" "$MNT"
    ( cd "$MNT"
      find . -mindepth 1 | sort | while read -r p; do
          path="${p#.}"
          subvol=-
          if [ -d "$p" ] && [ ! -L "$p" ] && [ "$(stat -c %i "$p")" = 256 ]; then subvol=subvol; fi
          if [ -L "$p" ]; then
              printf 'l\t%s\t%s\t%s\t-\n' "$path" "$(stat -c %s "$p")" "$(readlink "$p")"
          elif [ -d "$p" ]; then
              printf 'd\t%s\t-\t-\t%s\n' "$path" "$subvol"
          else
              printf 'f\t%s\t%s\t%s\t-\n' "$path" "$(stat -c %s "$p")" "$(sha256sum "$p" | cut -d' ' -f1)"
          fi
      done
    ) > "$manifest"
    # The hole is a hole: fewer bytes allocated than the file's length.
    [ "$(( $(stat -c %b "$MNT/sparse.bin") * 512 ))" -lt "$(stat -c %s "$MNT/sparse.bin")" ] || {
        echo "guest-build-images: btrfs-cli's sparse.bin has no hole" >&2
        umount "$MNT"
        exit 1
    }
    umount "$MNT"
    grep -q $'^d\t/vol\t-\t-\tsubvol$' "$manifest" && grep -q $'^d\t/snap\t-\t-\tsubvol$' "$manifest" || {
        echo "guest-build-images: btrfs-cli's manifest does not show vol/ and snap/ as subvolumes" >&2
        exit 1
    }

    btrfs inspect-internal dump-tree -t 5 "$img" > "$dump"
    grep -q 'inline extent' "$dump" || {
        echo "guest-build-images: btrfs-cli has no inline extent (hello.txt)" >&2
        exit 1
    }
    grep -q 'extent compression 3 (zstd)' "$dump" || {
        echo "guest-build-images: btrfs-cli has no zstd-compressed extent (zstd/text.txt)" >&2
        exit 1
    }
    rm -f "$dump"
    dump_super "$img"
    mkdir -p "$OUT/cli"
    local f
    for f in "$img" "${img%.img}.superdump" "$manifest"; do
        cp --sparse=always "$f" "$OUT/cli/$(basename "$f").partial"
        mv -f "$OUT/cli/$(basename "$f").partial" "$OUT/cli/$(basename "$f")"
        rm -f "$f"
    done
    note "built btrfs-cli"
}

# ---------------------------------------------------------------------

echo "[guest] $(btrfs --version 2>&1 | head -1) on $(uname -srm)"
for target in "$@"; do
    started=$(date +%s)
    "build_$target"
    note "$target: $(( $(date +%s) - started ))s"
done
