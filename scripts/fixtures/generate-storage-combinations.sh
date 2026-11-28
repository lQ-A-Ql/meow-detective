set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/s06-combinations"
mkdir -p "$root/files" "$root/mount" "$root/config"
export TMPDIR="$root"
export LVM_SYSTEM_DIR="$root/config"
cp "$repo/testdata/fixtures/public-small/mdraid/oracle.bin" "$root/files/evidence.bin"
config='backup { backup=0 archive=0 } devices { use_devicesfile=0 } activation { udev_sync=0 udev_rules=0 }'
loops=()
partition_loop=
array=
vg=
mounted=false
md_owned=false
vg_owned=false
cleanup() {
    if [[ "$mounted" == true ]]; then
        umount "$root/mount"
        mounted=false
    fi
    if [[ -n "$partition_loop" ]]; then
        losetup -d "$partition_loop"
        partition_loop=
    fi
    if [[ "$vg_owned" == true ]]; then
        vgchange --config "$config" -an "$vg"
        vgremove --config "$config" -ff -y "$vg"
        vg_owned=false
    fi
    if [[ "$md_owned" == true ]]; then
        mdadm --stop "$array"
        md_owned=false
    fi
    for device in "${loops[@]}"; do
        losetup -d "$device"
    done
    loops=()
}
trap cleanup EXIT
modprobe raid0
modprobe dm_mod
modprobe btrfs
modprobe xfs
(
    uname -r
    mdadm --version 2>&1
    mkfs.btrfs --version
    mkfs.xfs -V
    lvm version
) > "$root/tool-versions.txt"

build() {
    name=$1
    folder="$root/$name"
    mkdir -p "$folder"
    capacity=80M
    member_kib=65536
    if [[ "$name" == md-xfs ]]; then
        capacity=272M
        member_kib=262144
    fi
    for index in 0 1; do
        truncate -s 0 "$folder/member-$index.raw"
        truncate -s "$capacity" "$folder/member-$index.raw"
        loops+=("$(losetup --find --show "$folder/member-$index.raw")")
    done
    array="/dev/md/forensic-s06-$$"
    mdadm --create "$array" --run --assume-clean --force --metadata=1.2 \
        --level=0 --raid-devices=2 --chunk=64 --data-offset=2048s --size="$member_kib" \
        --homehost=forensic-fixture --name="$name" "${loops[@]}"
    md_owned=true
    target="$array"
    filesystem=ext4
    case "$name" in
        md-gpt-ext4)
            printf 'label: gpt\nstart=2048,size=196608,type=L\n' | sfdisk "$array"
            sfdisk --json "$array" > "$folder/partitions.json"
            partition_loop=$(losetup --find --show --offset 1048576 --sizelimit 100663296 "$array")
            target="$partition_loop"
            ;;
        md-lvm-ext4)
            vg="forensic-s06-$$"
            pvcreate --config "$config" -y "$array"
            vgcreate --config "$config" -s 1M "$vg" "$array"
            vg_owned=true
            lvcreate --config "$config" -L 64M -n evidence "$vg"
            target="/dev/$vg/evidence"
            lvs --config "$config" -o lv_uuid,lv_name,segtype,seg_size,devices --reportformat json "$vg" > "$folder/lvs.json"
            ;;
        md-xfs) filesystem=xfs ;;
        md-btrfs) filesystem=btrfs ;;
    esac
    case "$filesystem" in
        ext4) mke2fs -q -F -t ext4 "$target" ;;
        xfs) mkfs.xfs -q -f -m crc=1,reflink=0,rmapbt=0,bigtime=0,inobtcount=0 -i nrext64=0,exchange=0 -n parent=0 "$target" ;;
        btrfs) mkfs.btrfs -q -f -d single -m dup "$target" ;;
    esac
    mount -t "$filesystem" "$target" "$root/mount"
    mounted=true
    cp "$root/files/evidence.bin" "$root/mount/evidence.bin"
    sync -f "$root/mount"
    (cd "$root/mount"; sha256sum evidence.bin) > "$folder/files.sha256"
    umount "$root/mount"
    mounted=false
    mdadm --detail --export "$array" > "$folder/md.env"
    if [[ "$vg_owned" == true ]]; then
        vgchange --config "$config" -an "$vg"
        vg_owned=false
    fi
    sha256sum "$array" > "$folder/logical.sha256"
    cleanup
    for index in 0 1; do
        gzip -n -6 -c "$folder/member-$index.raw" > "$folder/member-$index.raw.gz"
    done
}

targets=("$@")
if [[ ${#targets[@]} -eq 0 ]]; then
    targets=(md-gpt-ext4 md-lvm-ext4 md-xfs md-btrfs)
fi
for name in "${targets[@]}"; do
    case "$name" in
        md-gpt-ext4|md-lvm-ext4|md-xfs|md-btrfs) build "$name" ;;
        *) printf 'Unknown storage composition: %s\n' "$name" >&2; exit 1 ;;
    esac
done
