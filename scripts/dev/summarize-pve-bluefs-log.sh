#!/usr/bin/env bash
set -euo pipefail

for file in "$@"; do
  printf '==== %s\n' "$file"
  grep -E '^ log_fnode|^ 0x[0-9a-f]+: txn|op_jump seq|log file size' "$file"
  printf 'dirs=%s\n' "$(grep -c 'op_dir_create' "$file")"
  printf 'links=%s\n' "$(grep -c 'op_dir_link' "$file")"
  printf 'updates=%s\n' "$(grep -c 'op_file_update  ' "$file")"
  printf 'increments=%s\n' "$(grep -c 'op_file_update_inc' "$file")"
  grep 'op_dir_link' "$file" | sed -E 's#.*op_dir_link  ##' | cut -d/ -f1 |
    sort | uniq -c
done
