## Registry Module Completion

### SAM RID Fix
- Root cause: SAM Names key default value stores RID in VK cell data_type field
- Fix: check data_type >= 500 && data_type < 2000 (matches ForensicsTool Go approach)
- Verified: liuyang (5 users) + jc2 (4 users) both extract correct RIDs

### Registry Module Refactor
- Split lookup.rs (4,500L monolith) into 9 files:
  mod.rs + types.rs + reader.rs + utf16.rs + txlog_util.rs +
  system.rs + software.rs + ntuser.rs + sam.rs
- All 9 files under 1,200L

### New Capabilities
- SAM: UserF/V parsing, BootKey extraction, DomainAccountF password policy
- NTUSER: UserAssist ROT13, Run keys, RecentDocs, MountPoints
- Deleted cell recovery: hbin free cell scanning
- Txlog integration: *_with_txlog variants

### Quality Gates
- 115 registry tests, 1,757 total Rust tests
- cargo fmt + clippy clean
- SAM verified on 2 real E01 samples
