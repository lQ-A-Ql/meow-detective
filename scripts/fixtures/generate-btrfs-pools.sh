set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/s06-btrfs"
mkdir -p "$root/tree/nested"
dd if=/dev/zero bs=64K count=8 status=none |
    openssl enc -aes-256-ctr -K 000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f \
        -iv 20261128000000000000000000000000 > "$root/tree/evidence.bin"
printf 'Btrfs native multi-device evidence\n' > "$root/tree/nested/identity.txt"
sha256sum "$root/tree/evidence.bin" "$root/tree/nested/identity.txt" > "$root/files.sha256"
mkfs.btrfs --version > "$root/tool-versions.txt"
uname -r >> "$root/tool-versions.txt"
modprobe btrfs
loops=()
mounted=false
mountpoint="$root/mount"
mkdir -p "$mountpoint"
cleanup() {
    if [[ "$mounted" == true ]]; then
        umount "$mountpoint"
        mounted=false
    fi
    for device in "${loops[@]}"; do
        losetup -d "$device"
    done
    loops=()
}
trap cleanup EXIT

build() {
    profile=$1
    count=$2
    folder="$root/$profile"
    mkdir -p "$folder"
    devices=()
    : > "$folder/images.sha256"
    for ((index=0; index<count; index++)); do
        path="$folder/member-$index.raw"
        truncate -s 0 "$path"
        truncate -s 256M "$path"
        devices+=("$path")
        loops+=("$(losetup --find --show "$path")")
    done
    mkfs.btrfs -f -d "$profile" -m "$profile" "${loops[@]}" > "$folder/mkfs.txt"
    cp --sparse=always "${devices[0]}" "$folder/stale-member.raw"
    btrfs device scan "${loops[@]}"
    mount -t btrfs "${loops[0]}" "$mountpoint"
    mounted=true
    cp -a "$root/tree/." "$mountpoint/"
    sync -f "$mountpoint"
    (
        cd "$mountpoint"
        sha256sum evidence.bin nested/identity.txt
    ) > "$folder/files.sha256"
    btrfs inspect-internal dump-tree -t chunk "${loops[0]}" > "$folder/chunks.txt"
    cleanup
    for path in "${devices[@]}"; do
        btrfs inspect-internal dump-super -f "$path" > "${path%.raw}.super.txt"
        sha256sum "$path" >> "$folder/images.sha256"
        gzip -n -6 -c "$path" > "$path.gz"
    done
    gzip -n -6 -c "$folder/stale-member.raw" > "$folder/stale-member.raw.gz"
}

build single 2
build raid0 2
build raid1 2
build raid10 4
