//! L5 rollback-switch mode selection.
//!
//! The end-to-end equivalence (Inline mode actually running timeline
//! materialization and artifact extraction inside the job) is covered by the
//! existing Inline `execute_import_job` integration fixtures; this module
//! pins only the pure mode selection the shell derives from
//! `capabilityLifecycle.enabled`. A real E01 round-trip with the switch off
//! remains design-only/unverified here.

use super::*;

#[test]
fn disabled_switch_selects_inline_rollback_mode() {
    assert_eq!(
        SingleImportMode::from_enabled(false),
        SingleImportMode::Inline
    );
}

#[test]
fn enabled_switch_selects_detached_default_mode() {
    assert_eq!(
        SingleImportMode::from_enabled(true),
        SingleImportMode::Detached
    );
}
