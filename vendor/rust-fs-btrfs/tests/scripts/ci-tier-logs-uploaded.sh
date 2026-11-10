#!/usr/bin/env bash
#
# ci-tier-logs-uploaded.sh — EVERY CI JOB THAT RUNS A TIER KEEPS ITS LOG.
#
# Each tier writes its whole run to tmp/logs/<tier>.log and prints one line
# naming it. On a CI runner that log goes away with the runner unless it is
# uploaded, and then:
#
#   * a FAILING run shows a verdict naming a file nobody can open, so the
#     reason for the failure is simply gone;
#   * a PASSING run throws away its evidence -- the `[oracle vm]` and
#     `[kernel vm]` lines recording that every tool ran and every mount
#     happened in the guest, which is what makes a green oracle run mean
#     anything.
#
# So every job in ci.yml that runs a `chore test` task must carry an
# `actions/upload-artifact` step that uploads tmp/logs/, and that step must
# be `if: always()` -- exactly that. An upload that runs only on success
# uploads nothing on the run it was wanted for, and any other condition can
# only make it run less often than an unconditional step would.
#
# The check is proven to refuse before it is trusted: it is run against
# workflows that lack the step, condition it on success, and upload the
# wrong path, and each must be refused.
#
#   bash tests/scripts/ci-tier-logs-uploaded.sh
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="$REPO/.github/workflows/ci.yml"
fails=0
ok()   { echo "ok $*"; }
fail() { echo "not ok $*" >&2; fails=$(( fails + 1 )); }

command -v python3 >/dev/null \
    || { echo "not ok python3 is required (the ci-gate lint needs it too)" >&2; exit 1; }
python3 -c 'import yaml' 2>/dev/null \
    || { echo "not ok python3's yaml module is required (pip install pyyaml)" >&2; exit 1; }

work="$(mktemp -d "${TMPDIR:-/tmp}/tier-logs-check.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# check WORKFLOW -> prints one line per job that does not keep its logs,
# exits 1 if there is any, 0 if none.
check() {
    python3 - "$1" <<'PY'
import re, sys, yaml

doc = yaml.safe_load(open(sys.argv[1])) or {}
jobs = doc.get("jobs") or {}
runs_a_tier = re.compile(r"(^|[\s;&|])chore\s+test(:[a-z]+)?(\s|$)", re.M)

missing = []
for name, job in jobs.items():
    steps = (job or {}).get("steps") or []
    if not any(runs_a_tier.search(str(s.get("run", ""))) for s in steps):
        continue
    kept = False
    for s in steps:
        if not str(s.get("uses", "")).startswith("actions/upload-artifact@"):
            continue
        cond = str(s.get("if", "")).replace("${{", "").replace("}}", "").strip()
        path = str((s.get("with") or {}).get("path", ""))
        if cond == "always()" and "tmp/logs" in [p.rstrip("/") for p in path.split()]:
            kept = True
    if not kept:
        missing.append(name)

for name in missing:
    print(name)
sys.exit(1 if missing else 0)
PY
}

# --- The real workflow. ------------------------------------------------------
tier_jobs="$(python3 - "$WORKFLOW" <<'PY'
import re, sys, yaml
jobs = (yaml.safe_load(open(sys.argv[1])) or {}).get("jobs") or {}
pat = re.compile(r"(^|[\s;&|])chore\s+test(:[a-z]+)?(\s|$)", re.M)
print(sum(1 for j in jobs.values()
          if any(pat.search(str(s.get("run", ""))) for s in (j or {}).get("steps") or [])))
PY
)"
if [ "${tier_jobs:-0}" -ge 4 ]; then
    ok "ci.yml has $tier_jobs jobs that run a tier"
else
    fail "ci.yml has at least four jobs that run a tier (found ${tier_jobs:-0}) -- the scan finds nothing to check"
fi

if out="$(check "$WORKFLOW")"; then
    ok "every job that runs a tier uploads tmp/logs/ with if: always()"
else
    fail "these jobs run a tier and do not upload tmp/logs/ with if: always(): $(echo $out)"
fi

# --- The check refuses what it exists to refuse. ----------------------------
write() {
    cat > "$work/$1.yml" <<EOF
on: pull_request
jobs:
  test:
    runs-on: ubuntu-24.04
    steps:
      - run: chore test
$2
EOF
}

write none ''
write on-success '      - uses: actions/upload-artifact@v4
        with:
          path: tmp/logs/'
write expression '      - uses: actions/upload-artifact@v4
        if: ${{ success() }}
        with:
          path: tmp/logs/'
write wrong-path '      - uses: actions/upload-artifact@v4
        if: always()
        with:
          path: fixtures.tar.gz'
write good '      - uses: actions/upload-artifact@v4
        if: ${{ always() }}
        with:
          name: logs-${{ github.job }}
          path: tmp/logs/'

for shape in none on-success expression wrong-path; do
    if check "$work/$shape.yml" >/dev/null; then
        fail "a job whose upload is '$shape' is refused"
    else
        ok "a job whose upload is '$shape' is refused"
    fi
done
if check "$work/good.yml" >/dev/null; then
    ok "a job uploading tmp/logs/ with if: always() is accepted"
else
    fail "a job uploading tmp/logs/ with if: always() is accepted"
fi

if [ "$fails" -gt 0 ]; then
    echo "FAIL  $fails tier-log upload violation(s)" >&2
    exit 1
fi
echo "PASS  every CI job that runs a tier keeps its log"
