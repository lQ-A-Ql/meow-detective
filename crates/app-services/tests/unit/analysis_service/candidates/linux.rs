use super::super::common::{evidence_path_matches, normalize_evidence_path};
use super::LINUX_ARTIFACTS_CATEGORY_DEF;

fn matches_linux_candidate_patterns(path: &str) -> bool {
    let normalized = normalize_evidence_path(path);
    evidence_path_matches(&normalized, LINUX_ARTIFACTS_CATEGORY_DEF.patterns)
}

#[test]
fn baota_panel_paths_are_linux_artifact_candidates() {
    for path in [
        "/www/server/panel/vhost/nginx/example.com.conf",
        "/www/server/panel/vhost/apache/example.com.conf",
        "/www/server/nginx/conf/nginx.conf",
        "/www/server/apache/conf/httpd.conf",
        "/www/wwwlogs/example.com.log",
        "/www/wwwlogs/example.com.error.log",
        "/www/wwwlogs/example.com.log.1",
    ] {
        assert!(
            matches_linux_candidate_patterns(path),
            "{path} should match the Linux artifact candidate patterns"
        );
    }
}

#[test]
fn baota_lookalike_paths_do_not_overmatch() {
    for path in [
        // Non-config / non-log payloads under the same roots stay out.
        "/www/server/panel/vhost/nginx/example.com.conf.bak",
        "/www/server/nginx/conf/mime.types",
        "/www/wwwlogs/example.com.access.json",
        // Site content itself is not a config/log candidate.
        "/www/wwwroot/example.com/index.html",
    ] {
        assert!(
            !matches_linux_candidate_patterns(path),
            "{path} should not match the Linux artifact candidate patterns"
        );
    }
}

#[test]
fn docker_overlay_paths_still_match_host_layout_candidates() {
    // Extraction intentionally keeps running for overlay content; the
    // overlayContext attr separates container records from host records.
    assert!(matches_linux_candidate_patterns(
        "/var/lib/docker/overlay2/abc123/diff/etc/cron.d/container-job"
    ));
}
