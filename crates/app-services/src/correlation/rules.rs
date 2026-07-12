mod builders;
mod context;
mod extraction;
mod matching;

pub(crate) use builders::build_artifact_rule_matches;
pub(crate) use context::{
    rule_match_paths, rule_match_text_needles, rule_match_timestamps, timeline_path_candidates,
    timeline_text_candidates,
};
pub(crate) use extraction::{extract_file_name_candidates, extract_path_candidates};
pub(crate) use matching::{
    basename, dedup_rule_matches, find_best_file_by_name, find_best_file_by_path, looks_like_path,
    normalize_path,
};
