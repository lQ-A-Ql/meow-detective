#!/usr/bin/env bash
set -euo pipefail

fixture_root=${1:-/mnt/e/pangushi/服务器}
current_mount_dir=
current_loop_device=
current_volume_group=

cleanup_current() {
  if [[ -n "$current_volume_group" && -n "$current_loop_device" ]]; then
    vgchange -an --devices "$current_loop_device" "$current_volume_group" \
      >/dev/null 2>&1 || true
  fi
  if [[ -n "$current_loop_device" ]]; then
    losetup -d "$current_loop_device" >/dev/null 2>&1 || true
  fi
  if [[ -n "$current_mount_dir" ]]; then
    fusermount -u "$current_mount_dir" >/dev/null 2>&1 || true
  fi
  current_volume_group=
  current_loop_device=
  current_mount_dir=
}

trap cleanup_current EXIT

while IFS= read -r -d '' source_path; do
  base_name=$(basename "$source_path" .E01)
  mount_dir="/tmp/meow-ceph-ewf-$base_name"
  mkdir -p "$mount_dir"
  current_mount_dir=$mount_dir
  ewfmount "$source_path" "$mount_dir" >/dev/null

  loop_device=$(losetup -r -f --show "$mount_dir/ewf1")
  current_loop_device=$loop_device
  volume_group=$(pvs --readonly --devices "$loop_device" --noheadings -o vg_name | xargs)
  current_volume_group=$volume_group
  logical_volume=$(lvs --readonly --devices "$loop_device" --noheadings -o lv_name "$volume_group" | xargs)
  # The backing loop is read-only; force the mapped LV read-only as a second
  # evidence-integrity boundary before invoking the Ceph inspection tool.
  vgchange -ay --devices "$loop_device" "$volume_group" >/dev/null
  device_path=$(lvs --readonly --devices "$loop_device" --noheadings -o lv_path \
    "$volume_group/$logical_volume" | xargs)
  blockdev --setro "$device_path"

  printf '=== %s vg=%s lv=%s ro=%s ===\n' \
    "$base_name" "$volume_group" "$logical_volume" "$(blockdev --getro "$device_path")"
  ceph-bluestore-tool show-label --dev "$device_path" |
    jq 'with_entries(
      .value |= (
        . + {osd_key_present: has("osd_key")}
        | del(.osd_key)
      )
    )'

  cleanup_current
done < <(find "$fixture_root" -mindepth 2 -maxdepth 2 -type f -name '*-disk02.E01' -print0 | sort -z)
