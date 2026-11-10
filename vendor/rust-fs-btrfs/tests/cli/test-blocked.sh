# The verbs this library cannot do still exist and answer, by name: status
# 3, `not implemented` and the reason, nothing on stdout. A script moved
# here from a filesystem that can do them fails loudly instead of meaning
# something else. None of them opens the image, so none needs one.
source "$(dirname "$0")/lib.sh"

img="$SANDBOX/never-opened.img"

# blocked VERB... -- WHY: status 3, nothing on stdout, `not implemented`
# naming WHY.
blocked() {
    local why="${*: -1}"
    local args=("${@:1:$#-2}")
    fs.btrfs "$img" "${args[@]}" >"$SANDBOX/b.out" 2>"$SANDBOX/b.err"
    check "${args[*]} exits 3" test $? -eq 3
    check "${args[*]} prints nothing on stdout" test ! -s "$SANDBOX/b.out"
    jq_check "${args[*]} says not implemented, naming $why" \
        ".code == 3 and (.error | startswith(\"not implemented\")) and (.error | contains(\"$why\"))" \
        "$SANDBOX/b.err"
}

blocked mkdir /d -- "rust-fs-btrfs#262"
blocked resize 20G -- "resize"
blocked resize 20G --force -- "resize"

# A read-only key is refused with the same status; an unknown one is a
# wrong command line.
fs.btrfs "$img" set total_bytes 1 >"$SANDBOX/ro.out" 2>"$SANDBOX/ro.err"
check "set total_bytes exits 3" test $? -eq 3
jq_check "set total_bytes says it is read-only" '.code == 3 and (.error | test("read-only"))' "$SANDBOX/ro.err"
fs.btrfs "$img" set colour blue >"$SANDBOX/uk.out" 2>"$SANDBOX/uk.err"
check "set of an unknown key exits 2" test $? -eq 2
jq_check "set of an unknown key is a structured usage error" '.code == 2' "$SANDBOX/uk.err"

# --text: `fs.btrfs: <message>`, for a person.
fs.btrfs --text "$img" mkdir /d >/dev/null 2>"$SANDBOX/t.err"
check "mkdir --text exits 3" test $? -eq 3
check "mkdir --text says so for a person" grep -q '^fs.btrfs: not implemented: ' "$SANDBOX/t.err"

finish
