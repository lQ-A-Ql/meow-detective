# Scripts Directory

Organized collection of development, testing, and deployment scripts.

## Directory Structure

### `testing/`
Test and validation scripts for the iSCSI target implementation.

- **run-integration-tests.sh** - Run all Rust integration tests
- **test-status-codes.sh** - Test RFC 3720 status code responses
- **test-with-iscsiadm.sh** - Test compatibility with Linux iscsiadm client
- **test-with-python-client.py** - Python client compatibility tests
- **run-all-tests.sh** - Run all test suites
- **test-rust.sh** - Rust-specific test runner
- **run-tests.sh** - Main test orchestration script
- **validate-against-tgtd.sh** - Validate behavior against TGTD reference implementation
- **test-tgtd.sh** - Test against TGTD
- **debug_tc008.sh**, **test-tc008.sh**, **test_tc008.sh** - TC008 test case debugging

### `setup/`
VM, Docker, and environment setup scripts for development and deployment.

- **vm-setup.sh** - Set up local VM for testing
- **remote-vm-setup.sh** - Set up remote VM environment
- **docker-setup-vsprod.sh** - Docker environment setup for vsprod
- **run-auto-fix-vsprod.sh** - Run auto-fix in vsprod environment
- **run-implement-vsprod.sh** - Run implementation scripts in vsprod
- **install-debug-tools.sh** - Install debugging and development tools

### `tools/`
Development and debugging utilities.

- **auto-fix-loop.sh** - Automated issue fixing loop
- **fix-issue.sh** - Fix GitHub issues automatically
- **implement-issue.sh** - Implement GitHub issues automatically
- **strace_claude.sh** - Trace Claude execution with strace
- **watch_claude.sh** - Monitor Claude processes

## Usage

Most scripts can be run directly from the repository root:

```bash
# Run integration tests
./scripts/testing/run-integration-tests.sh

# Test with iscsiadm
./scripts/testing/test-with-iscsiadm.sh

# Set up development VM
./scripts/setup/vm-setup.sh
```

## Requirements

- Bash 4.0+
- Python 3.6+ (for Python scripts)
- Root/sudo access (for some system-level tests)
- open-iscsi package (for iscsiadm tests)
