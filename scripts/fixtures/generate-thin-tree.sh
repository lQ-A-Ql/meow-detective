set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/thin-tree"
mkdir -p "$root"
export TMPDIR="$root"
python3 "$repo/scripts/fixtures/generate-thin-tree.py" "$root/tree.xml"
truncate -s 4M "$root/tree.raw"
thin_restore --input "$root/tree.xml" --output "$root/tree.raw"
thin_check "$root/tree.raw"
thin_dump "$root/tree.raw" > "$root/tree-verified.xml"
gzip -n -9 -c "$root/tree.raw" > "$root/tree.raw.gz"
