# Registry browser P0 preview

All IDs, paths, users and byte patterns in this folder are synthetic. The 70 MiB
SOFTWARE size is a metadata scenario; this mock does not read a real 70 MiB hive.

Run `pnpm --dir frontend dev:mock`, then open
`/mocks/registry-browser/preview.html` on that dev server.

The entry rejects non-mock and production execution. Production app code never
imports this directory. No native command handlers are registered by P0.

Cases: default value with empty string, missing default value, empty key,
duplicate NTUSER.DAT names with different file identities, corrupt key, partial
value coverage, unknown native value type, exact u64 QWORD and FILETIME, a 128 KiB
binary value returned in 64 KiB windows, and a locked hive. Source absence and
read failure are selectable in the preview toolbar.
