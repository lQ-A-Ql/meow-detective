#!/usr/bin/env bash
set -euo pipefail

source_path=${1:?usage: inspect-pve-rbd-placement.sh <disk02.E01> <output-dir> [object-name]}
output_dir=${2:?usage: inspect-pve-rbd-placement.sh <disk02.E01> <output-dir> [object-name]}
object_name=${3:-rbd_data.16ecc87af5c9.0000000000000000}

mount_dir=$(mktemp -d /tmp/meow-rbd-ewf.XXXXXX)
osd_dir=$(mktemp -d /tmp/meow-rbd-osd.XXXXXX)
loop_device=
volume_group=

cleanup() {
  if [[ -n "$volume_group" && -n "$loop_device" ]]; then
    vgchange -an --devices "$loop_device" "$volume_group" >/dev/null 2>&1 || true
  fi
  if [[ -n "$loop_device" ]]; then
    losetup -d "$loop_device" >/dev/null 2>&1 || true
  fi
  fusermount -u "$mount_dir" >/dev/null 2>&1 || true
  rm -rf "$mount_dir" "$osd_dir"
}

trap cleanup EXIT

mkdir -p "$output_dir"
ewfmount "$source_path" "$mount_dir" >/dev/null
loop_device=$(losetup -r -f --show "$mount_dir/ewf1")
mapfile -t volume_groups < <(
  pvs --readonly --devices "$loop_device" --noheadings -o vg_name |
    awk 'NF { print $1 }'
)
if [[ ${#volume_groups[@]} -ne 1 ]]; then
  printf 'expected one volume group, found %d\n' "${#volume_groups[@]}" >&2
  exit 1
fi
volume_group=${volume_groups[0]}
mapfile -t logical_volumes < <(
  lvs --readonly --devices "$loop_device" --noheadings -o lv_name \
    "$volume_group" | awk 'NF { print $1 }'
)
if [[ ${#logical_volumes[@]} -ne 1 ]]; then
  printf 'expected one logical volume, found %d\n' "${#logical_volumes[@]}" >&2
  exit 1
fi
logical_volume=${logical_volumes[0]}

vgchange -ay --devices "$loop_device" "$volume_group" >/dev/null
device_path=$(lvs --readonly --devices "$loop_device" --noheadings -o lv_path \
  "$volume_group/$logical_volume" | xargs)
blockdev --setro "$device_path"
if [[ $(blockdev --getro "$device_path") != 1 ]]; then
  printf 'refusing to inspect a writable BlueStore device: %s\n' "$device_path" >&2
  exit 1
fi

ceph-bluestore-tool prime-osd-dir --dev "$device_path" --path "$osd_dir" >/dev/null
ln -s "$device_path" "$osd_dir/block"

map_path="$output_dir/osdmap"
superblock_path="$output_dir/osd-superblock"
ceph-objectstore-tool --data-path "$osd_dir" --no-mon-config \
  --op get-osdmap --file "$map_path"
ceph-objectstore-tool --data-path "$osd_dir" --no-mon-config \
  --op get-superblock --file "$superblock_path"

osdmaptool --print "$map_path" >"$output_dir/osdmap.txt"
osdmaptool --test-map-object "$object_name" --pool 2 "$map_path" \
  >"$output_dir/object-placement.txt"

printf 'device=%s\n' "$device_path"
printf 'readOnly=%s\n' "$(blockdev --getro "$device_path")"
printf 'osdmap=%s\n' "$map_path"
printf 'superblock=%s\n' "$superblock_path"
cat "$output_dir/object-placement.txt"
