set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/thin-held"
mkdir -p "$root"
export TMPDIR="$root"
pool="forensic-held-pool-$$"
thin="forensic-held-device-$$"
loops=()
pool_owned=false
thin_owned=false
cleanup() {
    if [[ "$thin_owned" == true ]]; then
        dmsetup remove "$thin"
        thin_owned=false
    fi
    if [[ "$pool_owned" == true ]]; then
        dmsetup remove "$pool"
        pool_owned=false
    fi
    for loop in "${loops[@]}"; do losetup -d "$loop"; done
    loops=()
}
trap cleanup EXIT
modprobe dm_thin_pool
truncate -s 0 "$root/metadata.raw"
truncate -s 4M "$root/metadata.raw"
truncate -s 0 "$root/data.raw"
truncate -s 48M "$root/data.raw"
loops+=("$(losetup --find --show "$root/metadata.raw")")
loops+=("$(losetup --find --show "$root/data.raw")")
dmsetup create "$pool" --table "0 98304 thin-pool ${loops[0]} ${loops[1]} 128 0"
pool_owned=true
dmsetup message "$pool" 0 "create_thin 1"
dmsetup create "$thin" --table "0 16384 thin /dev/mapper/$pool 1"
thin_owned=true
dd if="$repo/testdata/fixtures/public-small/mdraid/oracle.bin" of="/dev/mapper/$thin" bs=4K conv=fsync status=none
dmsetup message "$pool" 0 reserve_metadata_snap
dd if="$repo/testdata/fixtures/public-small/mdraid/oracle.bin" of="/dev/mapper/$thin" bs=4K seek=256 conv=fsync status=none
dmsetup suspend "$thin"
dmsetup suspend "$pool"
cp "$root/metadata.raw" "$root/held.raw"
dmsetup resume "$pool"
dmsetup resume "$thin"
thin_check "$root/held.raw"
thin_dump "$root/held.raw" > "$root/held.xml"
gzip -n -9 -c "$root/held.raw" > "$root/held.raw.gz"
cleanup
