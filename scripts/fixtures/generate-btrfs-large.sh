set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
workspace="$repo/.codex-tmp/g04-btrfs-large"
fixture="$repo/testdata/fixtures/public-small/btrfs-large"
tree="$workspace/tree"
image="$workspace/single.raw"
restored="$workspace/restored"

if [[ -e "$workspace" || -e "$fixture" ]]; then
    printf 'Fixture workspace or output already exists\n' >&2
    exit 1
fi
mkdir -p "$tree/evidence" "$fixture" "$restored"
for ((index=0; index<4097; index++)); do
    name=$(printf 'entry-%05d.txt' "$index")
    printf 'Btrfs namespace evidence %05d\n' "$index" > "$tree/evidence/$name"
done
(
    cd "$tree"
    sha256sum evidence/*.txt
) > "$fixture/files.sha256"
truncate -s 256M "$image"
mkfs.btrfs --version > "$fixture/tool-versions.txt"
uname -r >> "$fixture/tool-versions.txt"
mkfs.btrfs -f -d single -m single --rootdir "$tree" "$image" |
    awk 'NF {sub(/[[:blank:]]+$/, ""); print}' > "$fixture/mkfs.txt"
btrfs check --readonly "$image" > "$fixture/check.txt" 2>&1
btrfs restore "$image" "$restored"
(
    cd "$restored"
    sha256sum --check "$fixture/files.sha256"
) > "$fixture/restored-oracle.txt"
btrfs inspect-internal dump-super -f "$image" |
    awk 'NF {sub(/[[:blank:]]+$/, ""); print}' > "$fixture/super.txt"
(
    cd "$workspace"
    sha256sum single.raw
) > "$fixture/image.sha256"
gzip -n -6 -c "$image" > "$fixture/single.raw.gz"
