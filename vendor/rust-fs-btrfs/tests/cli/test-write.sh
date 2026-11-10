# fs.btrfs write, as installed, on a copy of the kernel-made CLI volume:
# the one write this library can make -- an existing NODATACOW file
# overwritten in place at the same length -- round-trips byte for byte,
# and every other write is refused with the library's reason and status 3
# (or 1 for a path that is no file at all), leaving the image untouched.
#
# What btrfs-progs and the kernel make of the written volume is checked in
# tests/cli_write_kernel.rs, in the harness VM.
source "$(dirname "$0")/lib.sh"

src="$REPO/test-disks/cli/btrfs-cli.img"
manifest="$REPO/test-disks/cli/btrfs-cli.manifest"
if [ ! -s "$src" ] || [ ! -s "$manifest" ]; then
    fail "test-disks/cli/ is missing: the fixtures are built by \`chore fixtures\` (in the harness VM). Nothing skips."
    finish
fi

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1; else shasum -a 256 | cut -d' ' -f1; fi
}

# sparse_copy FROM TO
sparse_copy() {
    dd if="$1" of="$2" bs=1048576 conv=sparse status=none 2>/dev/null ||
        dd if="$1" of="$2" bs=1048576 conv=sparse 2>/dev/null
}

img="$SANDBOX/write.img"
sparse_copy "$src" "$img"
size="$(awk -F'\t' '$2 == "/nocow/data.bin" { print $3 }' "$manifest")"
check "the manifest gives /nocow/data.bin a size ($size)" test -n "$size"

# The write this library can make.
head -c "$size" /dev/urandom >"$SANDBOX/new.bin"
status=0
fs.btrfs "$img" write /nocow/data.bin <"$SANDBOX/new.bin" >"$SANDBOX/w.json" 2>"$SANDBOX/w.err" || status=$?
check "write /nocow/data.bin exits 0, not $status ($(cat "$SANDBOX/w.err"))" test "$status" -eq 0
jq_check "write reports the path and $size bytes, not created" \
    ".path == \"/nocow/data.bin\" and .bytes == $size and .created == false" "$SANDBOX/w.json"
check "read /nocow/data.bin is what was written" cmp -s "$SANDBOX/new.bin" <(fs.btrfs "$img" read /nocow/data.bin)
fs.btrfs "$img" ls /nocow >"$SANDBOX/ls.json" 2>/dev/null
jq_check "the file keeps its size" ".[0].name == \"data.bin\" and .[0].size == $size" "$SANDBOX/ls.json"
want="$(awk -F'\t' '$2 == "/hello.txt" { print $4 }' "$manifest")"
check "the file beside it is untouched" test "$(fs.btrfs "$img" read /hello.txt | sha256_of)" = "$want"
check "the volume is not dirty after the write" test "$(fs.btrfs "$img" get dirty --text)" = false
fs.btrfs --text "$img" write /nocow/data.bin <"$SANDBOX/new.bin" >"$SANDBOX/t.out" 2>/dev/null
check "write --text says what it did" grep -q '^overwrote /nocow/data.bin' "$SANDBOX/t.out"

# Everything else is refused, and the image is left as it was.
sparse_copy "$img" "$SANDBOX/before.img"

# refused CODE PATTERN PATH STDIN-FILE
refused() {
    local code="$1" pattern="$2" path="$3" input="$4"
    local status=0
    fs.btrfs "$img" write "$path" <"$input" >"$SANDBOX/r.out" 2>"$SANDBOX/r.err" || status=$?
    check "write $path ($(basename "$input")) exits $code, not $status" test "$status" -eq "$code"
    check "write $path prints nothing on stdout" test ! -s "$SANDBOX/r.out"
    jq_check "write $path says why ($pattern)" ".code == $code and (.error | test(\"$pattern\"))" "$SANDBOX/r.err"
}
head -c "$((size + 4096))" /dev/urandom >"$SANDBOX/longer.bin"
head -c 10 /dev/urandom >"$SANDBOX/shorter.bin"
hello_size="$(awk -F'\t' '$2 == "/hello.txt" { print $3 }' "$manifest")"
random_size="$(awk -F'\t' '$2 == "/dir/random.bin" { print $3 }' "$manifest")"
head -c "$hello_size" /dev/urandom >"$SANDBOX/hello-sized.bin"
head -c "$random_size" /dev/urandom >"$SANDBOX/random-sized.bin"

refused 3 "grow the file" /nocow/data.bin "$SANDBOX/longer.bin"
refused 3 "shorter is not something this library does yet" /nocow/data.bin "$SANDBOX/shorter.bin"
refused 3 "copy-on-write" /dir/random.bin "$SANDBOX/random-sized.bin"
refused 3 "copy-on-write|inline" /hello.txt "$SANDBOX/hello-sized.bin"
refused 3 "^not implemented: .*creating a file is not something this library does yet" /new.txt "$SANDBOX/shorter.bin"
refused 3 "^not implemented: .*creating a file" /dir/new.txt "$SANDBOX/shorter.bin"
refused 3 "subvolume or snapshot" /vol/inside.txt "$SANDBOX/shorter.bin"
refused 1 "is a directory" /dir "$SANDBOX/shorter.bin"
refused 1 "not a regular file" /link "$SANDBOX/shorter.bin"
check "the refusals left the image as it was" cmp -s "$img" "$SANDBOX/before.img"

# mkdir waits on the same namespace work, #262.
fs.btrfs "$img" mkdir /newdir >"$SANDBOX/m.out" 2>"$SANDBOX/m.err"
check "mkdir exits 3" test $? -eq 3
jq_check "mkdir names what blocks it" '.code == 3 and (.error | test("rust-fs-btrfs#262"))' "$SANDBOX/m.err"

finish
