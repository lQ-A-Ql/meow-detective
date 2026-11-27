set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/lvm-shared"
mkdir -p "$root/config" "$root/files"
export TMPDIR="$root"
export LVM_SYSTEM_DIR="$root/config"
config='backup { backup=0 archive=0 } devices { use_devicesfile=0 } activation { udev_sync=0 udev_rules=0 }'
vg="forensic-shared-$$"
loops=()
owned=false
cleanup() {
    if [[ "$owned" == true ]]; then
        vgchange --config "$config" -an "$vg"
        vgremove --config "$config" -ff -y "$vg"
        owned=false
    fi
    for loop in "${loops[@]}"; do
        losetup -d "$loop"
    done
    loops=()
}
trap cleanup EXIT
for index in 0 1; do
    truncate -s 0 "$root/pv-$index.raw"
    truncate -s 64M "$root/pv-$index.raw"
    loops+=("$(losetup --find --show "$root/pv-$index.raw")")
done
pvcreate --config "$config" --metadatacopies 0 -y "${loops[0]}"
pvcreate --config "$config" --metadatacopies 1 -y "${loops[1]}"
vgcreate --config "$config" -s 1M "$vg" "${loops[@]}"
owned=true
lvcreate --config "$config" -L 80M -n logical "$vg" "${loops[@]}"
cp "$repo/testdata/fixtures/public-small/mdraid/oracle.bin" "$root/files/evidence.bin"
mkfs.ext4 -q -F -d "$root/files" "/dev/$vg/logical"
sha256sum "/dev/$vg/logical" > "$root/logical.sha256"
lvs --config "$config" --segments --reportformat json -o lv_name,lv_uuid,seg_start_pe,seg_size_pe,devices "$vg" > "$root/lvs.json"
vgchange --config "$config" -an "$vg"
for index in 0 1; do
    gzip -n -9 -c "$root/pv-$index.raw" > "$root/pv-$index.raw.gz"
done
cleanup
