set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
workspace="$repo/.codex-tmp/g04-btrfs-large-pool"
fixture="$repo/testdata/fixtures/public-small/btrfs-large-pool"
source_tree="$repo/.codex-tmp/g04-btrfs-large/tree"
mountpoint="$workspace/mount"

if [[ -e "$workspace" || -e "$fixture" || ! -d "$source_tree" ]]; then
    printf 'Generate the single-device fixture first; pool output must be new\n' >&2
    exit 1
fi
mkdir -p "$workspace" "$fixture" "$mountpoint"
devices=()
mounted=false
cleanup() {
    if [[ "$mounted" == true ]]; then
        umount "$mountpoint"
    fi
    for device in "${devices[@]}"; do
        losetup -d "$device"
    done
}
trap cleanup EXIT
for index in 0 1; do
    truncate -s 256M "$workspace/member-$index.raw"
    devices+=("$(losetup --find --show "$workspace/member-$index.raw")")
done
mkfs.btrfs --version > "$fixture/tool-versions.txt"
uname -r >> "$fixture/tool-versions.txt"
mkfs.btrfs -f -d single -m single "${devices[@]}" |
    awk 'NF {sub(/[[:blank:]]+$/, ""); print}' > "$fixture/mkfs.txt"
btrfs device scan "${devices[@]}"
mount -t btrfs "${devices[0]}" "$mountpoint"
mounted=true
cp -a "$source_tree/." "$mountpoint/"
sync -f "$mountpoint"
(
    cd "$mountpoint"
    sha256sum evidence/*.txt
) > "$fixture/files.sha256"
cmp "$fixture/files.sha256" "$repo/testdata/fixtures/public-small/btrfs-large/files.sha256"
btrfs filesystem sync "$mountpoint"
umount "$mountpoint"
mounted=false
btrfs check --readonly "${devices[0]}" > "$fixture/check.txt" 2>&1
for index in 0 1; do
    btrfs inspect-internal dump-super -f "$workspace/member-$index.raw" |
        awk 'NF {sub(/[[:blank:]]+$/, ""); print}' > "$fixture/member-$index.super.txt"
    gzip -n -6 -c "$workspace/member-$index.raw" > "$fixture/member-$index.raw.gz"
done
(
    cd "$workspace"
    sha256sum member-0.raw member-1.raw
) > "$fixture/images.sha256"
