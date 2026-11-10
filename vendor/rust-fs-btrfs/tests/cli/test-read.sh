# fs.btrfs's read verbs, as installed, against a volume the kernel wrote
# (test-disks/cli/btrfs-cli.img, `chore fixtures`): every path in the
# kernel's manifest listed with the type, size and target Linux gave it and
# every file read back to the bytes Linux hashed -- inline, compressed,
# sparse, NODATACOW, inside a subvolume and through a snapshot -- then
# get/info with the canonical keys, --offset into a whole-disk image, and
# structured errors with nothing on stdout.
source "$(dirname "$0")/lib.sh"

DISKS="$REPO/test-disks"
img="$DISKS/cli/btrfs-cli.img"
manifest="$DISKS/cli/btrfs-cli.manifest"
for f in "$img" "$manifest"; do
    if [ ! -s "$f" ]; then
        fail "${f#"$REPO"/} is missing: the fixtures are built by \`chore fixtures\` (in the harness VM). Nothing skips."
        finish
    fi
done

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum | cut -d' ' -f1
    else
        shasum -a 256 | cut -d' ' -f1
    fi
}

# entry_check DESCRIPTION JSON JQ-ARGS...: ok if `jq -e JQ-ARGS` holds for JSON.
entry_check() {
    local what="$1" json="$2"
    shift 2
    if jq -e "$@" <<<"$json" >/dev/null 2>&1; then ok; else fail "$what"; fi
}

# --- ls and read, path by path, against the kernel's manifest -----------
files=0
dirs=0
while IFS=$'\t' read -r kind path size what subvol; do
    parent="${path%/*}"
    [ -n "$parent" ] || parent=/
    name="${path##*/}"
    fs.btrfs "$img" ls "$parent" >"$SANDBOX/ls.json" 2>"$SANDBOX/ls.err" ||
        { fail "ls $parent: $(cat "$SANDBOX/ls.err")"; continue; }
    entry="$(jq -c --arg n "$name" '.[] | select(.name == $n)' "$SANDBOX/ls.json")"
    if [ -z "$entry" ]; then
        fail "ls $parent does not list $name"
        continue
    fi
    case "$kind" in
        f)
            files=$((files + 1))
            entry_check "$path is listed as a file of $size bytes ($entry)" "$entry" \
                --argjson s "$size" '.type == "file" and .size == $s and .subvolume == false'
            got="$(fs.btrfs "$img" read "$path" 2>"$SANDBOX/read.err" | sha256_of)"
            check "read $path is the kernel's bytes ($(cat "$SANDBOX/read.err"))" test "$got" = "$what"
            ;;
        d)
            dirs=$((dirs + 1))
            want=false
            [ "$subvol" = subvol ] && want=true
            entry_check "$path is listed as a directory, subvolume $want ($entry)" "$entry" \
                --argjson v "$want" '.type == "dir" and .subvolume == $v'
            ;;
        l)
            entry_check "$path is listed as a symlink to $what ($entry)" "$entry" \
                --arg t "$what" '.type == "symlink" and .target == $t'
            ;;
    esac
done <"$manifest"
check "the manifest named files ($files) and directories ($dirs)" test "$files" -ge 8 -a "$dirs" -ge 6

# Every entry of a listing is typed the same way, and a directory lists
# exactly what the kernel put in it.
fs.btrfs "$img" ls / >"$SANDBOX/root.json" 2>/dev/null
jq_check "every ls entry has typed fields" \
    'length > 0 and all(.[]; (.name|type)=="string" and (.type|type)=="string" and (.size|type)=="number" and (.mode|test("^[0-7]{4}$")) and (.mtime|type)=="number" and (.inode|type)=="number" and (.subvolume|type)=="boolean")' \
    "$SANDBOX/root.json"
want="$(awk -F'\t' '{ p = $2; sub(/^\//, "", p); if (p !~ /\//) print p }' "$manifest" | sort | tr '\n' ' ')"
got="$(jq -r '.[].name' "$SANDBOX/root.json" | sort | tr '\n' ' ')"
check "ls / names exactly what the kernel wrote ('$got' vs '$want')" test "$got" = "$want"
check "the snapshot does not hold what was written after it" \
    test -z "$(fs.btrfs "$img" ls /snap | jq -r '.[] | select(.name == "later.txt") | .name')"
fs.btrfs "$img" ls --text / >"$SANDBOX/root.txt" 2>/dev/null
check "ls --text marks a subvolume" grep -q ' vol (subvolume)$' "$SANDBOX/root.txt"
check "ls --text shows a symlink's target" grep -q ' link -> hello.txt$' "$SANDBOX/root.txt"
fs.btrfs "$img" ls /hello.txt >"$SANDBOX/one.json" 2>/dev/null
jq_check "ls of a file lists that file" 'length == 1 and .[0].name == "hello.txt"' "$SANDBOX/one.json"

# read -o writes the file whole, and leaves no .partial behind.
fs.btrfs "$img" read /dir/random.bin -o "$SANDBOX/out.bin" >"$SANDBOX/o.out" 2>/dev/null
check "read -o prints nothing on stdout" test ! -s "$SANDBOX/o.out"
check "read -o wrote the file" cmp -s "$SANDBOX/out.bin" <(fs.btrfs "$img" read /dir/random.bin)
check "read -o left no .partial file" test ! -e "$SANDBOX/out.bin.partial"

# --- get / info -----------------------------------------------------------
fs.btrfs "$img" get >"$SANDBOX/get.json" 2>/dev/null
check "get exits 0" test $? -eq 0
jq_check "get carries every canonical key with its type" \
    '(.fs=="btrfs") and (.label|type)=="string" and (.total_bytes|type)=="number" and (.free_bytes|type)=="number" and (.block_size|type)=="number" and (.dirty|type)=="boolean" and (.btrfs|type)=="object"' \
    "$SANDBOX/get.json"
jq_check "the volume is CLITEST, 512 MiB, clean, crc32c" \
    '.label == "CLITEST" and .total_bytes == 536870912 and .dirty == false and .btrfs.csum_type == "crc32c" and .free_bytes < .total_bytes' \
    "$SANDBOX/get.json"
fs.btrfs "$img" info >"$SANDBOX/info.json" 2>/dev/null
check "info and get print the same" cmp -s "$SANDBOX/get.json" "$SANDBOX/info.json"
fs.btrfs "$img" get label >"$SANDBOX/label.json" 2>/dev/null
jq_check "get label is {\"label\": \"CLITEST\"}" '. == {"label": "CLITEST"}' "$SANDBOX/label.json"
check "get label --text is CLITEST" test "$(fs.btrfs "$img" get label --text)" = CLITEST
check "get btrfs.fsid --text is a UUID" \
    grep -qE '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' <<<"$(fs.btrfs "$img" get btrfs.fsid --text)"
fs.btrfs "$img" get no.such.key >"$SANDBOX/nk.out" 2>"$SANDBOX/nk.err"
check "get of an unknown key exits 2" test $? -eq 2
jq_check "get of an unknown key is a structured error" '.code == 2' "$SANDBOX/nk.err"

# Every checksum algorithm, from the geometry fixtures.
for pair in crc32c:crc32c xxhash:xxhash64 sha256:sha256 blake2:blake2; do
    fixture="$DISKS/btrfs-csum-${pair%%:*}.img"
    if [ ! -s "$fixture" ]; then
        fail "${fixture#"$REPO"/} is missing: \`chore fixtures\` builds it"
        continue
    fi
    check "get btrfs.csum_type of btrfs-csum-${pair%%:*} is ${pair#*:}" \
        test "$(fs.btrfs "$fixture" get btrfs.csum_type --text 2>&1)" = "${pair#*:}"
    status=0
    fs.btrfs "$fixture" ls / >"$SANDBOX/csum.json" 2>"$SANDBOX/csum.err" || status=$?
    check "ls / of btrfs-csum-${pair%%:*} exits 0, not $status ($(cat "$SANDBOX/csum.err"))" test "$status" -eq 0
done

# A log waiting for replay: get still answers, and says it is dirty; the
# verbs that need the trees refuse, because the trees are not the truth yet.
dirty="$DISKS/dirtylog/btrfs-dirty-log.img"
if [ -s "$dirty" ]; then
    check "get dirty of a volume with a log to replay is true" \
        test "$(fs.btrfs "$dirty" get dirty --text 2>&1)" = true
    fs.btrfs "$dirty" ls / >"$SANDBOX/dl.out" 2>"$SANDBOX/dl.err"
    check "ls of a volume with a log to replay exits 1" test $? -eq 1
    check "ls of a volume with a log to replay prints nothing on stdout" test ! -s "$SANDBOX/dl.out"
    jq_check "ls of a volume with a log to replay says why" '.code == 1 and (.error | test("log"))' "$SANDBOX/dl.err"
else
    fail "test-disks/dirtylog/btrfs-dirty-log.img is missing: \`chore fixtures\` builds it"
fi

# --- --offset: the volume 1 MiB into a whole-disk image ----------------
dd if="$img" of="$SANDBOX/whole.img" bs=1048576 seek=1 conv=sparse,notrunc status=none 2>/dev/null ||
    dd if="$img" of="$SANDBOX/whole.img" bs=1048576 seek=1 conv=sparse,notrunc 2>/dev/null
fs.btrfs "$SANDBOX/whole.img" --offset 1048576 get label --text >"$SANDBOX/off.txt" 2>"$SANDBOX/off.err"
check "--offset 1048576 finds the volume ($(cat "$SANDBOX/off.err"))" test "$(cat "$SANDBOX/off.txt")" = CLITEST
want="$(awk -F'\t' '$2 == "/dir/sub/deep.txt" { print $4 }' "$manifest")"
check "--offset reads a file back to the kernel's bytes" \
    test "$(fs.btrfs --offset 1048576 "$SANDBOX/whole.img" read /dir/sub/deep.txt | sha256_of)" = "$want"
fs.btrfs "$SANDBOX/whole.img" get >"$SANDBOX/nooff.out" 2>"$SANDBOX/nooff.err"
check "without --offset the whole-disk image is no volume (exit 1)" test $? -eq 1
check "without --offset nothing is printed on stdout" test ! -s "$SANDBOX/nooff.out"
fs.btrfs "$img" --offset 99999999999 get >"$SANDBOX/past.out" 2>"$SANDBOX/past.err"
check "an --offset past the end exits 1" test $? -eq 1
jq_check "an --offset past the end says so" '.code == 1 and (.error | test("past the end"))' "$SANDBOX/past.err"

# --- failures: status 1, a structured error, nothing on stdout ------------
# refused1 WHAT PATTERN VERB...
refused1() {
    local what="$1" pattern="$2"
    shift 2
    fs.btrfs "$@" >"$SANDBOX/r.out" 2>"$SANDBOX/r.err"
    check "$what exits 1" test $? -eq 1
    check "$what prints nothing on stdout" test ! -s "$SANDBOX/r.out"
    jq_check "$what is a structured error saying so" ".code == 1 and (.error | test(\"$pattern\"))" "$SANDBOX/r.err"
}
refused1 "ls of a missing path" "no such file" "$img" ls /missing
refused1 "read of a missing path" "no such file" "$img" read /missing
refused1 "read of a directory" "is a directory" "$img" read /dir
refused1 "read of a symlink" "symlink to hello.txt" "$img" read /link
refused1 "a missing image" "open " "$SANDBOX/absent.img" ls /
head -c 4096 "$img" >"$SANDBOX/cut.img"
refused1 "a truncated image" "Btrfs" "$SANDBOX/cut.img" get

# A superblock whose magic is gone from every copy: no volume, and no
# bytes from it. The primary is at 64 KiB and the first mirror at 64 MiB;
# a 512 MiB volume has no copy at 256 GiB.
dd if="$img" of="$SANDBOX/nomagic.img" bs=1048576 conv=sparse,notrunc status=none 2>/dev/null ||
    dd if="$img" of="$SANDBOX/nomagic.img" bs=1048576 conv=sparse,notrunc 2>/dev/null
for at in $((65536 + 64)) $((67108864 + 64)); do
    printf 'XXXXXXXX' | dd of="$SANDBOX/nomagic.img" bs=1 seek="$at" conv=notrunc status=none 2>/dev/null ||
        printf 'XXXXXXXX' | dd of="$SANDBOX/nomagic.img" bs=1 seek="$at" conv=notrunc 2>/dev/null
done
refused1 "get of a volume with no superblock magic" "superblock|Btrfs" "$SANDBOX/nomagic.img" get
refused1 "read of a volume with no superblock magic" "Btrfs" "$SANDBOX/nomagic.img" read /hello.txt

finish
