#!/usr/bin/env bash
set -euo pipefail

source_path=${1:?usage: inspect-pve-bluefs-log.sh <disk02.E01> [output-file]}
output_file=${2:-}
mount_dir=$(mktemp -d /tmp/meow-bluefs-log-ewf.XXXXXX)
osd_dir=$(mktemp -d /tmp/meow-bluefs-log-osd.XXXXXX)
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

ewfmount "$source_path" "$mount_dir" >/dev/null
loop_device=$(losetup -r -f --show "$mount_dir/ewf1")
volume_group=$(pvs --readonly --devices "$loop_device" --noheadings -o vg_name | xargs)
logical_volume=$(lvs --readonly --devices "$loop_device" --noheadings -o lv_name \
  "$volume_group" | xargs)
vgchange -ay --devices "$loop_device" "$volume_group" >/dev/null
device_path=$(lvs --readonly --devices "$loop_device" --noheadings -o lv_path \
  "$volume_group/$logical_volume" | xargs)
blockdev --setro "$device_path"
ceph-bluestore-tool prime-osd-dir --dev "$device_path" --path "$osd_dir" >/dev/null
ln -s "$device_path" "$osd_dir/block"

printf 'device=%s\n' "$device_path"
printf 'readOnly=%s\n' "$(blockdev --getro "$device_path")"
if [[ -n "$output_file" ]]; then
  ceph-bluestore-tool bluefs-log-dump --dev "$device_path" --path "$osd_dir" \
    >"$output_file"
else
  ceph-bluestore-tool bluefs-log-dump --dev "$device_path" --path "$osd_dir"
fi
