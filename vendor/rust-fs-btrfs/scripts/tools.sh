#!/usr/bin/env bash
#
# tools.sh — what the HOST needs (`chore tools`).
#
#   tools.sh           install what is missing, then verify
#   tools.sh --check   verify only; exit 1 naming what is missing
#
# THE ORACLE TOOLS ARE NOT HERE, AND THAT IS THE POINT. btrfs-progs —
# mkfs.btrfs, `btrfs check`, `btrfs inspect-internal` — runs inside the
# fs-linux-test-harness VM, provisioned by scripts/vm-setup.sh, and
# nowhere else. A workstation therefore installs none of it: no Homebrew
# formula on a Mac, no distribution build whose version differs from the
# next machine's, and no chance of an oracle answering differently
# depending on who asked. tests/support/src/oracle.rs is the only way a
# test reaches one, and tests/test_contract.rs fails the suite if
# anything runs one on the host.
#
# It replaces scripts/install-host-tools.sh, which installed btrfs-progs
# on the host and then had to reason about which version it got.
#
# What the host does need:
#
#   ripgrep      the script tests (`chore test:scripts`) search the tree
#                with it; a check written around a missing rg reports
#                PASS having matched nothing
#   the VM       Vagrant, QEMU and KVM/HVF — checked by the harness
#                itself (../fs-linux-test-harness/scripts/host-tools.sh,
#                also `chore vm:host:check`), which knows what it needs
#                on each platform
#
# `--check` LEAVES THE VM OUT. It is the early gate the test tasks run so
# that a missing ripgrep fails once instead of in every script test, and
# it is also what the architecture-only CI job can run on a machine with
# no KVM at all. Whether the VM works is answered by booting it: the
# first oracle call does that, and says what to install when it cannot.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
HARNESS_TOOLS="$REPO/../fs-linux-test-harness/scripts/host-tools.sh"

MODE=install
[ "${1:-}" = "--check" ] && MODE=check

# tool:debian-package:homebrew-formula
TOOLS="rg:ripgrep:ripgrep"

missing() {
    local entry out=""
    for entry in $TOOLS; do
        command -v "${entry%%:*}" >/dev/null 2>&1 || out="$out $entry"
    done
    echo "${out# }"
}

names() {
    local entry
    for entry in "$@"; do printf '%s ' "${entry%%:*}"; done
}

packages() {
    local field="$1" entry
    shift
    for entry in "$@"; do echo "$entry" | cut -d: -f"$field"; done | sort -u | tr '\n' ' '
}

install_linux() {
    local sudo="" pkgs
    # shellcheck disable=SC2046  # one word per package
    pkgs="$(packages 2 $(missing))"
    [ "$(id -u)" -eq 0 ] || sudo="sudo"
    if ! command -v apt-get >/dev/null 2>&1; then
        echo "tools: no apt-get on this Linux host; install with its package manager: $pkgs" >&2
        exit 1
    fi
    echo "tools: installing with apt-get: $pkgs"
    # shellcheck disable=SC2086  # the package list splits into words
    $sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq $pkgs >/dev/null ||
        { $sudo apt-get update -qq && $sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq $pkgs >/dev/null; }
}

gap="$(missing)"
if [ -n "$gap" ] && [ "$MODE" = install ]; then
    case "$(uname -s)" in
        Linux) install_linux ;;
        Darwin)
            # shellcheck disable=SC2086
            echo "tools: missing on this Mac: $(names $gap)" >&2
            # shellcheck disable=SC2086
            echo "       brew install $(packages 3 $gap)" >&2
            exit 1
            ;;
        *) echo "tools: unsupported host $(uname -s)" >&2; exit 1 ;;
    esac
    gap="$(missing)"
fi

if [ -n "$gap" ]; then
    # shellcheck disable=SC2086
    echo "tools: missing: $(names $gap)— run 'chore tools'" >&2
    exit 1
fi

printf '  %-9s %s\n' rg "$(command -v rg)"
echo "tools: $(rg --version | head -1) — the host's own tools are present"
echo "tools: btrfs-progs lives in the harness VM (scripts/vm-setup.sh), not here."

[ "$MODE" = check ] && exit 0

if [ ! -x "$HARNESS_TOOLS" ]; then
    echo "tools: the fs-linux-test-harness sibling is not checked out." >&2
    echo "       Run 'chore siblings'. The oracle tools, the fixtures and the" >&2
    echo "       kernel tests all run in its VM." >&2
    exit 1
fi
"$HARNESS_TOOLS"
