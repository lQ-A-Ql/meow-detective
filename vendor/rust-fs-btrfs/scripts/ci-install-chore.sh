#!/usr/bin/env bash
#
# ci-install-chore.sh — install the pinned chore on a Linux CI runner.
#
# Every CI job runs chore tasks (the siblings, the tools, the fixtures,
# the tests), so chore comes first, pinned by CHORE_VERSION (set in the
# workflow) and checked against the release's own checksums.
set -euo pipefail

: "${CHORE_VERSION:?set CHORE_VERSION, e.g. 0.11.0}"
case "$(uname -m)" in
    x86_64) arch=x86_64 ;;
    aarch64) arch=arm64 ;;
    *) echo "ci-install-chore: no chore build for $(uname -m)" >&2; exit 1 ;;
esac
tarball="chore-${CHORE_VERSION}-linux-${arch}.tar.gz"
base="https://github.com/antimatter-studios/chore/releases/download/v${CHORE_VERSION}"
dir="${RUNNER_TEMP:-$(mktemp -d)}/chore"
mkdir -p "$dir"
curl -fsSL -o "$dir/$tarball" "$base/$tarball"
curl -fsSL -o "$dir/checksums.txt" "$base/checksums.txt"
(cd "$dir" && grep " ${tarball}\$" checksums.txt | sha256sum -c -)
tar -xzf "$dir/$tarball" -C "$dir"
bin="$(find "$dir" -type f -name chore -perm -u+x | head -1)"
[ -n "$bin" ] || { echo "ci-install-chore: no chore binary in $tarball" >&2; exit 1; }
install -D -m 0755 "$bin" "$HOME/.local/bin/chore"
[ -z "${GITHUB_PATH:-}" ] || echo "$HOME/.local/bin" >> "$GITHUB_PATH"
"$HOME/.local/bin/chore" --version | head -1
