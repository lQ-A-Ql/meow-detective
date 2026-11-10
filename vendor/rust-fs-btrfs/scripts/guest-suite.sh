#!/usr/bin/env bash
#
# guest-suite.sh [cargo test args...] — THE WHOLE SUITE, INSIDE THE VM.
#
# The fs-linux-test-harness [test] guest_command: `chore test:vm` (and
# `chore test` on a host that is not Linux) boots the VM and runs this
# from /repo, where the harness mounts this repository.
#
# WHY IT EXISTS. We run the Linux tests on Linux. On a Linux host that is
# the host itself and this is not used. On a Mac there is no btrfs, no
# btrfs-progs worth trusting and no loop mount — so the suite runs in here,
# against the same sources, with the same pinned toolchain.
#
# The toolchain and the build directory live on the VM's own disk
# (/var/lib, /var/cache), which outlives `vm:down` and is thrown away by
# `vm:destroy`: the first run pays a full build, later runs are
# incremental. The repository itself is a 9p mount, so nothing is written
# back into it except what the tests write to tmp/.
set -euo pipefail

[ "${FLTH_GUEST:-}" = 1 ] ||
    { echo "guest-suite.sh runs INSIDE the harness VM ('chore test:vm')." >&2; exit 1; }

# The path dependencies in Cargo.toml, by directory name.
SIBLINGS="rust-fs-core"

RUST_ROOT=/var/lib/fs-btrfs-rust
export RUSTUP_HOME="$RUST_ROOT/rustup"
export CARGO_HOME="$RUST_ROOT/cargo"
export CARGO_TARGET_DIR=/var/cache/fs-btrfs-target
export PATH="$CARGO_HOME/bin:$PATH"

# THE SIBLING CRATES. This crate's Cargo.toml has a path dependency on
# ../rust-fs-core, which on the host is a sibling checkout — and the
# guest is given this repository, not the directory that holds it. The
# harness mounts us at /repo, whose parent IS the guest's root, so
# `../rust-fs-core` resolves to /rust-fs-core: the task stages the
# sibling on the share (from its pinned, clean checkout) and this links
# it into place. `chore siblings` is what keeps it at the right ref.
# shellcheck disable=SC2043  # one sibling today; the list is the point
for sibling in $SIBLINGS; do
    staged="/share/siblings/$sibling"
    [ -d "$staged" ] || {
        echo "guest-suite.sh: $sibling is not staged on the share; 'chore test:vm' does that." >&2
        exit 1
    }
    [ -L "/$sibling" ] || ln -sfn "$staged" "/$sibling"
done

cd /repo
# THE PINNED TOOLCHAIN, installed -- or repaired, when an earlier install
# was interrupted and left its wreckage on the VM's own disk. It is
# rust-fs-core's family script, the one copy every driver runs, reached
# through the sibling linked just above: this is the first point at which
# core is in the guest, which is why vm-setup.sh cannot do it. It reads
# rust-toolchain.toml and installs into RUSTUP_HOME and CARGO_HOME, and
# over a whole toolchain it costs one manifest check (rust-fs-core#190).
bash ../rust-fs-core/scripts/guest-rust-toolchain.sh
command -v cargo >/dev/null ||
    { echo "guest-suite.sh: no cargo in the guest after the toolchain install." >&2; exit 1; }

echo "== in-guest suite: $(uname -srm), $(cargo --version)"
started=$(date +%s)
scripts/test.sh --locked --release "$@"
echo "== in-guest suite: $(( $(date +%s) - started ))s"
