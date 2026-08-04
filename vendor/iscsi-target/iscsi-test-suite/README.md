# iSCSI Target Test Suite

A comprehensive C-based test suite for validating iSCSI target implementations against RFC 7143 using libiscsi.

## Purpose

This test suite provides black-box validation of iSCSI targets through protocol-level testing. It's designed to be completely independent of any specific target implementation, making it suitable for:

- Validating new iSCSI target implementations
- Regression testing during development
- Comparing different target implementations
- Pre-release conformance verification
- Troubleshooting interoperability issues

## Features

- **Comprehensive Coverage**: Tests discovery, authentication, SCSI commands, I/O operations, error handling, and edge cases
- **RFC 7143 Conformance**: Tests based on iSCSI protocol specification
- **Target-Agnostic**: Works with any iSCSI target (LIO, TGT, custom implementations)
- **Detailed Reporting**: Clear test results with actionable failure information
- **Configurable**: All test parameters in simple INI file
- **Data Integrity Focus**: Extensive tests for data corruption and consistency

## Requirements

### Build Dependencies
- C compiler (GCC or Clang)
- libiscsi development headers
- POSIX-compatible system (Linux, FreeBSD, macOS)
- Make

### Runtime Requirements
- Access to iSCSI target (network reachable)
- Target credentials (if authentication enabled)
- At least one LUN configured on target

## Installation

### Installing libiscsi

**Debian/Ubuntu:**
```bash
sudo apt-get install libiscsi-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install libiscsi-devel
```

**From Source:**
```bash
git clone https://github.com/sahlberg/libiscsi.git
cd libiscsi
./autogen.sh
./configure
make
sudo make install
```

### Building the Test Suite

```bash
make
```

This produces the `iscsi-test-suite` executable.

## Configuration

Edit `config/test_config.ini` with your target details:

```ini
[target]
portal = 192.168.1.100:3260
iqn = iqn.2024-12.net.example:storage.target01
lun = 0

[authentication]
auth_method = none

[test_parameters]
block_size = 512
large_transfer_blocks = 1024
timeout = 30
stress_iterations = 100

[options]
verbosity = 1
stop_on_fail = false
generate_report = true
```

### Configuration Options

**[target]**
- `portal`: IP:port of iSCSI target
- `iqn`: Target IQN (leave empty for discovery)
- `lun`: LUN number to test (default: 0)

**[authentication]**
- `auth_method`: none, chap, or mutual_chap
- `username`, `password`: CHAP credentials
- `mutual_username`, `mutual_password`: Mutual CHAP credentials

**[test_parameters]**
- `block_size`: Block size for I/O tests (typically 512 or 4096)
- `large_transfer_blocks`: Number of blocks for large transfers
- `timeout`: Operation timeout in seconds
- `stress_iterations`: Iterations for stress tests

**[options]**
- `verbosity`: 0=errors only, 1=normal, 2=verbose, 3=debug
- `stop_on_fail`: Stop testing on first failure
- `generate_report`: Create detailed report file

## Usage

### Basic Usage
```bash
./iscsi-test-suite config/test_config.ini
```

### Command Line Options
```bash
# Verbose output (shows all test details)
./iscsi-test-suite -v config/test_config.ini

# Quiet mode (only failures shown)
./iscsi-test-suite -q config/test_config.ini

# Stop on first failure
./iscsi-test-suite -f config/test_config.ini

# Run specific test category
./iscsi-test-suite -c io config/test_config.ini

# Available categories: discovery, login, auth, commands, io, multiconn, error, edge, integrity
```

## Understanding Test Results

### Console Output

Tests are organized by category. Each test shows:
- Test ID and name
- Result: [PASS], [FAIL], [SKIP], or [ERROR]
- Duration in seconds

Example:
```
[I/O Operation Tests]
  TI-001: Single Block Read        [PASS]  (0.012s)
  TI-002: Single Block Write       [PASS]  (0.015s)
  TI-003: Multi-Block Read         [FAIL]  (0.234s)
    └─ Data mismatch at block 5
```

### Test Result Meanings

- **PASS**: Test completed successfully, target behaved correctly
- **FAIL**: Test found non-conformant behavior or bug
- **SKIP**: Test skipped (usually due to configuration, e.g., no auth configured)
- **ERROR**: Test couldn't run (network issue, setup problem)

### Detailed Reports

When `generate_report = true`, detailed reports are saved to `reports/test_report_YYYYMMDD_HHMMSS.txt`

Reports include:
- Test configuration
- Individual test results with details
- Failure explanations and recommendations
- Summary by category
- Overall statistics

## Test Categories

1. **Discovery Tests**: Target discovery and enumeration
2. **Login/Logout Tests**: Session establishment and parameter negotiation
3. **Authentication Tests**: CHAP and mutual CHAP validation
4. **SCSI Command Tests**: Basic SCSI command set
5. **I/O Operation Tests**: Read/write operations and data integrity
6. **Multi-Connection Tests**: Multiple connections per session
7. **Error Handling Tests**: Network failures, timeouts, malformed PDUs
8. **Edge Cases**: Boundary conditions and stress tests
9. **Data Integrity Tests**: Long-running stability and crash recovery

## Interpreting Failures

### Common Failure Scenarios

**Parameter Negotiation Failures**
- Target accepting invalid values
- Incompatible parameter combinations
- Missing mandatory parameters

**I/O Failures**
- Data corruption (most serious)
- Incorrect transfer sizes
- Timeout issues
- Performance problems

**Error Handling Failures**
- Crashes on malformed PDUs
- Resource leaks
- Improper connection cleanup
- Missing error recovery

### What to Do

1. Read the detailed report for the failed test
2. Check if it's a target bug or configuration issue
3. Rerun specific category to isolate problem
4. Use verbose mode (-v) for detailed protocol traces
5. Fix target implementation and retest

## Exit Codes

- `0`: All tests passed
- `1`: One or more tests failed
- `2`: Configuration error
- `3`: Target unreachable
- `4`: Setup error (libiscsi issue, etc.)

## Troubleshooting

### Target Not Reachable
```
Error: Cannot connect to target at 192.168.1.100:3260
```
**Solutions:**
- Verify target is running: `ss -tulpn | grep 3260`
- Check firewall rules
- Verify portal address in config
- Test with iscsiadm: `iscsiadm -m discovery -t st -p 192.168.1.100`

### Authentication Failures
```
TL-001: Basic Login [FAIL] - Authentication required
```
**Solutions:**
- Check if target requires authentication
- Verify credentials in config file
- Try discovery first to see auth requirements

### Permission Errors
```
Error: Cannot bind to initiator name
```
**Solutions:**
- Run as root or with CAP_NET_ADMIN
- Check /etc/iscsi/initiatorname.iscsi

### libiscsi Not Found
```
/usr/bin/ld: cannot find -liscsi
```
**Solutions:**
- Install libiscsi-dev package
- Check library path: `ldconfig -p | grep iscsi`
- Add path to LD_LIBRARY_PATH if installed in non-standard location

## Extending the Test Suite

To add new tests:

1. Add test function to appropriate test_*.c file
2. Register test in category
3. Follow test function signature: `test_result_t test_func(struct iscsi_context *iscsi, test_report_t *report)`
4. Use helper functions from utils.h
5. Document test in TESTING_GUIDE.md

## Performance Considerations

- Full test suite typically completes in 2-5 minutes
- Large transfer tests may take longer depending on network/target
- Stress tests can be adjusted via `stress_iterations` config
- Use `-c` to run specific categories for faster iteration

## CI/CD Integration

The test suite can be integrated into continuous integration:

```bash
#!/bin/bash
# CI test script
./iscsi-test-suite -q config/test_config.ini
EXIT_CODE=$?

if [ $EXIT_CODE -eq 0 ]; then
    echo "All tests passed"
    exit 0
else
    echo "Tests failed, check reports/"
    cat reports/test_report_*.txt
    exit 1
fi
```

## Known Limitations

- Does not test iSER (iSCSI Extensions for RDMA)
- Does not test iSNS (iSCSI Name Service) integration
- Performance benchmarking is basic (not fio-level detail)
- No automated multi-initiator testing from single host
- Target-side state inspection requires manual verification

## Contributing

When contributing test cases:
1. Ensure tests are target-agnostic
2. Base tests on RFC requirements
3. Provide clear pass/fail criteria
4. Include detailed failure messages
5. Document expected behavior

## License

[Specify license here]

## References

- RFC 7143: Internet Small Computer System Interface (iSCSI) Protocol
- RFC 3720: Internet Small Computer Systems Interface (iSCSI)
- RFC 5046: Internet Small Computer System Interface (iSCSI) Extensions for Remote Direct Memory Access (RDMA)
- libiscsi: https://github.com/sahlberg/libiscsi

## Support

For issues with:
- **Test suite itself**: [Contact or issue tracker]
- **libiscsi**: https://github.com/sahlberg/libiscsi/issues
- **iSCSI protocol**: IETF STORM working group
- **Your target implementation**: [Your support channel]
