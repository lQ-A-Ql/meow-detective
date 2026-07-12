mod cache;
mod grouping;
mod nodes;
mod ordering;
mod persistence;
mod presentation;
mod projection;
mod scope;
mod snapshot;
mod timeline_context;

pub use snapshot::{
    get_correlation_snapshot, get_correlation_snapshot_for_case,
    get_correlation_snapshot_incremental,
};

pub use cache::invalidate_correlation_cache;

pub(crate) use grouping::{build_rule_groups, build_source_groups};
pub(crate) use nodes::{
    build_artifact_node, build_artifact_provenance, build_file_node, build_file_node_for_entry,
    build_timeline_node, build_timeline_provenance,
};
pub(crate) use persistence::persist_correlation_edges;
pub(crate) use presentation::{
    artifact_guarantee_level, build_lead_jumps, group_caveats, group_confidence, group_summary,
    group_title, rule_group_caveats, rule_group_confidence, rule_group_match_signals,
    rule_group_summary, source_group_match_signals, timeline_guarantee_level,
};
pub(crate) use projection::{append_rule_group, append_source_group};
pub(crate) use scope::{empty_snapshot, finalize_snapshot_counts, merge_source_snapshot};
pub(crate) use timeline_context::derive_rule_timeline_signals;
