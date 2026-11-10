#!/usr/bin/env bash
# Rebuild fuzz/corpus from a filesystem mkfs.btrfs wrote.
#
# NO WHOLE-IMAGE SEED, and that is a decision rather than an oversight.
# The smallest filesystem mkfs.btrfs will make is 16 MiB even with
# --mixed, which is over the 10 MiB ceiling github-guard enforces on a
# committed file. The structures worth fuzzing are all reachable as byte
# slices, and the mount path is the best-covered thing in this
# repository already -- 47 of 49 suites reach the harness -- so the
# marginal value of a whole-image target here is lower than the cost of
# carrying one.
#
# btrfs tree blocks have no magic number. They are found by their fsid,
# which every block repeats at offset 32 and which the superblock
# carries at the same offset.
#
# Usage: scripts/make-fuzz-corpus.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/btrfs-fuzz-corpus.XXXXXX")"
trap 'rm -rf "$work"' EXIT

command -v mkfs.btrfs >/dev/null || {
    echo "mkfs.btrfs not found; install btrfs-progs" >&2
    exit 1
}

# Populated through mkfs.btrfs's --rootdir, not by mounting: mounting
# needs root and --rootdir does not, and it still produces trees a real
# btrfs-progs wrote. Enough entries that the directory tree has leaves
# worth reading, and a file large enough to need extents.
tree="$work/tree"
mkdir -p "$tree/sub"
head -c 200000 /dev/urandom > "$tree/random.bin"
python3 -c "import sys; open(sys.argv[1],'w').write('the quick brown fox. ' * 6000)" "$tree/text.txt"
echo "deep" > "$tree/sub/deep.txt"
ln -sf sub/deep.txt "$tree/link"
for i in $(seq 1 200); do
    : > "$tree/entry-$(printf '%03d' "$i")"
done
command -v setfattr >/dev/null && setfattr -n user.colour -v blue "$tree/text.txt" || true

rm -rf "$here/fuzz/corpus"
mkdir -p "$here/fuzz/corpus"/{superblock,tree_block,inode_item,dir_items,xattr_items,chunk_item}

# Two filesystems, because the node size changes the arithmetic and
# nothing else: --mixed forces 4 KiB nodes at 16 MiB, and an ordinary
# one uses the 16 KiB default. Both are built in scratch; only the
# structures cut out of them are committed.
make_fs() {
    local name="$1" size="$2"; shift 2
    local img="$work/$name.img"
    truncate -s "$size" "$img"
    mkfs.btrfs -q -f --rootdir "$tree" "$@" "$img" || {
        echo "mkfs.btrfs could not build the '$name' filesystem" >&2
        exit 1
    }
    echo "$img"
}

mixed=$(make_fs mixed 16M --mixed)
plain=$(make_fs plain 256M)

python3 - "$here/fuzz/corpus" "$mixed" "$plain" <<'PY'
import os, struct, sys

root = sys.argv[1]
images = sys.argv[2:]

SUPER_OFFSET = 0x10000
SUPER_LEN = 4096
MAGIC = b'_BHRfS_M'
HEADER_LEN = 101
ITEM_LEN = 25

# The item types whose data is a decoder's input.
INODE_ITEM = 1
XATTR_ITEM = 24
DIR_ITEM = 84
CHUNK_ITEM = 228

def write(kind, name, data):
    with open(os.path.join(root, kind, name), 'wb') as f:
        f.write(data)

total_blocks = 0
total_items = 0
for img_path in images:
    label = os.path.basename(img_path)[:-len('.img')]
    img = open(img_path, 'rb').read()

    sb = img[SUPER_OFFSET:SUPER_OFFSET + SUPER_LEN]
    assert sb[64:72] == MAGIC, f"{label}: no btrfs magic in the superblock"
    write('superblock', f'{label}.bin', sb)

    fsid = sb[32:48]
    nodesize, = struct.unpack_from('<I', sb, 0x94)
    assert nodesize in (4096, 8192, 16384, 32768, 65536), f"implausible nodesize {nodesize}"

    # A tree block repeats the fsid at offset 32 and carries its level
    # at offset 100. A leaf and an internal node are different decoders
    # behind one entry point, so one of each shape is kept.
    seen_blocks = set()
    seen_items = {INODE_ITEM: 0, XATTR_ITEM: 0, DIR_ITEM: 0, CHUNK_ITEM: 0}
    for at in range(0, len(img) - nodesize + 1, nodesize):
        block = img[at:at + nodesize]
        if block[32:48] != fsid:
            continue
        nritems, = struct.unpack_from('<I', block, 96)
        level = block[100]
        if level > 8 or nritems == 0:
            continue
        if nritems * ITEM_LEN + HEADER_LEN > nodesize:
            continue

        shape = (level, min(nritems, 4))
        if shape not in seen_blocks:
            seen_blocks.add(shape)
            write('tree_block', f'{label}-level{level}-items{nritems}.bin', block)
            total_blocks += 1

        if level != 0:
            continue

        # Leaf items: a 17-byte key, then the offset and size of the
        # item's data, which grows from the end of the block.
        for i in range(nritems):
            base = HEADER_LEN + i * ITEM_LEN
            item_type = block[base + 8]
            offset, size = struct.unpack_from('<II', block, base + 17)
            start = HEADER_LEN + offset
            if size == 0 or start + size > nodesize:
                continue
            if item_type not in seen_items or seen_items[item_type] >= 3:
                continue
            data = block[start:start + size]
            kind = {
                INODE_ITEM: 'inode_item',
                XATTR_ITEM: 'xattr_items',
                DIR_ITEM: 'dir_items',
                CHUNK_ITEM: 'chunk_item',
            }[item_type]
            write(kind, f'{label}-{seen_items[item_type]}.bin', data)
            seen_items[item_type] += 1
            total_items += 1

assert total_blocks, "no tree block found -- the fsid scan needs revisiting"
assert total_items, "no leaf items found -- the item walk needs revisiting"
print(f"{total_blocks} tree blocks, {total_items} leaf items")
PY

echo "corpus rebuilt under fuzz/corpus:"
find "$here/fuzz/corpus" -type f | sort | sed "s#$here/##"
echo "total: $(find "$here/fuzz/corpus" -type f | wc -l) seeds, $(du -sh "$here/fuzz/corpus" | cut -f1)"
