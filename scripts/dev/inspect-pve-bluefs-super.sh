#!/usr/bin/env bash
set -euo pipefail

source_path=${1:?usage: inspect-pve-bluefs-super.sh <disk02.E01> [output-dir]}
output_dir=${2:-/tmp/meow-bluefs-stage2-output}
mount_dir=
loop_device=
volume_group=
osd_dir=

cleanup() {
  if [[ -n "$volume_group" && -n "$loop_device" ]]; then
    vgchange -an --devices "$loop_device" "$volume_group" >/dev/null 2>&1 || true
  fi
  if [[ -n "$loop_device" ]]; then
    losetup -d "$loop_device" >/dev/null 2>&1 || true
  fi
  if [[ -n "$mount_dir" ]]; then
    fusermount -u "$mount_dir" >/dev/null 2>&1 || true
    rmdir "$mount_dir" >/dev/null 2>&1 || true
  fi
  if [[ -n "$osd_dir" ]]; then
    rm -rf "$osd_dir"
  fi
}

trap cleanup EXIT

base_name=$(basename "$source_path" .E01)
mount_dir=$(mktemp -d /tmp/meow-bluefs-ewf.XXXXXX)
osd_dir=$(mktemp -d /tmp/meow-bluefs-osd.XXXXXX)
mkdir -p "$output_dir"

ewfmount "$source_path" "$mount_dir" >/dev/null
loop_device=$(losetup -r -f --show "$mount_dir/ewf1")
volume_group=$(pvs --readonly --devices "$loop_device" --noheadings -o vg_name | xargs)
logical_volume=$(lvs --readonly --devices "$loop_device" --noheadings -o lv_name "$volume_group" | xargs)
vgchange -ay --devices "$loop_device" "$volume_group" >/dev/null
device_path=$(lvs --readonly --devices "$loop_device" --noheadings -o lv_path \
  "$volume_group/$logical_volume" | xargs)
blockdev --setro "$device_path"

raw_super="$output_dir/$base_name-bluefs-super.bin"
official_json="$output_dir/$base_name-bluefs-super.json"
dd if="$device_path" of="$raw_super" bs=4096 skip=1 count=1 status=none

# prime-osd-dir materializes only a temporary descriptor directory. The source
# block device remains kernel-enforced read-only throughout the inspection.
ceph-bluestore-tool prime-osd-dir --dev "$device_path" --path "$osd_dir" >/dev/null
if ceph-bluestore-tool bluefs-super-dump --dev "$device_path" --path "$osd_dir" \
  >"$official_json" 2>"$official_json.stderr"; then
  rm -f "$official_json.stderr"
else
  rm -f "$official_json"
  printf 'officialSuperDump=unavailable\n'
  sed 's/^/officialSuperDumpError=/' "$official_json.stderr"
  rm -f "$official_json.stderr"
fi

printf 'source=%s\n' "$source_path"
printf 'device=%s\n' "$device_path"
printf 'readOnly=%s\n' "$(blockdev --getro "$device_path")"
printf 'rawSuper=%s\n' "$raw_super"
if [[ -f "$official_json" ]]; then
  printf 'officialJson=%s\n' "$official_json"
fi
