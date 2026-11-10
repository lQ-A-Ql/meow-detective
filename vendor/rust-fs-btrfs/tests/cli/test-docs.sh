# Every installed tool has its man page and its zsh, bash and fish
# completions where the install prefix keeps them -- share/ beside the bin/
# that PATH found the tool in, the layout of the release tarball and of a
# Homebrew prefix alike -- and each page names every subcommand the tool's
# --help lists.
source "$(dirname "$0")/lib.sh"

for name in $(rust-fs-btrfs generate names) rust-fs-btrfs; do
    path="$(command -v "$name" 2>/dev/null || true)"
    if [ -z "$path" ]; then
        fail "$name is not on PATH"
        continue
    fi
    share="$(cd "$(dirname "$path")/.." && pwd)/share"
    case "$name" in mkfs.* | fsck.*) section=8 ;; *) section=1 ;; esac
    page="$share/man/man$section/$name.$section"
    check "$name has a man page at $page" test -s "$page"
    for verb in $("$name" --help | awk '/^Commands:/ { on = 1; next } on && /^  [a-z]/ { print $1 } on && !/^  / { on = 0 }'); do
        [ "$verb" = help ] && continue
        check "$name's man page mentions $verb" grep -q -- "$verb" "$page"
    done
    check "$name has a zsh completion" test -s "$share/zsh/site-functions/_$name"
    check "$name has a bash completion" test -s "$share/bash-completion/completions/$name"
    check "$name has a fish completion" test -s "$share/fish/vendor_completions.d/$name.fish"
done

finish
