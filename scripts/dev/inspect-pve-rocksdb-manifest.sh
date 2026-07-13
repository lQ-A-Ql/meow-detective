#!/usr/bin/env bash
set -euo pipefail

source_path=${1:?usage: inspect-pve-rocksdb-manifest.sh <disk02.E01>}
manifest_copy=${2:-}
export_copy=${3:-}
mount_dir=$(mktemp -d /tmp/meow-rocksdb-ewf.XXXXXX)
osd_dir=$(mktemp -d /tmp/meow-rocksdb-osd.XXXXXX)
export_dir=$(mktemp -d /tmp/meow-rocksdb-export.XXXXXX)
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
  rm -rf "$mount_dir" "$osd_dir" "$export_dir"
}

trap cleanup EXIT

ewfmount "$source_path" "$mount_dir" >/dev/null
loop_device=$(losetup -r -f --show "$mount_dir/ewf1")
mapfile -t volume_groups < <(
  pvs --readonly --devices "$loop_device" --noheadings -o vg_name \
    | awk 'NF { print $1 }'
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
ceph-bluestore-tool prime-osd-dir --dev "$device_path" --path "$osd_dir" >/dev/null
ln -s "$device_path" "$osd_dir/block"
ceph-bluestore-tool bluefs-export --dev "$device_path" --path "$osd_dir" \
  --out-dir "$export_dir" >/dev/null
if [[ -n "$export_copy" ]]; then
  mkdir -p "$export_copy"
  cp -a "$export_dir/." "$export_copy/"
fi

printf 'device=%s\n' "$device_path"
printf 'readOnly=%s\n' "$(blockdev --getro "$device_path")"
find "$export_dir" -maxdepth 3 -type f -printf '%P %s\n' | sort
printf 'CURRENT='
cat "$export_dir/db/CURRENT"
printf 'IDENTITY='
cat "$export_dir/db/IDENTITY"
printf '\n'
manifest_name=$(tr -d '\n' <"$export_dir/db/CURRENT")
if [[ ! "$manifest_name" =~ ^MANIFEST-[0-9]+$ ]]; then
  printf 'CURRENT contains an invalid MANIFEST name: %q\n' "$manifest_name" >&2
  exit 1
fi
if [[ -n "$manifest_copy" ]]; then
  install -m 0600 "$export_dir/db/$manifest_name" "$manifest_copy"
fi
printf 'MANIFEST_DUMP_BEGIN\n'
manifest_dump_file="$export_dir/manifest-dump.txt"
ldb manifest_dump --verbose --path="$export_dir/db/$manifest_name" | tee "$manifest_dump_file"
printf 'MANIFEST_DUMP_END\n'
printf 'logicalEdits=%s\n' "$(grep -c '^VersionEdit {' "$manifest_dump_file")"
printf 'columnFamilies=%s\n' \
  "$(grep -aEc '^--------------- Column family ' "$manifest_dump_file")"
printf 'manifestDumpSstRows=%s\n' \
  "$(grep -aEc '^[[:space:]]+[0-9]+:[0-9]+\[' "$manifest_dump_file")"
live_files_metadata="$export_dir/live-files-metadata.txt"
ldb list_live_files_metadata --db="$export_dir/db" --sort_by_filename \
  | tee "$live_files_metadata"
printf 'liveSstFiles=%s\n' \
  "$(grep -aEc '^/.+\.sst : level [0-9]+, column family ' "$live_files_metadata")"
printf 'exportedSstFiles=%s\n' \
  "$(find "$export_dir/db" -maxdepth 1 -type f -name '*.sst' | wc -l)"
