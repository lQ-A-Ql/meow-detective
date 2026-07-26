# Test Artifacts

This directory contains test fixture files for artifact parser testing.

## Prefetch Files

- `test.pf` - Minimal valid Prefetch v30 file for testing

## LNK Files

- `test.lnk` - Minimal valid Shell Link file for testing

## Recycle Bin Files

- `$I12345.txt` - Sample recycle bin info file

## Usage in Tests

```rust
fn test_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/artifacts")
        .join(name)
}
```
