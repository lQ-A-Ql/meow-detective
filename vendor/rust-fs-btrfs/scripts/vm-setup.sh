#!/usr/bin/env bash
#
# vm-setup.sh — the fs-linux-test-harness [setup] script. Runs as root
# INSIDE the VM, re-applied by the harness whenever this file changes.
#
# THE GUEST IS WHERE THE ORACLE TOOLS LIVE. Not the host: btrfs-progs on
# a workstation is whatever that machine has — a Homebrew formula on a
# Mac, a distribution build on Linux, a different version per developer
# — and on a Mac it is not even the platform these images are for. One
# Debian guest, one version, the same answers for everyone.
#
#   btrfs-progs  mkfs.btrfs, `btrfs check`, `btrfs inspect-internal
#                dump-super/dump-tree`, `btrfs subvolume` — the oracle
#                tools (tests/support/src/oracle.rs) and the fixture
#                builder's formatter
#   attr         setfattr/getfattr: the extended-attribute fixture both
#                sets attributes with and takes its reference answer
#                from. Without it that fixture builds a filesystem with
#                no attributes and a manifest saying so
#   acl          setfacl/getfacl: the ACL fixture sets its ACLs, finds the
#                largest one the node size admits, and takes the entry
#                counts it records, through them
#   e2fsprogs    chattr/lsattr, which is how a directory is marked
#                NODATACOW — an ext2 tool that btrfs honours
#   python3      the compressible payloads the compression fixtures need
#   util-linux   losetup and mount: the loop mounts, which happen here
#                and nowhere else
#
# THE EXOTIC CHECKSUM MODULES. xxhash, sha256 and blake2 live in
# separate crypto modules, and a mount of such an image is a bad place
# to discover they are missing. They are loaded here so a fixture build
# fails with the reason rather than with "invalid argument".
#
# AND WHAT A RUST BUILD NEEDS FROM THE DISTRIBUTION (curl, gcc, libc6-dev,
# pkg-config), for `chore test:vm`: the whole suite compiled and run in
# here, which is how a macOS host runs a Linux test suite at all. NOT THE
# TOOLCHAIN ITSELF: scripts/guest-suite.sh installs that through
# `../rust-fs-core/scripts/guest-rust-toolchain.sh`, rust-fs-core's one copy of the
# install, which every driver runs and which recovers from an install a
# reaper or a deadline interrupted. It cannot run from here: the harness
# ships this one file into the guest, before `test:vm` has staged the core
# sibling on the share (rust-fs-core#190).
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

apt-get update -qq
apt-get install -y -qq \
    btrfs-progs attr acl e2fsprogs python3 util-linux \
    curl gcc libc6-dev pkg-config >/dev/null

modprobe loop
modprobe btrfs
# Not fatal on their own: the fixture that needs one fails by name when
# mkfs.btrfs refuses the algorithm, which is a better message than a
# setup script that stops the whole VM coming up.
for module in xxhash_generic sha256_generic blake2b_generic; do
    modprobe "$module" || echo "vm-setup: note: $module unavailable"
done

# sed, not head: head exits after one line, the tool gets SIGPIPE writing
# its second, and pipefail turns that into a failed setup.
btrfs --version 2>&1 | sed -n 1p
mkfs.btrfs --version 2>&1 | sed -n 1p

# 5.16 is the first btrfs-progs whose `btrfs check` knows the block-group
# tree and whose `inspect-internal dump-tree` prints `extent compression`
# for every algorithm the compression fixtures assert on. Debian 12 ships
# 6.2.
version="$(btrfs --version 2>&1 | sed -n 's/^btrfs-progs v\([0-9][0-9.]*\).*/\1/p' | head -1)"
if [ -z "$version" ] ||
    [ "$(printf '%s\n%s\n' 5.16 "$version" | sort -V | head -1)" != 5.16 ]; then
    echo "vm-setup: btrfs-progs ${version:-of unknown version} is older than 5.16" >&2
    exit 1
fi

echo "vm-setup: btrfs-progs and the oracle tools are installed in the guest"
