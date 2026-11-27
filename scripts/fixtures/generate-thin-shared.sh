set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
root="$repo/.codex-tmp/thin-shared"
mkdir -p "$root"
export TMPDIR="$root"
python3 "$repo/scripts/fixtures/generate-thin-shared.py" "$root/shared.xml"
truncate -s 4M "$root/shared.raw"
thin_restore --input "$root/shared.xml" --output "$root/shared.raw"
thin_check "$root/shared.raw"
thin_dump "$root/shared.raw" > "$root/shared-verified.xml"
gzip -n -9 -c "$root/shared.raw" > "$root/shared.raw.gz"
