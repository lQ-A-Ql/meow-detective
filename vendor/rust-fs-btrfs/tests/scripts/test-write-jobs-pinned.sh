#!/usr/bin/env bash
# A job that holds a write token runs only code fixed by commit, and does
# not leave that token on disk.
#
# A tag is a pointer its owner can move, and a branch moves on every push.
# `uses: dtolnay/rust-toolchain@stable` in a job with `id-token: write` runs
# whatever that branch points at on the day, in the job that mints this
# crate's crates.io publishing token. A 40-hex commit SHA cannot be moved;
# the tag it stands for is kept in a comment beside it, for the person who
# updates it. rust-fs-ext4 met this first (its #329) and this is its guard.
#
# `actions/checkout` persists the job's token into .git/config by default,
# where every later step -- every action, every build script -- can read
# it. A write job's checkout must say `persist-credentials: false`.
#
# WHAT COUNTS AS A WRITE JOB: any `permissions` value of `write`, or
# `write-all`, on the job or inherited from the workflow. A workflow that
# declares no permissions at all runs on the repository's default token,
# which this file cannot see -- and here that default is read-write -- so it
# is treated as write: declare them.
#
# Parsed as YAML, not scanned by line: a `run: |` block can hold text that
# looks like a `uses:` key, and a guard that misreads its input reports
# protection it is not providing.
#
#   bash tests/scripts/test-write-jobs-pinned.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

fail() { echo "FAIL  $*" >&2; exit 1; }

command -v python3 >/dev/null 2>&1 || fail "python3 is required"
python3 -c 'import yaml' 2>/dev/null ||
    fail "the python3 yaml module is required (pip install pyyaml)"

# Every violation in the workflows named, one per line; nothing when clean.
scan() {
    python3 - "$@" <<'PY'
import re, sys, yaml

SHA = re.compile(r"^[^@\s]+@[0-9a-f]{40}$")

def writes(perms):
    if perms is None:
        return True                     # the repository default: unknown
    if isinstance(perms, str):
        return perms == "write-all"
    if isinstance(perms, dict):
        return any(v == "write" for v in perms.values())
    return True

for path in sys.argv[1:]:
    doc = yaml.safe_load(open(path)) or {}
    top = doc.get("permissions")
    for name, job in (doc.get("jobs") or {}).items():
        perms = job.get("permissions", top)
        if not writes(perms):
            continue
        where = f"{path}: job {name}"
        refs = [job["uses"]] if "uses" in job else []
        for step in job.get("steps") or []:
            uses = step.get("uses")
            if not uses:
                continue
            refs.append(uses)
            if uses.split("@")[0] == "actions/checkout":
                persist = (step.get("with") or {}).get("persist-credentials")
                if persist not in (False, "false"):
                    print(f"{where}: {uses} does not set persist-credentials: false")
        for uses in refs:
            if uses.startswith("./"):
                continue                # this repository, at this commit
            if uses.startswith("docker://"):
                if "@sha256:" not in uses:
                    print(f"{where}: {uses} is not pinned by digest")
                continue
            if not SHA.match(uses):
                print(f"{where}: {uses} is not pinned to a commit SHA")
PY
}

SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT HUP INT TERM

# --- 1. The scan refuses what it exists to refuse. ------------------------
#
# Without this, a scan that matched nothing -- a wrong key, a job read as
# read-only -- would pass the real workflows having checked nothing.
cat > "$SANDBOX/bad.yml" <<'EOF'
on: push
permissions:
  contents: read
jobs:
  upload:
    permissions:
      contents: write
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: some/action@v2
      - uses: some/action@6323deb102c322ba6fcbdcafc7e3dddab5  # too short
      - uses: ./local-action
      - run: |
          echo "uses: not/a-step@v1"
  oidc:
    permissions:
      id-token: write
    runs-on: ubuntu-latest
    steps:
      - uses: other/action@main
  read-only:
    runs-on: ubuntu-latest
    steps:
      - uses: read/only@v1
EOF
cat > "$SANDBOX/undeclared.yml" <<'EOF'
on: push
jobs:
  default-token:
    runs-on: ubuntu-latest
    steps:
      - uses: undeclared/action@v1
EOF
cat > "$SANDBOX/good.yml" <<'EOF'
on: push
permissions:
  contents: read
jobs:
  upload:
    permissions:
      contents: write
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0
        with:
          persist-credentials: false
      - uses: ./local-action
      - run: 'echo "uses: not/a-step@v1"'
EOF

found="$(scan "$SANDBOX/bad.yml" "$SANDBOX/undeclared.yml")" || fail "the scan itself failed"
expect=(
    "job upload: actions/checkout@v5 does not set persist-credentials: false"
    "job upload: actions/checkout@v5 is not pinned"
    "job upload: some/action@v2 is not pinned"
    "job upload: some/action@6323deb102c322ba6fcbdcafc7e3dddab5 is not pinned"
    "job oidc: other/action@main is not pinned"
    "job default-token: undeclared/action@v1 is not pinned"
)
for e in "${expect[@]}"; do
    grep -qF "$e" <<<"$found" || fail "the scan missed '$e':"$'\n'"$found"
done
count="$(grep -c . <<<"$found")"
[[ "$count" -eq ${#expect[@]} ]] ||
    fail "the scan found $count problems, expected ${#expect[@]}:"$'\n'"$found"

found="$(scan "$SANDBOX/good.yml")" || fail "the scan itself failed"
[[ -z "$found" ]] || fail "the scan refused a pinned write job:"$'\n'"$found"

# --- 2. The real workflows. -----------------------------------------------
shopt -s nullglob
workflows=("$REPO"/.github/workflows/*.yml "$REPO"/.github/workflows/*.yaml)
[[ ${#workflows[@]} -gt 0 ]] || fail "no workflows under .github/workflows"

found="$(scan "${workflows[@]}")" || fail "the scan itself failed"
if [[ -n "$found" ]]; then
    echo "FAIL  a job holding a write token runs a movable ref or keeps its token on disk:" >&2
    printf '%s\n' "${found//$REPO\//}" >&2
    exit 1
fi

echo "PASS  every write job runs actions pinned to a commit, with no persisted token"
