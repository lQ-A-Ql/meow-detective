set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/lvm-md-native"
mkdir -p "$root/config" "$root/files"
export TMPDIR="$root"
export LVM_SYSTEM_DIR="$root/config"
config='backup { backup=0 archive=0 } devices { use_devicesfile=0 } activation { udev_sync=0 udev_rules=0 }'
vg="forensic-lvm-md-$$"
array="/dev/md/forensic-lvm-md-$$"
loops=()
vg_owned=false
md_owned=false
cleanup() {
    if [[ "$md_owned" == true ]]; then
        mdadm --stop "$array"
        md_owned=false
    fi
    if [[ "$vg_owned" == true ]]; then
        vgchange --config "$config" -an "$vg"
        vgremove --config "$config" -ff -y "$vg"
        vg_owned=false
    fi
    for device in "${loops[@]}"; do
        losetup -d "$device"
    done
    loops=()
}
trap cleanup EXIT
modprobe dm_mod
modprobe raid0
for index in 0 1; do
    truncate -s 16M "$root/pv-$index.raw"
    loops+=("$(losetup --find --show "$root/pv-$index.raw")")
done
pvcreate --config "$config" --metadatacopies 1 -y "${loops[0]}"
pvcreate --config "$config" --metadatacopies 0 -y "${loops[1]}"
vgcreate --config "$config" -s 1M "$vg" "${loops[@]}"
vg_owned=true
lvcreate --config "$config" -L 8M -n member0 "$vg" "${loops[0]}"
lvcreate --config "$config" -L 8M -n member1 "$vg" "${loops[1]}"
mdadm --create "$array" --run --assume-clean --force \
    --metadata=1.2 --level=0 --raid-devices=2 --chunk=4 --size=4096 \
    --homehost=forensic-fixture --name=lvm-md-ext4 \
    "/dev/$vg/member0" "/dev/$vg/member1"
md_owned=true
cp "$repo/testdata/fixtures/public-small/mdraid/oracle.bin" "$root/files/evidence.bin"
mke2fs -q -F -t ext4 -d "$root/files" "$array"
debugfs -R 'cat /evidence.bin' "$array" > "$root/oracle.bin"
cmp "$root/files/evidence.bin" "$root/oracle.bin"
mdadm --detail "$array" > "$root/md-detail.txt"
pvs --config "$config" -o pv_uuid,pv_mda_count,pv_name --reportformat json "${loops[@]}" > "$root/pvs.json"
vgs --config "$config" -o vg_uuid,vg_seqno,vg_name --reportformat json "$vg" > "$root/vgs.json"
mdadm --stop "$array"
md_owned=false
vgchange --config "$config" -an "$vg"
for index in 0 1; do
    gzip -n -9 -c "$root/pv-$index.raw" > "$root/pv-$index.raw.gz"
done
cleanup
