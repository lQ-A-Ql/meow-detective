# public-small registry fixtures

## Source

The hives referenced by this expected JSON are the synthetic fixtures generated
by `crates/testing/src/builders/registry.rs`:

- `../logical/Windows/System32/config/SYSTEM`
- `../logical/Windows/System32/config/SOFTWARE`

These hives are deliberately minimal and only contain keys required by the
analysis system-info tests.

## Visibility

public-small — suitable for default CI.

## Coverage

- Canonical registry extraction path (`extract_registry_candidate`).
- SYSTEM hive: `RegistryValue` artifacts for `computerName` and `timezone`.
- SOFTWARE hive: `RegistryValue` artifacts for product info (`productName`,
  `currentBuild`, `displayVersion`, `installDate`, `registeredOwner`,
  `productId`).
- Hive meta artifact (`RegistryHive`) emitted for every recognized hive.
- Warning governance: synthetic fixtures produce zero warnings.

## Known limitations

- No SAM, SECURITY, NTUSER, USRCLASS, or Amcache coverage in public-small.
- Services, network adapters, USBSTOR, MountedDevices, ShutdownTime, ShimCache,
  LSA packages, installed software, run keys, Winlogon, NetworkList, and
  AppCompatFlags Layers are not populated in the tiny synthetic hives; they are
  covered by unit tests and real-sample regression tests instead.

## Expected JSON

`expected.json` follows the contract in `docs/expected-json-contract.md` and is
consumed by `crates/app-services/tests/registry_fixture_expected_test.rs`.
