mod error;
mod export;
mod facets;
mod ingestion;
mod pagination;
mod projection;
mod projection_graph;
mod query;

pub use error::TimelineServiceError;
pub use export::{
    query_timeline_filtered_for_case_instrumented, query_timeline_filtered_instrumented,
    query_timeline_for_case_instrumented, query_timeline_instrumented, InstrumentedPage,
};
pub use facets::get_timeline_facets_for_case;
pub use pagination::{
    query_timeline_aggregated, query_timeline_filtered_for_case, query_timeline_for_case,
};
pub use projection::{
    materialize_file_activity, materialize_file_activity_unknown,
    materialize_file_activity_unknown_with_cancel,
    materialize_file_activity_unknown_with_cancel_and_identity,
    materialize_file_activity_with_identity, project_and_store_file_activity,
    TimelineProjectionStats,
};
pub use query::{
    count_timeline_events_for_case, get_timeline_event_by_id, get_timeline_event_by_id_for_case,
    query_timeline, query_timeline_filtered, TimelineQuery,
};

pub(crate) use ingestion::retain_analysis_events;
