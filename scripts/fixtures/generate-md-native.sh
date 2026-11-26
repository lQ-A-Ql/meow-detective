set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/md-native-fixtures"
mkdir -p "$root"
loops=()
array=
owned=false
cleanup() {
    if [[ "$owned" == true ]]; then
        mdadm --stop "$array"
        array=
        owned=false
    fi
    for device in "${loops[@]}"; do
        losetup -d "$device"
    done
    loops=()
}
trap cleanup EXIT
python3 "$repo/scripts/fixtures/md_payload.py" "$root"

build() {
    name=$1
    level=$2
    metadata=$3
    count=$4
    layout=$5
    filesystem=${6:-}
    folder="$root/$name"
    mkdir -p "$folder"
    for ((index=0; index<count; index++)); do
        truncate -s 0 "$folder/member-$index.raw"
        truncate -s 16M "$folder/member-$index.raw"
        loops+=("$(losetup --find --show "$folder/member-$index.raw")")
    done
    array="/dev/md/forensic-fixture-$$"
    options=()
    if [[ "$level" != 1 && "$level" != linear ]]; then
        options+=(--chunk=4)
    fi
    if [[ -n "$layout" ]]; then
        options+=(--layout="$layout")
    fi
    if [[ "$metadata" == 0.90 ]]; then
        options+=(--uuid=f87ce10e:9dbe4038:a0fc30d3:7420593a)
    fi
    mdadm --create "$array" --run --assume-clean --force \
        --homehost=forensic-fixture --name="$name" \
        --metadata="$metadata" --level="$level" --raid-devices="$count" \
        --size=8192 "${options[@]}" "${loops[@]}"
    owned=true
    dd if="$root/payload.bin" of="$array" bs=64K conv=fsync status=none
    dd if="$array" of="$folder/oracle.bin" bs=64K count=4 status=none
    cmp "$root/payload.bin" "$folder/oracle.bin"
    if [[ "$filesystem" == ext4 ]]; then
        mkdir -p "$folder/root"
        cp "$root/payload.bin" "$folder/root/evidence.bin"
        mke2fs -q -F -t ext4 -d "$folder/root" "$array"
        debugfs -R 'cat /evidence.bin' "$array" > "$folder/file-oracle.bin"
        cmp "$root/payload.bin" "$folder/file-oracle.bin"
    fi
    mdadm --detail "$array" > "$folder/detail.txt"
    cleanup
    for ((index=0; index<count; index++)); do
        mdadm --examine "$folder/member-$index.raw" > "$folder/member-$index.txt"
        gzip -n -9 -c "$folder/member-$index.raw" > "$folder/member-$index.raw.gz"
    done
}

build raid0-v12 0 1.2 2 ""
build raid1-v10 1 1.0 2 ""
build raid5-v11 5 1.1 3 left-symmetric
build raid6-v12 6 1.2 4 parity-last
build raid10-v12 10 1.2 4 n2
build linear-v12 linear 1.2 2 ""
build raid5-v090 5 0.90 3 left-symmetric
build raid0-ext4 0 1.2 2 "" ext4
