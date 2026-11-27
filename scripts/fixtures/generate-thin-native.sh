set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/thin-native"
mkdir -p "$root/config"
export TMPDIR="$root"
export LVM_SYSTEM_DIR="$root/config"
config='backup { backup=0 archive=0 } devices { use_devicesfile=0 } activation { udev_sync=0 udev_rules=0 }'
vg="forensic-thin-$$"
loop=
owned=false
cleanup() {
    if [[ "$owned" == true ]]; then
        vgchange --config "$config" -an "$vg"
        vgremove --config "$config" -ff -y "$vg"
        owned=false
    fi
    if [[ -n "$loop" ]]; then
        losetup -d "$loop"
        loop=
    fi
}
trap cleanup EXIT
modprobe dm_thin_pool
truncate -s 0 "$root/pv.raw"
truncate -s 96M "$root/pv.raw"
loop=$(losetup --find --show "$root/pv.raw")
pvcreate --config "$config" -y "$loop"
vgcreate --config "$config" -s 1M "$vg" "$loop"
owned=true
lvcreate --config "$config" --type thin-pool --poolmetadatasize 4M -L 48M -n pool "$vg"
lvcreate --config "$config" --thinpool "$vg/pool" -V 16M -n thin
dd if="$repo/testdata/fixtures/public-small/mdraid/oracle.bin" of="/dev/$vg/thin" \
    bs=4K conv=fsync status=none
dd if="$repo/testdata/fixtures/public-small/mdraid/oracle.bin" of="/dev/$vg/thin" \
    bs=4K seek=2049 conv=fsync status=none
dd if="/dev/$vg/thin" of="$root/oracle.raw" bs=1M status=none
sha256sum "$root/oracle.raw" > "$root/oracle.sha256"
pvs --config "$config" --segments --reportformat json --units b --nosuffix \
    -o pv_name,pe_start,pvseg_start,lv_name "$loop" > "$root/pvs.json"
vgs --config "$config" --reportformat json --units b --nosuffix \
    -o vg_extent_size,vg_uuid,vg_seqno "$vg" > "$root/vgs.json"
lvs --config "$config" --reportformat json -a -o lv_name,lv_size,lv_attr,thin_id,transaction_id "$vg" \
    > "$root/lvs.json"
vgchange --config "$config" -an "$vg"
extent=$(jq -r '.report[0].vg[0].vg_extent_size | tonumber' "$root/vgs.json")
pe_start=$(jq -r '.report[0].pvseg[0].pe_start | tonumber' "$root/pvs.json")
metadata_pe=$(jq -r '.report[0].pvseg[] | select(.lv_name == "[pool_tmeta]") | .pvseg_start | tonumber' "$root/pvs.json")
metadata_offset=$((pe_start + metadata_pe * extent))
dd if="$root/pv.raw" of="$root/metadata.raw" bs=512 skip=$((metadata_offset / 512)) count=8192 status=none
thin_check "$root/metadata.raw"
thin_dump "$root/metadata.raw" > "$root/metadata.xml"
gzip -n -9 -c "$root/pv.raw" > "$root/pv.raw.gz"
gzip -n -9 -c "$root/metadata.raw" > "$root/metadata.raw.gz"
gzip -n -9 -c "$root/oracle.raw" > "$root/oracle.raw.gz"
truncate -s 4M "$root/transaction-conflict.raw"
thin_restore --input "$root/metadata.xml" --output "$root/transaction-conflict.raw" \
    --transaction-id 2
thin_check "$root/transaction-conflict.raw"
dd if="$root/transaction-conflict.raw" of="$root/pv.raw" bs=512 \
    seek=$((metadata_offset / 512)) conv=notrunc status=none
gzip -n -9 -c "$root/pv.raw" > "$root/pv-transaction-conflict.raw.gz"
cleanup
